mod columns;
pub mod container;
pub mod control;
mod distribute;
pub mod docinfo;
pub mod numbering;
mod page_border_fill;
pub mod para;
mod password;
pub mod reader;
pub mod record;
pub mod shape;
mod summary;
pub mod table;
pub mod tags;

use std::collections::BTreeMap;
use std::io::Read;
use std::sync::Arc;

use cfb::CompoundFile;

use crate::info::DocumentInfo;
use crate::model::{Blob, BlobKind, Document, Section};
use crate::report::{Fatal, Warning, WarningKind};
use crate::{ParseOptions, Parsed};

use reader::BinReader;
use record::Record;
use tags::{T_DOCUMENT_PROPERTIES, T_ID_MAPPINGS, T_PARA_HEADER};

const FILE_HEADER: &str = "FileHeader";
const DOC_INFO: &str = "DocInfo";
const BODY_TEXT_PREFIX: &str = "BodyText/Section";
const VIEW_TEXT_PREFIX: &str = "ViewText/Section";

struct StreamSet {
    lengths: BTreeMap<String, u64>,
    cipher: Option<password::PasswordCipher>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StreamIntegrity {
    Ok,
    ChecksumMismatch,
    Truncated { len: usize, block: usize },
}

struct ReadStream {
    data: Vec<u8>,
    size_mismatch: bool,
    integrity: StreamIntegrity,
}

impl StreamSet {
    fn read<R: Read + std::io::Seek>(
        &self,
        file: &mut CompoundFile<R>,
        name: &str,
    ) -> Option<ReadStream> {
        let expected = *self.lengths.get(name)?;
        let mut stream = file.open_stream(format!("/{name}")).ok()?;
        let mut data = Vec::new();
        let read_failed = match stream.read_to_end(&mut data) {
            Ok(_) => false,
            Err(_) => data.is_empty(),
        };
        if read_failed {
            return None;
        }
        let stored_len = data.len() as u64;
        let mut integrity = StreamIntegrity::Ok;
        if let Some(cipher) = &self.cipher {
            if password::protected_stream(name) {
                match cipher.decrypt(&mut data) {
                    Ok(password::FooterIntegrity::Verified | password::FooterIntegrity::Absent) => {
                    }
                    Ok(password::FooterIntegrity::Mismatch) => {
                        integrity = StreamIntegrity::ChecksumMismatch;
                    }
                    Err(password::StreamError::Truncated { len, block }) => {
                        integrity = StreamIntegrity::Truncated { len, block };
                        data.clear();
                    }
                }
            }
        }
        // The directory length covers the ciphertext (including the password footer), not the
        // decrypted payload.
        Some(ReadStream {
            data,
            size_mismatch: stored_len != expected,
            integrity,
        })
    }
}

fn warn_stream_integrity(
    name: &str,
    section: Option<u32>,
    integrity: StreamIntegrity,
    warnings: &mut Vec<Warning>,
) {
    let detail = match integrity {
        StreamIntegrity::Ok => return,
        StreamIntegrity::ChecksumMismatch => {
            format!("{name} footer CRC32 does not match the decrypted payload")
        }
        StreamIntegrity::Truncated { len, block } => {
            format!("{name} is {len} bytes, not a whole {block}-byte cipher block (truncated)")
        }
    };
    warnings.push(Warning::new(
        WarningKind::StreamChecksumMismatch,
        section,
        detail,
    ));
}

pub fn parse(bytes: &[u8], options: &ParseOptions) -> Result<Parsed, Fatal> {
    let mut file = container::open(bytes)?;
    let mut streams = collect_streams(&file);
    let file_header = streams
        .read(&mut file, FILE_HEADER)
        .filter(|stream| stream.data.len() >= 40)
        .ok_or(Fatal::NotHwp)?;
    let header = file_header.data;
    let mut header_reader = BinReader::at(&header, 36);
    let flags = header_reader.u32();
    if flags & 0x2 != 0 {
        // FileHeader is plaintext, so a known version can be checked before a password is
        // requested. A header too short to carry the version keeps the old password-first error.
        let known_version = (header.len() >= 48).then(|| BinReader::at(&header, 44).u32());
        if let Some(version) = known_version {
            if !password::supported_version(version) {
                return Err(Fatal::UnsupportedEncryptionDetail(format!(
                    "HWP5 EncryptVersion {version} is not supported (supported: 0 DES, 4 AES)"
                )));
            }
        }
        let supplied = options
            .password
            .as_deref()
            .ok_or(Fatal::PasswordProtected)?;
        let version = known_version.ok_or_else(|| {
            Fatal::UnsupportedEncryptionDetail(
                "HWP5 FileHeader is shorter than 48 bytes; EncryptVersion is missing".to_string(),
            )
        })?;
        streams.cipher = Some(
            password::PasswordCipher::new(supplied, version)
                .expect("supported version constructs a cipher"),
        );
    }
    let compressed = flags & 0x1 != 0;
    let mut warnings = Vec::new();
    // 배포용 표시만 있고 ViewText 없이 BodyText에 평문 본문을 둔 파일이 있다.
    let mut distributed = flags & 0x4 != 0;
    if distributed && collect_section_streams(&streams, VIEW_TEXT_PREFIX).is_empty() {
        distributed = false;
        warnings.push(Warning::new(
            WarningKind::SectionMissing,
            None,
            "distribution document has no ViewText; reading BodyText",
        ));
    }
    let section_prefix = if distributed {
        VIEW_TEXT_PREFIX
    } else {
        BODY_TEXT_PREFIX
    };
    let section_streams = collect_section_streams(&streams, section_prefix);
    if section_streams.is_empty() {
        return Err(Fatal::NoReadableSection);
    }

    warn_section_gaps(&section_streams, section_prefix, &mut warnings);
    let (doc_info, doc_info_truncated) =
        read_doc_info(&mut file, &streams, compressed, &mut warnings);
    if streams.cipher.is_some()
        && (doc_info.first().map(|record| record.tag) != Some(T_DOCUMENT_PROPERTIES)
            || !doc_info.iter().any(|record| record.tag == T_ID_MAPPINGS))
    {
        if doc_info_truncated {
            return Err(Fatal::CorruptDocument(
                "DocInfo is not a whole DES block; the document is truncated".to_string(),
            ));
        }
        return Err(Fatal::WrongPassword);
    }
    check_document_properties(&doc_info, section_streams.len(), &mut warnings);

    let mut document = Document {
        wchar_positions: true,
        ..Document::default()
    };
    let version = BinReader::at(&header, 32).u32();
    let bin_data = docinfo::parse_doc_info(&doc_info, version, &mut document, &mut warnings);
    load_bin_data(
        &mut file,
        &streams,
        compressed,
        &bin_data,
        &mut document,
        &mut warnings,
    );
    let max_bin = bin_data.names.keys().next_back().copied().unwrap_or(0) as usize;
    let mut sections = Vec::new();
    // A damaged protected stream is not a password error. Keep reading what is intact and only
    // report damage if no section survives.
    let mut damaged = false;
    for (index, name) in &section_streams {
        let section = Some(*index);
        let Some(stream) = streams.read(&mut file, name) else {
            warnings.push(Warning::new(
                WarningKind::SectionUnreadable,
                section,
                format!("{name} is unreadable"),
            ));
            continue;
        };
        let mut raw = stream.data;
        if stream.size_mismatch {
            warnings.push(Warning::new(
                WarningKind::StreamSizeMismatch,
                section,
                format!("{name} size does not match the directory"),
            ));
        }
        warn_stream_integrity(name, section, stream.integrity, &mut warnings);
        if matches!(stream.integrity, StreamIntegrity::Truncated { .. }) {
            damaged = true;
            continue;
        }
        if matches!(stream.integrity, StreamIntegrity::ChecksumMismatch) {
            damaged = true;
        }
        if distributed {
            match distribute::decrypt(&raw, section, &mut warnings) {
                Ok(data) => raw = data,
                Err(warning) => {
                    warnings.push(warning);
                    continue;
                }
            }
        }
        if compressed {
            match inflate_raw(&raw) {
                Some(inflated) => {
                    if inflated.truncated {
                        warnings.push(Warning::new(
                            WarningKind::StreamSizeMismatch,
                            section,
                            format!(
                                "{name} deflate output exceeds {} MiB; truncated",
                                MAX_INFLATE / (1024 * 1024)
                            ),
                        ));
                    }
                    if streams
                        .cipher
                        .as_ref()
                        .is_some_and(|cipher| cipher.uses_des())
                    {
                        if let password::CompactTrailer::Mismatch(reason) =
                            password::validate_compact_trailer(
                                &raw,
                                inflated.consumed,
                                &inflated.data,
                            )
                        {
                            damaged = true;
                            warnings.push(Warning::new(
                                WarningKind::StreamChecksumMismatch,
                                section,
                                format!("{name} legacy compact trailer {reason} does not match"),
                            ));
                        }
                    }
                    raw = inflated.data;
                }
                None => {
                    warnings.push(Warning::new(
                        WarningKind::SectionUnreadable,
                        section,
                        format!("{name} deflate stream is unreadable"),
                    ));
                    continue;
                }
            }
        }
        let data = raw;
        let (records, record_warnings) = record::parse_checked(&data, section);
        warnings.extend(record_warnings);
        let mut parsed = Section::default();
        let mut state = control::ParseState::default()
            .with_max_bin(max_bin)
            .with_max_depth(options.max_depth);
        for paragraph in records.iter().filter(|record| record.tag == T_PARA_HEADER) {
            let mut context = control::ParseContext::new(
                &document,
                &mut parsed,
                &mut state,
                section,
                &mut warnings,
            );
            let value = para::parse_paragraph(paragraph, &mut context);
            context.section.paragraphs.push(value);
        }
        sections.push(parsed);
    }
    if sections.is_empty() {
        if damaged {
            return Err(Fatal::CorruptDocument(
                "no section could be read and a protected stream failed its integrity check"
                    .to_string(),
            ));
        }
        return Err(Fatal::NoReadableSection);
    }

    document.sections = sections;
    let metadata = streams
        .read(&mut file, summary::SUMMARY_STREAM)
        .map(|stream| summary::parse(&stream.data))
        .unwrap_or_default();
    let info = DocumentInfo {
        version: Some(format!(
            "{}.{}.{}.{}",
            version >> 24,
            (version >> 16) & 0xFF,
            (version >> 8) & 0xFF,
            version & 0xFF
        )),
        encrypted: streams.cipher.is_some(),
        distribution: flags & 0x4 != 0,
        metadata,
    };
    Ok(Parsed {
        document,
        warnings,
        info,
    })
}

fn collect_streams<R>(file: &CompoundFile<R>) -> StreamSet {
    let mut lengths = BTreeMap::new();
    for entry in file.walk() {
        if !entry.is_stream() {
            continue;
        }
        let name = entry
            .path()
            .components()
            .filter_map(|component| match component {
                std::path::Component::Normal(name) => Some(name.to_string_lossy()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("/");
        lengths.insert(name, entry.len());
    }
    StreamSet {
        lengths,
        cipher: None,
    }
}

fn section_index(name: &str, prefix: &str) -> Option<u32> {
    let suffix = name.strip_prefix(prefix)?;
    if suffix.is_empty() || !suffix.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let index = suffix.parse::<u32>().ok()?;
    if format!("{prefix}{index}") == name {
        Some(index)
    } else {
        None
    }
}

fn collect_section_streams(streams: &StreamSet, prefix: &str) -> Vec<(u32, String)> {
    let mut sections: Vec<(u32, String)> = streams
        .lengths
        .keys()
        .filter_map(|name| section_index(name, prefix).map(|index| (index, name.clone())))
        .collect();
    sections.sort_by_key(|(index, _)| *index);
    sections
}

fn warn_section_gaps(sections: &[(u32, String)], prefix: &str, warnings: &mut Vec<Warning>) {
    let mut next = 0u32;
    for (index, _) in sections {
        if *index > next {
            let last = index.saturating_sub(1);
            let missing = u64::from(*index) - u64::from(next);
            warnings.push(Warning::new(
                WarningKind::SectionMissing,
                Some(next),
                format!("{prefix}{next}..{last} missing ({missing})"),
            ));
        }
        next = index.saturating_add(1);
    }
}

fn read_doc_info<R: Read + std::io::Seek>(
    file: &mut CompoundFile<R>,
    streams: &StreamSet,
    compressed: bool,
    warnings: &mut Vec<Warning>,
) -> (Vec<Record>, bool) {
    let Some(stream) = streams.read(file, DOC_INFO) else {
        warnings.push(Warning::new(
            WarningKind::DocInfoUnreadable,
            None,
            "DocInfo is unreadable",
        ));
        return (Vec::new(), false);
    };
    if stream.size_mismatch {
        warnings.push(Warning::new(
            WarningKind::StreamSizeMismatch,
            None,
            "DocInfo size does not match the directory",
        ));
    }
    warn_stream_integrity(DOC_INFO, None, stream.integrity, warnings);
    if matches!(stream.integrity, StreamIntegrity::Truncated { .. }) {
        return (Vec::new(), true);
    }
    let raw = stream.data;
    let mut data = if compressed {
        match inflate_raw(&raw) {
            Some(inflated) => {
                if inflated.truncated {
                    warnings.push(Warning::new(
                        WarningKind::StreamSizeMismatch,
                        None,
                        format!(
                            "DocInfo deflate output exceeds {} MiB; truncated",
                            MAX_INFLATE / (1024 * 1024)
                        ),
                    ));
                }
                if streams
                    .cipher
                    .as_ref()
                    .is_some_and(|cipher| cipher.uses_des())
                {
                    if let password::CompactTrailer::Mismatch(reason) =
                        password::validate_compact_trailer(&raw, inflated.consumed, &inflated.data)
                    {
                        warnings.push(Warning::new(
                            WarningKind::StreamChecksumMismatch,
                            None,
                            format!("DocInfo legacy compact trailer {reason} does not match"),
                        ));
                    }
                }
                inflated.data
            }
            None => {
                warnings.push(Warning::new(
                    WarningKind::DocInfoUnreadable,
                    None,
                    "DocInfo deflate stream is unreadable",
                ));
                return (Vec::new(), false);
            }
        }
    } else {
        raw
    };
    if streams.cipher.is_some() {
        data = normalize_protected_doc_info(data);
    }
    let (records, record_warnings) = record::parse_checked(&data, None);
    warnings.extend(record_warnings);
    (records, false)
}

/// Protected DocInfo streams prepend 256 bytes to the first DocumentProperties payload.
/// Remove that prefix so the declared section count is read from the real payload.
fn normalize_protected_doc_info(data: Vec<u8>) -> Vec<u8> {
    const PREFIX: usize = 256;
    const PROTECTED_PROPERTIES_SIZE: usize = 282;
    if data.len() < 4 + PROTECTED_PROPERTIES_SIZE {
        return data;
    }
    let header = u32::from_le_bytes(data[..4].try_into().expect("four-byte record header"));
    let tag = header & 0x3ff;
    let level = (header >> 10) & 0x3ff;
    let size = header >> 20;
    if tag != u32::from(T_DOCUMENT_PROPERTIES)
        || level != 0
        || size != PROTECTED_PROPERTIES_SIZE as u32
    {
        return data;
    }
    let mut normalized = Vec::with_capacity(data.len() - PREFIX);
    normalized.extend_from_slice(&(u32::from(T_DOCUMENT_PROPERTIES) | (26 << 20)).to_le_bytes());
    normalized.extend_from_slice(&data[4 + PREFIX..]);
    normalized
}

fn check_document_properties(
    records: &[Record],
    section_count: usize,
    warnings: &mut Vec<Warning>,
) {
    let properties: Vec<&Record> = records
        .iter()
        .filter(|record| record.tag == T_DOCUMENT_PROPERTIES)
        .collect();
    if properties.len() > 1 {
        warnings.push(Warning::new(
            WarningKind::CountMismatch,
            None,
            format!("{} document property records", properties.len()),
        ));
    }
    let Some(properties) = properties.first() else {
        return;
    };
    if properties.data.len() < 2 {
        return;
    }
    let declared = usize::from(BinReader::new(&properties.data).u16());
    if declared != section_count {
        warnings.push(Warning::new(
            WarningKind::CountMismatch,
            None,
            format!("document properties declare {declared} section(s), found {section_count}"),
        ));
    }
}

fn load_bin_data<R: Read + std::io::Seek>(
    file: &mut CompoundFile<R>,
    streams: &StreamSet,
    compressed: bool,
    bin_data: &docinfo::BinDataStreams,
    document: &mut Document,
    warnings: &mut Vec<Warning>,
) {
    for (index, name) in &bin_data.names {
        let path = format!("BinData/{name}");
        let Some(stream) = streams.read(file, &path) else {
            warnings.push(Warning::new(
                WarningKind::MissingBinData,
                None,
                format!("{path} is missing"),
            ));
            continue;
        };
        if stream.size_mismatch {
            warnings.push(Warning::new(
                WarningKind::StreamSizeMismatch,
                None,
                format!("{path} size does not match the directory"),
            ));
        }
        warn_stream_integrity(&path, None, stream.integrity, warnings);
        if matches!(stream.integrity, StreamIntegrity::Truncated { .. }) {
            continue;
        }
        let raw = stream.data;
        let mode = bin_data.compression.get(index).copied().unwrap_or(0);
        let compressed_stream = mode == 1 || (mode == 0 && compressed);
        let data = if compressed_stream {
            match inflate_raw(&raw) {
                Some(inflated) => {
                    if inflated.truncated {
                        warnings.push(Warning::new(
                            WarningKind::StreamSizeMismatch,
                            None,
                            format!(
                                "{path} deflate output exceeds {} MiB; truncated",
                                MAX_INFLATE / (1024 * 1024)
                            ),
                        ));
                    }
                    if inflated.data.is_empty() {
                        raw
                    } else {
                        inflated.data
                    }
                }
                None => raw,
            }
        } else {
            raw
        };
        let id = format!("bin{index}");
        let (kind, data) = if crate::model::is_font_file(name) {
            (BlobKind::Font, data)
        } else {
            (
                BlobKind::Picture,
                crate::wmf::picture_data(data, name, &id, warnings),
            )
        };
        document.bins.push(Blob {
            id,
            kind,
            data: Arc::new(data),
        });
    }
}

const MAX_INFLATE: u64 = 512 * 1024 * 1024;

struct Inflated {
    data: Vec<u8>,
    consumed: usize,
    truncated: bool,
}

fn inflate_raw(raw: &[u8]) -> Option<Inflated> {
    inflate_raw_limit(raw, MAX_INFLATE)
}

fn inflate_raw_limit(raw: &[u8], limit: u64) -> Option<Inflated> {
    if raw.is_empty() {
        return None;
    }
    let mut decoder = flate2::read::DeflateDecoder::new(raw).take(limit.saturating_add(1));
    let mut data = Vec::new();
    match decoder.read_to_end(&mut data) {
        Ok(_) => {}
        Err(_) if !data.is_empty() => {}
        Err(_) => return None,
    }
    let consumed = decoder.get_ref().total_in() as usize;
    let truncated = data.len() as u64 > limit;
    if truncated {
        data.truncate(limit as usize);
    }
    Some(Inflated {
        data,
        consumed,
        truncated,
    })
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::inflate_raw_limit;

    #[test]
    fn inflate_limit_keeps_prefix() {
        let mut encoder =
            flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(&[7u8; 4096]).expect("deflate input");
        let compressed = encoder.finish().expect("finish deflate");
        let inflated = inflate_raw_limit(&compressed, 100).expect("inflate");
        assert!(inflated.truncated);
        assert!(inflated.consumed > 0 && inflated.consumed <= compressed.len());
        assert_eq!(inflated.data.len(), 100);
        assert!(inflated.data.iter().all(|byte| *byte == 7));
        let complete = inflate_raw_limit(&compressed, 10_000).expect("inflate");
        assert!(!complete.truncated);
        assert_eq!(complete.consumed, compressed.len());
        assert_eq!(complete.data.len(), 4096);
    }
}
