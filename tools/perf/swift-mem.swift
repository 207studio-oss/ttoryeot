import Darwin
import Foundation

// swift-mem <document> <a.ttf:b.ttc> <out.pdf>: 앱처럼 글꼴을 매핑해 Swift 감싸개로 그리고 최대 메모리를 찍는다.
// SWIFT_MEM_CALLS=n이면 한 프로세스에서 n번 그려, 문서를 거듭 여는 앱의 최대 메모리를 본다.
// iOS SDK 헤더에는 선언이 없지만 시뮬레이터 libSystem에는 있다.
@_silgen_name("proc_pid_rusage")
func procPidRusage(_ pid: Int32, _ flavor: Int32, _ buffer: UnsafeMutablePointer<rusage_info_t?>) -> Int32

func footprint() -> (now: Double, peak: Double) {
    var info = rusage_info_v4()
    _ = withUnsafeMutablePointer(to: &info) {
        $0.withMemoryRebound(to: rusage_info_t?.self, capacity: 1) {
            procPidRusage(getpid(), RUSAGE_INFO_V4, $0)
        }
    }
    return (Double(info.ri_phys_footprint) / 1_048_576, Double(info.ri_lifetime_max_phys_footprint) / 1_048_576)
}

@main
struct SwiftMem {
    static func main() throws {
        let args = CommandLine.arguments
        let document = try Data(contentsOf: URL(fileURLWithPath: args[1]))
        let fonts = try args[2].split(separator: ":").map {
            try Data(contentsOf: URL(fileURLWithPath: String($0)), options: .alwaysMapped)
        }
        let caller = footprint().now
        let calls = ProcessInfo.processInfo.environment["SWIFT_MEM_CALLS"].flatMap { Int($0) } ?? 1
        for call in 0..<calls {
            let start = Date()
            let output = try Ttoryeot.render(data: document, fonts: fonts)
            let total = Date().timeIntervalSince(start) * 1000
            try output.pdf.write(to: URL(fileURLWithPath: args[3]))
            print(String(format: "{\"phase\":\"run\",\"call\":%d,\"pdf_bytes\":%d,\"total\":%.1f,\"mb_caller\":%.1f,\"mb_peak\":%.1f}",
                         call, output.pdf.count, total, caller, footprint().peak))
        }
    }
}
