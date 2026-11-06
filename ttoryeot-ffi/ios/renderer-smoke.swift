import Foundation
import PDFKit
import CHWPCore

@main
struct RendererSmoke {
    static func main() throws {
        let args = CommandLine.arguments
        if args.count >= 5 && args[1] == "render" {
            let input = try Data(contentsOf: URL(fileURLWithPath: args[2]))
            let font = try Data(contentsOf: URL(fileURLWithPath: args[3]))
            let output = try Ttoryeot.render(data: input, password: args.count > 5 ? args[5] : nil,
                                             fonts: [font])
            let pdf = try checkPDF(output)
            if args.count > 6 {
                let text = pdf.string?.filter { !$0.isWhitespace } ?? ""
                guard text.contains(args[6]), args.count < 8 || output.pages == Int(args[7]) else {
                    fatalError("Public PDF is missing expected text or page count")
                }
            }
            try output.pdf.write(to: URL(fileURLWithPath: args[4]))
            print("pages=\(output.pages) warnings=\(output.warnings.count) providedFaces=\(output.fontFaces.provided) documentEmbeddedFaces=\(output.fontFaces.documentEmbedded) textCharacters=\(pdf.string?.count ?? 0)")
            return
        }
        guard args.count == 7 && args[1] == "fixtures" else {
            fatalError("fixtures <hwpx> <encrypted-hwp> <unsupported-hwp> <font> <output-dir> or render <document> <font> <pdf> [password]")
        }
        let hwpx = try Data(contentsOf: URL(fileURLWithPath: args[2]))
        let hwp = try Data(contentsOf: URL(fileURLWithPath: args[3]))
        let unsupported = try Data(contentsOf: URL(fileURLWithPath: args[4]))
        let font = try Data(contentsOf: URL(fileURLWithPath: args[5]))
        let outputDirectory = URL(fileURLWithPath: args[6], isDirectory: true)
        try FileManager.default.createDirectory(at: outputDirectory, withIntermediateDirectories: true)

        guard hwp_core_abi_version() == UInt32(HWP_CORE_ABI_VERSION) else {
            fatalError("Parser ABI version does not match")
        }
        var parserResult = HwpCoreBuf(ptr: nil, len: 0)
        let parserStatus = hwpx.withUnsafeBytes { bytes in
            hwp_core_parse(bytes.baseAddress?.assumingMemoryBound(to: UInt8.self), bytes.count,
                           UInt32(HWP_CORE_HINT_AUTO), &parserResult)
        }
        guard parserStatus == 0, let parserBytes = parserResult.ptr, parserResult.len >= 4,
              Data(bytes: parserBytes, count: 4) == Data("HWPC".utf8) else {
            fatalError("Parser and renderer could not share the library")
        }
        hwp_core_buf_free(parserResult)

        let result = try Ttoryeot.render(data: hwpx, fonts: [font])
        let pdf = try checkPDF(result)
        let text = pdf.string?.filter { !$0.isWhitespace } ?? ""
        guard text.contains("SECTION_0"), text.contains("ALPHA"), text.contains("BETA"),
              result.pages == 11, result.fontFaces.provided > 0, hasTableColors(pdf) else {
            fatalError("HWPX PDF is missing expected table or text")
        }
        try result.pdf.write(to: outputDirectory.appendingPathComponent("synthetic-hwpx.pdf"))
        try expectFailure("PasswordProtected") { try Ttoryeot.render(data: hwp, fonts: [font]) }
        try expectFailure("WrongPassword") { try Ttoryeot.render(data: hwp, password: "", fonts: [font]) }
        try expectFailure("WrongPassword") { try Ttoryeot.render(data: hwp, password: "wrong", fonts: [font]) }
        try expectFailure("UnsupportedEncryption") { try Ttoryeot.render(data: unsupported, fonts: [font]) }
        let decrypted = try Ttoryeot.render(data: hwp, password: "12345", fonts: [font])
        let decryptedPDF = try checkPDF(decrypted)
        guard decryptedPDF.string?.contains("EncryptVersion") == true else {
            fatalError("HWP PDF is missing expected text")
        }
        try decrypted.pdf.write(to: outputDirectory.appendingPathComponent("synthetic-hwp.pdf"))
        print("HWPX pages=\(result.pages), HWP pages=\(decrypted.pages); parser, table text, passwords and unsupported encryption verified")
    }

    static func checkPDF(_ output: Ttoryeot.Output) throws -> PDFDocument {
        guard let pdf = PDFDocument(data: output.pdf), pdf.pageCount == output.pages, output.pages > 0 else {
            fatalError("PDFKit could not open the PDF or page count differs")
        }
        return pdf
    }

    static func hasTableColors(_ pdf: PDFDocument) -> Bool {
        guard let page = pdf.page(at: 0) else { return false }
        let bounds = page.bounds(for: .mediaBox)
        let width = Int(bounds.width), height = Int(bounds.height)
        guard width > 0, height > 0 else { return false }
        var pixels = [UInt8](repeating: 255, count: width * height * 4)
        return pixels.withUnsafeMutableBytes { bytes in
            guard let context = CGContext(
                data: bytes.baseAddress, width: width, height: height, bitsPerComponent: 8,
                bytesPerRow: width * 4, space: CGColorSpaceCreateDeviceRGB(),
                bitmapInfo: CGBitmapInfo.byteOrder32Big.rawValue | CGImageAlphaInfo.premultipliedLast.rawValue
            ) else { return false }
            context.setFillColor(CGColor(gray: 1, alpha: 1))
            context.fill(CGRect(x: 0, y: 0, width: width, height: height))
            page.draw(with: .mediaBox, to: context)
            var fill = 0, border = 0
            for offset in stride(from: 0, to: bytes.count, by: 4) {
                let r = Int(bytes[offset]), g = Int(bytes[offset + 1]), b = Int(bytes[offset + 2])
                if (210...240).contains(r), g >= r + 4, b >= r + 2, b < g { fill += 1 }
                if r < 100, g > r + 20, b > r + 10, b < g { border += 1 }
            }
            return fill > 100 && border > 4
        }
    }

    static func expectFailure(_ kind: String, operation: () throws -> Ttoryeot.Output) throws {
        do {
            _ = try operation()
            fatalError("Expected \(kind)")
        } catch let error as Ttoryeot.Failure {
            guard error.kind == kind else { throw error }
        }
    }
}
