import CTtoryeot
import Foundation

public enum Ttoryeot {
    public struct Warning: Decodable, Equatable, Sendable {
        public let kind: String
        public let section: Int?
        public let detail: String
    }

    public struct FontFaces: Decodable, Equatable, Sendable {
        public let provided: Int
        public let documentEmbedded: Int
        public let hasEmbeddableFace: Bool
    }

    public struct Failure: Error, Decodable, Equatable, Sendable {
        public let kind: String
        public let detail: String
    }

    public struct Output: Sendable {
        public let pdf: Data
        public let pages: Int
        public let warnings: [Warning]
        public let fontFaces: FontFaces
    }

    private struct Metadata: Decodable {
        let schema: Int
        let ok: Bool
        let pages: Int?
        let warnings: [Warning]?
        let fontFaces: FontFaces?
        let error: Failure?
    }

    public static func render(data: Data, password: String? = nil, fonts: [Data]) throws -> Output {
        guard ttoryeot_abi_version() == UInt32(TTORYEOT_ABI_VERSION) else {
            throw Failure(kind: "AbiMismatch", detail: "Renderer ABI version does not match")
        }

        let passwordLength = password?.utf8.count ?? 0
        var passwordBytes = password.map { Array($0.utf8) } ?? []
        if password != nil && passwordBytes.isEmpty { passwordBytes.append(0) }

        var result: OpaquePointer?
        let status = data.withUnsafeBytes { documentBytes in
            passwordBytes.withUnsafeBufferPointer { passwordBuffer in
                withFontBytes(fonts[...]) { ranges in
                    ranges.withUnsafeBufferPointer { fontBuffer in
                        ttoryeot_render(
                            TtoryeotBytes(
                                ptr: documentBytes.baseAddress?.assumingMemoryBound(to: UInt8.self),
                                len: documentBytes.count
                            ),
                            TtoryeotBytes(
                                ptr: password == nil ? nil : passwordBuffer.baseAddress,
                                len: passwordLength
                            ),
                            fontBuffer.baseAddress,
                            fontBuffer.count,
                            &result
                        )
                    }
                }
            }
        }
        guard status == 0, let result else {
            throw Failure(kind: "InvalidArgument", detail: "Renderer rejected its input")
        }
        defer { ttoryeot_result_free(result) }

        let metadataData = try copy(ttoryeot_result_metadata(result))
        guard let metadata = try? JSONDecoder().decode(Metadata.self, from: metadataData),
              metadata.schema == 1 else {
            throw Failure(kind: "MalformedResult", detail: "Cannot decode renderer metadata")
        }
        if !metadata.ok {
            throw metadata.error ?? Failure(kind: "MalformedResult", detail: "Missing error details")
        }
        guard let pages = metadata.pages, let warnings = metadata.warnings,
              let fontFaces = metadata.fontFaces else {
            throw Failure(kind: "MalformedResult", detail: "Missing renderer output")
        }
        return Output(pdf: try copy(ttoryeot_result_pdf(result)), pages: pages,
                      warnings: warnings, fontFaces: fontFaces)
    }

    /// Pins each font's own storage for the call, so mapped font files are passed without copying.
    private static func withFontBytes<R>(
        _ fonts: ArraySlice<Data>, _ ranges: [TtoryeotBytes] = [], _ body: ([TtoryeotBytes]) -> R
    ) -> R {
        guard let font = fonts.first else { return body(ranges) }
        return font.withUnsafeBytes { bytes in
            let range = TtoryeotBytes(
                ptr: bytes.baseAddress?.assumingMemoryBound(to: UInt8.self), len: bytes.count)
            return withFontBytes(fonts.dropFirst(), ranges + [range], body)
        }
    }

    private static func copy(_ bytes: TtoryeotBytes) throws -> Data {
        if bytes.len == 0 { return Data() }
        guard let ptr = bytes.ptr else {
            throw Failure(kind: "MalformedResult", detail: "Missing result bytes")
        }
        return Data(bytes: ptr, count: bytes.len)
    }
}
