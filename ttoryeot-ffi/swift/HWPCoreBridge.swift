import CHWPCore
import Foundation

public enum HWPCore {
    public struct Warning: Decodable, Equatable {
        public let kind: String
        public let section: Int?
        public let detail: String
    }

    public enum Failure: Error, Equatable {
        case notHWP
        case hwp3Unsupported
        case hwpmlUnsupported
        case passwordProtected
        case noReadableSection
        case internalError(String)
        case coreMismatch
        case malformedFrame
    }

    public static func parse(
        data: Data,
        ext: String
    ) -> Result<(document: HWPDocument, warnings: [Warning]), Failure> {
        guard hwp_core_abi_version() == UInt32(HWP_CORE_ABI_VERSION) else {
            return .failure(.coreMismatch)
        }
        let hint: UInt32
        switch ext.lowercased().trimmingCharacters(in: CharacterSet(charactersIn: ".")) {
        case "hwp": hint = UInt32(HWP_CORE_HINT_HWP)
        case "hwpx": hint = UInt32(HWP_CORE_HINT_HWPX)
        default: hint = UInt32(HWP_CORE_HINT_AUTO)
        }
        var buffer = HwpCoreBuf(ptr: nil, len: 0)
        let status: Int32
        if data.isEmpty {
            var dummy: UInt8 = 0
            status = withUnsafePointer(to: &dummy) {
                hwp_core_parse($0, 0, hint, &buffer)
            }
        } else {
            status = data.withUnsafeBytes { raw in
                hwp_core_parse(
                    raw.bindMemory(to: UInt8.self).baseAddress,
                    data.count,
                    hint,
                    &buffer
                )
            }
        }
        guard status == 0 else {
            return .failure(.internalError("invalid FFI arguments"))
        }
        defer { hwp_core_buf_free(buffer) }
        guard let ptr = buffer.ptr else { return .failure(.malformedFrame) }
        let frame = Data(bytes: ptr, count: buffer.len)
        return decodeFrame(frame)
    }

    static func decodeFrame(
        _ frame: Data
    ) -> Result<(document: HWPDocument, warnings: [Warning]), Failure> {
        do {
            var reader = FrameReader(data: frame)
            guard try reader.take(4) == Data("HWPC".utf8) else {
                return .failure(.malformedFrame)
            }
            guard try reader.u32() == 1 else {
                return .failure(.malformedFrame)
            }
            let jsonLength = Int(try reader.u32())
            let json = try reader.take(jsonLength)
            let header = try JSONDecoder().decode(EnvelopeHeader.self, from: json)
            guard header.schema == 1 else {
                return .failure(.coreMismatch)
            }
            if !header.ok {
                let envelope = try JSONDecoder().decode(FailureEnvelope.self, from: json)
                return .failure(failure(for: envelope.fatal))
            }
            let envelope = try JSONDecoder().decode(SuccessEnvelope.self, from: json)
            let warnings = envelope.warnings
            let document = try makeDocument(envelope.document)
            try readBlobs(&reader, into: document)
            return .success((document, warnings))
        } catch let failure as Failure {
            return .failure(failure)
        } catch {
            return .failure(.malformedFrame)
        }
    }

    private static func failure(for fatal: FatalDTO) -> Failure {
        switch fatal.kind {
        case "NotHwp": return .notHWP
        case "Hwp3Unsupported": return .hwp3Unsupported
        case "HwpmlUnsupported": return .hwpmlUnsupported
        case "PasswordProtected": return .passwordProtected
        case "NoReadableSection": return .noReadableSection
        case "Internal": return .internalError(fatal.detail)
        default: return .internalError(fatal.kind)
        }
    }

    private static func makeDocument(_ dto: DocumentDTO) throws -> HWPDocument {
        let document = HWPDocument()
        for (language, faces) in dto.fontFaces {
            var named: [Int: String] = [:]
            var detailed: [Int: HWPFontFace] = [:]
            for face in faces {
                named[face.id] = face.name
                var value = HWPFontFace(name: face.name)
                value.type = face.type
                value.altName = face.altName
                value.altType = face.altType
                value.defaultName = face.defaultName
                value.embedded = face.embedded
                value.binaryRef = face.binaryRef
                detailed[face.id] = value
            }
            document.fonts[language] = named
            document.fontFaces[language] = detailed
        }
        for value in dto.charPrs { document.charPrs[value.id] = value }
        for value in dto.paraPrs { document.paraPrs[value.id] = value }
        for value in dto.tabPrs { document.tabPrs[value.id] = value }
        for value in dto.borderFills { document.borderFills[value.id] = value }
        for value in dto.numberings { document.numberings[value.id] = value }
        for value in dto.bullets { document.bullets[value.id] = value }
        for value in dto.styles {
            document.styles[value.id] = (value.paraPrID, value.charPrID)
        }
        document.sections = dto.sections
        document.memoShapes = dto.memoShapes
        document.wcharPositions = dto.wcharPositions
        return document
    }

    private static func readBlobs(_ reader: inout FrameReader, into document: HWPDocument) throws {
        let count = Int(try reader.u32())
        for _ in 0..<count {
            let idLength = Int(try reader.u32())
            let idData = try reader.take(idLength)
            guard let id = String(data: idData, encoding: .utf8) else {
                throw Failure.malformedFrame
            }
            let kind = try reader.u8()
            guard let lengthValue = Int(exactly: try reader.u64()), lengthValue >= 0 else {
                throw Failure.malformedFrame
            }
            let data = try reader.take(lengthValue)
            switch kind {
            case 0: document.binData[id] = data
            case 1: document.embeddedFonts.append(data)
            default: throw Failure.malformedFrame
            }
        }
        guard reader.isAtEnd else { throw Failure.malformedFrame }
    }
}

private struct EnvelopeHeader: Decodable {
    let schema: Int
    let ok: Bool
}

private struct FailureEnvelope: Decodable {
    let fatal: FatalDTO
}

private struct FatalDTO: Decodable {
    let kind: String
    let detail: String
}

private struct SuccessEnvelope: Decodable {
    let document: DocumentDTO
    let warnings: [HWPCore.Warning]
}

private struct StyleDTO: Decodable {
    let id: Int
    let paraPrID: Int
    let charPrID: Int
}

private struct FontFaceDTO: Decodable {
    let id: Int
    let name: String
    let type: Int
    let altName: String?
    let altType: Int
    let defaultName: String?
    let embedded: Bool
    let binaryRef: String?
}

private struct DocumentDTO: Decodable {
    let fontFaces: [String: [FontFaceDTO]]
    let charPrs: [HWPCharPr]
    let paraPrs: [HWPParaPr]
    let tabPrs: [HWPTabPr]
    let borderFills: [HWPBorderFill]
    let numberings: [HWPNumbering]
    let bullets: [HWPBullet]
    let styles: [StyleDTO]
    let sections: [HWPSection]
    let memoShapes: [HWPMemoShape]
    let wcharPositions: Bool
}

private struct FrameReader {
    let data: Data
    private(set) var offset = 0

    var isAtEnd: Bool { offset == data.count }

    mutating func take(_ count: Int) throws -> Data {
        guard count >= 0, offset >= 0, offset <= data.count, count <= data.count - offset else {
            throw HWPCore.Failure.malformedFrame
        }
        let start = data.startIndex + offset
        let end = start + count
        offset += count
        return data[start..<end]
    }

    mutating func u8() throws -> UInt8 {
        try take(1).first ?? 0
    }

    mutating func u32() throws -> UInt32 {
        let bytes = try take(4)
        var value: UInt32 = 0
        for (index, byte) in bytes.enumerated() {
            value |= UInt32(byte) << (8 * UInt32(index))
        }
        return value
    }

    mutating func u64() throws -> UInt64 {
        let bytes = try take(8)
        var value: UInt64 = 0
        for (index, byte) in bytes.enumerated() {
            value |= UInt64(byte) << (8 * UInt64(index))
        }
        return value
    }
}

extension HWPShape {
    private enum CodingKeys: String, CodingKey {
        case kind, width, height, pos, outMargins, lineColor, lineWidth, lineType
        case fillColor, fillAlpha, roundRatio, zOrder, paragraphs, textMargins, points
        case rotation, equation, children, caption, arrowStart, arrowEnd, gradient, imageFill
        case segmentTypes, arc
    }

    public init(from decoder: Decoder) throws {
        let values = try decoder.container(keyedBy: CodingKeys.self)
        kind = try values.decode(String.self, forKey: .kind)
        width = try values.decode(HWPUnit.self, forKey: .width)
        height = try values.decode(HWPUnit.self, forKey: .height)
        pos = try values.decode(HWPPos.self, forKey: .pos)
        outMargins = try values.decode(HWPMargins.self, forKey: .outMargins)
        lineColor = try values.decode(UInt32.self, forKey: .lineColor)
        lineWidth = try values.decode(CGFloat.self, forKey: .lineWidth)
        lineType = try values.decode(String.self, forKey: .lineType)
        fillColor = try values.decodeIfPresent(UInt32.self, forKey: .fillColor)
        fillAlpha = try values.decode(CGFloat.self, forKey: .fillAlpha)
        roundRatio = try values.decode(CGFloat.self, forKey: .roundRatio)
        zOrder = try values.decode(Int.self, forKey: .zOrder)
        paragraphs = try values.decode([HWPParagraph].self, forKey: .paragraphs)
        textMargins = try values.decode(HWPMargins.self, forKey: .textMargins)
        points = try values.decode([CGPoint].self, forKey: .points)
        rotation = try values.decode(CGFloat.self, forKey: .rotation)
        equation = try values.decodeIfPresent(HWPEquation.self, forKey: .equation)
        children = try values.decode([HWPShapeChild].self, forKey: .children)
        caption = try values.decodeIfPresent(HWPCaption.self, forKey: .caption)
        arrowStart = try values.decodeIfPresent(HWPArrow.self, forKey: .arrowStart)
        arrowEnd = try values.decodeIfPresent(HWPArrow.self, forKey: .arrowEnd)
        gradient = try values.decodeIfPresent(HWPGradient.self, forKey: .gradient)
        imageFill = try values.decodeIfPresent(HWPImageFill.self, forKey: .imageFill)
        segmentTypes = try values.decodeIfPresent([String].self, forKey: .segmentTypes) ?? []
        arc = try values.decodeIfPresent(HWPArc.self, forKey: .arc)
    }
}

extension HWPPos {
    private enum CodingKeys: String, CodingKey {
        case treatAsChar, vertRelTo, horzRelTo, widthRelTo, heightRelTo, vertAlign, horzAlign
        case vertOffset, horzOffset, flowWithText, allowOverlap, affectLSpacing, textWrap
    }

    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        treatAsChar = try container.decode(Bool.self, forKey: .treatAsChar)
        vertRelTo = try container.decode(String.self, forKey: .vertRelTo)
        horzRelTo = try container.decode(String.self, forKey: .horzRelTo)
        widthRelTo = try container.decodeIfPresent(String.self, forKey: .widthRelTo) ?? "ABSOLUTE"
        heightRelTo = try container.decodeIfPresent(String.self, forKey: .heightRelTo) ?? "ABSOLUTE"
        vertAlign = try container.decode(String.self, forKey: .vertAlign)
        horzAlign = try container.decode(String.self, forKey: .horzAlign)
        vertOffset = try container.decode(HWPUnit.self, forKey: .vertOffset)
        horzOffset = try container.decode(HWPUnit.self, forKey: .horzOffset)
        flowWithText = try container.decode(Bool.self, forKey: .flowWithText)
        allowOverlap = try container.decode(Bool.self, forKey: .allowOverlap)
        affectLSpacing = try container.decode(Bool.self, forKey: .affectLSpacing)
        textWrap = try container.decode(String.self, forKey: .textWrap)
    }
}

extension HWPBorderFill {
    private struct GradientStop: Decodable {
        let color: UInt32
        let position: CGFloat

        init(from decoder: Decoder) throws {
            var container = try decoder.unkeyedContainer()
            color = try container.decode(UInt32.self)
            position = try container.decode(CGFloat.self)
            guard container.isAtEnd else {
                throw DecodingError.dataCorrupted(
                    .init(codingPath: container.codingPath, debugDescription: "gradient stop")
                )
            }
        }
    }

    private enum CodingKeys: String, CodingKey {
        case id, left, right, top, bottom, diagonal, slash, backSlash, fillColor, gradient
    }

    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        id = try container.decode(Int.self, forKey: .id)
        left = try container.decode(HWPBorder.self, forKey: .left)
        right = try container.decode(HWPBorder.self, forKey: .right)
        top = try container.decode(HWPBorder.self, forKey: .top)
        bottom = try container.decode(HWPBorder.self, forKey: .bottom)
        diagonal = try container.decode(HWPBorder.self, forKey: .diagonal)
        slash = try container.decode(String.self, forKey: .slash)
        backSlash = try container.decode(String.self, forKey: .backSlash)
        fillColor = try container.decodeIfPresent(UInt32.self, forKey: .fillColor)
        let stops = try container.decodeIfPresent([GradientStop].self, forKey: .gradient)
        gradient = stops?.map { ($0.color, $0.position) }
    }
}

extension HWPSection {
    private enum CodingKeys: String, CodingKey {
        case page, paragraphs, header, footer, columns, columnGap, tabStop, footnoteShape
        case pageBorderFills
    }

    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        page = try container.decode(HWPPage.self, forKey: .page)
        paragraphs = try container.decode([HWPParagraph].self, forKey: .paragraphs)
        header = try container.decode([HWPParagraph].self, forKey: .header)
        footer = try container.decode([HWPParagraph].self, forKey: .footer)
        columns = try container.decode(Int.self, forKey: .columns)
        columnGap = try container.decode(HWPUnit.self, forKey: .columnGap)
        tabStop = try container.decode(HWPUnit.self, forKey: .tabStop)
        footnoteShape = try container.decode(HWPNoteShape.self, forKey: .footnoteShape)
        pageBorderFills = try container.decodeIfPresent([HWPPageBorderFill].self, forKey: .pageBorderFills) ?? []
        columnsDefined = false
        noteCount = 0
    }
}

extension HWPRunItem: Decodable {
    private enum CodingKeys: String, CodingKey {
        case t, v
    }

    private struct Flow: Decodable {
        let paragraphs: [HWPParagraph]
        let applies: String
    }

    private struct Hiding: Decodable {
        let header: Bool
        let footer: Bool
        let pageNum: Bool
    }

    private struct PageNumber: Decodable {
        let pos: String
        let format: String
        let side: String
    }

    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        let tag = try container.decode(String.self, forKey: .t)
        switch tag {
        case "text": self = .text(try container.decode(String.self, forKey: .v))
        case "table": self = .table(try container.decode(HWPTable.self, forKey: .v))
        case "picture": self = .picture(try container.decode(HWPPicture.self, forKey: .v))
        case "shape": self = .shape(try container.decode(HWPShape.self, forKey: .v))
        case "tab": self = .tab
        case "sizedTab": self = .sizedTab(try container.decode(HWPUnit.self, forKey: .v))
        case "formattedTab": self = .formattedTab(try container.decode(HWPTab.self, forKey: .v))
        case "lineBreak": self = .lineBreak
        case "autoNum": self = .autoNum(try container.decode(String.self, forKey: .v))
        case "control": self = .control(try container.decode(String.self, forKey: .v))
        case "note": self = .note(try container.decode(HWPNote.self, forKey: .v))
        // 글자 겹치기는 아직 앱 쪽 모양이 없어 조판 부호로 받는다.
        case "compose": self = .control("compose")
        case "skip": self = .skip(try container.decode(Int.self, forKey: .v))
        case "header":
            let flow = try container.decode(Flow.self, forKey: .v)
            self = .header(flow.paragraphs, flow.applies)
        case "footer":
            let flow = try container.decode(Flow.self, forKey: .v)
            self = .footer(flow.paragraphs, flow.applies)
        case "pageHiding":
            let hiding = try container.decode(Hiding.self, forKey: .v)
            self = .pageHiding(header: hiding.header, footer: hiding.footer, pageNum: hiding.pageNum)
        case "newNum": self = .newNum(try container.decode(Int.self, forKey: .v))
        case "pageNum":
            let pageNumber = try container.decode(PageNumber.self, forKey: .v)
            self = .pageNum(pos: pageNumber.pos, format: pageNumber.format, side: pageNumber.side)
        default:
            throw DecodingError.dataCorruptedError(
                forKey: .t,
                in: container,
                debugDescription: "unknown run item \(tag)"
            )
        }
    }
}
