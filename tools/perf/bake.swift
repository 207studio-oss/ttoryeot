// 또렷 PDF를 쵸키보드 RegisteredPDFRaster와 같은 설정으로 구워 쪽마다 시간, 크기, 화질을 JSON 한 줄씩 찍는다.
//
// swiftc -O tools/perf/bake.swift -o /tmp/bake && /tmp/bake <pdf> [작업 폴더]
// 첫 줄은 쵸키보드처럼 첫 쪽부터 끝 쪽까지 2배 JPEG 0.7로 한 번 굽는 데 걸린 시간이다(첫 쪽은 글꼴을 처음 읽는다).
// 나머지 줄은 쪽마다 한 번 데운 뒤 잰다. psnr은 같은 배율 RGBA 무손실 그림과 견준 RGB dB이고, 같으면 inf다.
// webp는 cwebp가 있을 때만 재며, 시간에 프로세스 띄우는 시간이 들어간다.

import CoreGraphics
import Foundation
import ImageIO
import UniformTypeIdentifiers

func ms(_ start: UInt64) -> Double {
    Double(DispatchTime.now().uptimeNanoseconds - start) / 1e6
}

func now() -> UInt64 { DispatchTime.now().uptimeNanoseconds }

struct Raster {
    let image: CGImage
    let drawMs: Double
    let makeMs: Double
}

func render(_ page: CGPDFPage, scale: CGFloat, gray: Bool = false) -> Raster? {
    let box = page.getBoxRect(.cropBox).intersection(page.getBoxRect(.mediaBox))
    let width = Int(ceil(box.width * scale)), height = Int(ceil(box.height * scale))
    let space = gray ? CGColorSpaceCreateDeviceGray() : CGColorSpaceCreateDeviceRGB()
    let info = gray
        ? CGImageAlphaInfo.none.rawValue
        : CGImageAlphaInfo.premultipliedLast.rawValue | CGBitmapInfo.byteOrder32Big.rawValue
    let started = now()
    guard let context = CGContext(data: nil, width: width, height: height, bitsPerComponent: 8,
                                  bytesPerRow: gray ? width : width * 4, space: space, bitmapInfo: info)
    else { return nil }
    let target = CGRect(x: 0, y: 0, width: width, height: height)
    context.setFillColor(gray ? CGColor(gray: 1, alpha: 1) : CGColor(red: 1, green: 1, blue: 1, alpha: 1))
    context.fill(target)
    context.saveGState()
    context.clip(to: target)
    context.scaleBy(x: scale, y: scale)
    let points = CGRect(x: 0, y: 0, width: CGFloat(width) / scale, height: CGFloat(height) / scale)
    context.concatenate(page.getDrawingTransform(.cropBox, rect: points, rotate: 0, preserveAspectRatio: true))
    context.clip(to: box)
    context.drawPDFPage(page)
    context.restoreGState()
    let drawMs = ms(started)
    let made = now()
    guard let image = context.makeImage() else { return nil }
    return Raster(image: image, drawMs: drawMs, makeMs: ms(made))
}

func opaque(_ image: CGImage) -> CGImage? {
    guard let context = CGContext(data: nil, width: image.width, height: image.height, bitsPerComponent: 8,
                                  bytesPerRow: image.width * 4, space: CGColorSpaceCreateDeviceRGB(),
                                  bitmapInfo: CGImageAlphaInfo.noneSkipLast.rawValue | CGBitmapInfo.byteOrder32Big.rawValue)
    else { return nil }
    context.draw(image, in: CGRect(x: 0, y: 0, width: image.width, height: image.height))
    return context.makeImage()
}

func encode(_ image: CGImage, _ type: UTType, quality: Double?) -> Data? {
    let data = NSMutableData()
    guard let dest = CGImageDestinationCreateWithData(data, type.identifier as CFString, 1, nil) else { return nil }
    var options: [CFString: Any] = [:]
    if let quality { options[kCGImageDestinationLossyCompressionQuality] = quality }
    CGImageDestinationAddImage(dest, image, options as CFDictionary)
    return CGImageDestinationFinalize(dest) ? data as Data : nil
}

func decode(_ data: Data) -> CGImage? {
    guard let source = CGImageSourceCreateWithData(data as CFData, nil) else { return nil }
    return CGImageSourceCreateImageAtIndex(source, 0, nil)
}

func rgba(_ image: CGImage, width: Int, height: Int) -> [UInt8] {
    var pixels = [UInt8](repeating: 0, count: width * height * 4)
    pixels.withUnsafeMutableBytes { buffer in
        let context = CGContext(data: buffer.baseAddress, width: width, height: height, bitsPerComponent: 8,
                                bytesPerRow: width * 4, space: CGColorSpaceCreateDeviceRGB(),
                                bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue | CGBitmapInfo.byteOrder32Big.rawValue)!
        context.setFillColor(CGColor(red: 1, green: 1, blue: 1, alpha: 1))
        context.fill(CGRect(x: 0, y: 0, width: width, height: height))
        context.interpolationQuality = .high
        context.draw(image, in: CGRect(x: 0, y: 0, width: width, height: height))
    }
    return pixels
}

func psnr(_ reference: [UInt8], _ other: [UInt8]) -> Double {
    var sum = 0.0
    var count = 0
    for i in stride(from: 0, to: reference.count, by: 4) {
        for c in 0..<3 {
            let d = Double(reference[i + c]) - Double(other[i + c])
            sum += d * d
        }
        count += 3
    }
    let mse = sum / Double(count)
    return mse == 0 ? .infinity : 10 * log10(255 * 255 / mse)
}

func cwebp() -> String? {
    ["/opt/homebrew/bin/cwebp", "/usr/local/bin/cwebp"].first { FileManager.default.isExecutableFile(atPath: $0) }
}

func webp(_ png: Data, args: [String], work: URL, tool: String) -> (Data, Double)? {
    let input = work.appendingPathComponent("in.png"), output = work.appendingPathComponent("out.webp")
    try? png.write(to: input)
    let process = Process()
    process.executableURL = URL(fileURLWithPath: tool)
    process.arguments = ["-quiet"] + args + [input.path, "-o", output.path]
    let started = now()
    guard (try? process.run()) != nil else { return nil }
    process.waitUntilExit()
    let elapsed = ms(started)
    guard process.terminationStatus == 0, let data = try? Data(contentsOf: output) else { return nil }
    return (data, elapsed)
}

let args = CommandLine.arguments
guard args.count >= 2, let document = CGPDFDocument(URL(fileURLWithPath: args[1]) as CFURL) else {
    FileHandle.standardError.write("usage: bake <pdf> [work dir]\n".data(using: .utf8)!)
    exit(2)
}
let work = URL(fileURLWithPath: args.count > 2 ? args[2] : NSTemporaryDirectory())
let name = URL(fileURLWithPath: args[1]).lastPathComponent
let tool = cwebp()

let bakeStart = now()
var bakeBytes = 0
for number in 1...document.numberOfPages {
    guard let page = document.page(at: number), let raster = render(page, scale: 2),
          let data = encode(raster.image, .jpeg, quality: 0.7) else { continue }
    bakeBytes += data.count
}
print(String(format: "{\"pdf\":\"%@\",\"bake\":\"jpeg70@2x\",\"pages\":%d,\"ms\":%.2f,\"bytes\":%d}",
             name, document.numberOfPages, ms(bakeStart), bakeBytes))

for number in 1...document.numberOfPages {
    guard let page = document.page(at: number), render(page, scale: 2) != nil,
          let full = render(page, scale: 2), let fullSmall = render(page, scale: 1.5) else { continue }
    let references = [2: rgba(full.image, width: full.image.width, height: full.image.height),
                      1: rgba(fullSmall.image, width: fullSmall.image.width, height: fullSmall.image.height)]
    var rows: [String] = []
    func row(_ variant: String, _ raster: Raster, encodeMs: Double, bytes: Int, image: CGImage?) {
        let width = raster.image.width, height = raster.image.height
        let reference = references[width == full.image.width ? 2 : 1]!
        let quality = image.map { psnr(reference, rgba($0, width: width, height: height)) } ?? .nan
        let psnrText = quality.isInfinite ? "\"inf\"" : String(format: "%.2f", quality)
        rows.append(String(format: "{\"pdf\":\"%@\",\"page\":%d,\"variant\":\"%@\",\"w\":%d,\"h\":%d,\"draw_ms\":%.2f,\"make_ms\":%.2f,\"encode_ms\":%.2f,\"bytes\":%d,\"psnr\":%@}",
                           name, number, variant, raster.image.width, raster.image.height, raster.drawMs, raster.makeMs,
                           encodeMs, bytes, psnrText))
    }
    func measure(_ variant: String, _ raster: Raster, _ prepared: CGImage, _ type: UTType, _ quality: Double?) -> Data? {
        let started = now()
        guard let data = encode(prepared, type, quality: quality) else { return nil }
        row(variant, raster, encodeMs: ms(started), bytes: data.count, image: decode(data))
        return data
    }

    _ = measure("jpeg70@2x", full, full.image, .jpeg, 0.7)
    let png = measure("png-rgba@2x", full, full.image, .png, nil)
    let started = now()
    if let rgb = opaque(full.image) {
        let convertMs = ms(started)
        let encodeStart = now()
        if let data = encode(rgb, .png, quality: nil) {
            row("png-rgb@2x", full, encodeMs: convertMs + ms(encodeStart), bytes: data.count, image: decode(data))
        }
    }
    _ = measure("heic70@2x", full, full.image, .heic, 0.7)
    if let gray = render(page, scale: 2, gray: true) {
        _ = measure("png-gray@2x", gray, gray.image, .png, nil)
    }
    _ = measure("jpeg70@1.5x", fullSmall, fullSmall.image, .jpeg, 0.7)
    _ = measure("png-rgba@1.5x", fullSmall, fullSmall.image, .png, nil)
    if let tool, let png {
        for (variant, flags) in [("webp-lossless@2x", ["-lossless", "-z", "6"]), ("webp-q80@2x", ["-q", "80"])] {
            if let (data, elapsed) = webp(png, args: flags, work: work, tool: tool) {
                row(variant, full, encodeMs: elapsed, bytes: data.count, image: decode(data))
            }
        }
    }
    print(rows.joined(separator: "\n"))
}
