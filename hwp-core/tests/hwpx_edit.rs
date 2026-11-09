#[path = "support/mod.rs"]
mod support;

use hwp_core::hwpx::edit::{fill_empty_paragraph, find_text, replace_text, EditError};
use hwp_core::hwpx::zip::MiniZip;
use hwp_core::model::RunItem;
use hwp_core::{parse, ParagraphOrigin, ParseOptions};

use support::odfcrypt;
use support::zipw::{self, Entry};

const HEADER: &str = r#"<?xml version="1.0" encoding="UTF-8"?><hh:head xmlns:hh="urn:hh" secCnt="1"><hh:charProperties><hh:charPr id="12" height="1000"/><hh:charPr id="13" height="1400"/></hh:charProperties></hh:head>"#;
const NAMESPACES: &str = r#"xmlns:hs="urn:hs" xmlns:hp="urn:hp""#;

const LINE: &str = r#"<hp:linesegarray><hp:lineseg textpos="0" vertpos="0" vertsize="1000" textheight="1000" baseline="850" spacing="600" horzpos="0" horzsize="42520" flags="393216"/></hp:linesegarray>"#;

fn section(body: &str) -> String {
    format!(r#"<?xml version="1.0" encoding="UTF-8"?><hs:sec {NAMESPACES}>{body}</hs:sec>"#)
}

fn package(section_xml: &str) -> Vec<u8> {
    zipw::build(&[
        Entry {
            name: "mimetype",
            data: b"application/hwp+zip",
            deflate: false,
        },
        Entry {
            name: "Contents/header.xml",
            data: HEADER.as_bytes(),
            deflate: true,
        },
        Entry {
            name: "Contents/section0.xml",
            data: section_xml.as_bytes(),
            deflate: true,
        },
        Entry {
            name: "Contents/content.hpf",
            data: b"<opf:package xmlns:opf=\"urn:opf\"/>",
            deflate: true,
        },
    ])
}

fn read(bytes: &[u8], name: &str) -> String {
    let zip = MiniZip::new(bytes).expect("zip");
    String::from_utf8(zip.read(name).expect("entry")).expect("utf8")
}

fn names(bytes: &[u8]) -> Vec<String> {
    MiniZip::new(bytes)
        .expect("zip")
        .entries()
        .iter()
        .map(|entry| entry.name.clone())
        .collect()
}

fn texts(bytes: &[u8]) -> Vec<String> {
    let parsed = parse(bytes, &ParseOptions::default()).expect("parse");
    let mut texts = Vec::new();
    for section in &parsed.document.sections {
        for paragraph in &section.paragraphs {
            for run in &paragraph.runs {
                for item in &run.items {
                    if let RunItem::Text(text) = item {
                        texts.push(text.clone());
                    }
                }
            }
        }
    }
    texts
}

fn le_u16(data: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([data[at], data[at + 1]])
}

fn le_u32(data: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([data[at], data[at + 1], data[at + 2], data[at + 3]])
}

/// 압축 꾸러미의 중앙 목록을 따라가며 각 항목의 검사합·크기·머리 위치가 실제 내용과 맞는지 본다.
fn assert_container_is_consistent(bytes: &[u8]) {
    let eocd = (0..=bytes.len() - 22)
        .rev()
        .find(|at| le_u32(bytes, *at) == 0x0605_4b50)
        .expect("eocd");
    let count = usize::from(le_u16(bytes, eocd + 10));
    let directory_size = le_u32(bytes, eocd + 12) as usize;
    let mut position = le_u32(bytes, eocd + 16) as usize;
    assert_eq!(position + directory_size, eocd);
    let zip = MiniZip::new(bytes).expect("zip");
    assert_eq!(zip.entries().len(), count);
    for entry in zip.entries() {
        assert_eq!(le_u32(bytes, position), 0x0201_4b50);
        let crc = le_u32(bytes, position + 16);
        let uncompressed = le_u32(bytes, position + 24) as usize;
        let name_length = usize::from(le_u16(bytes, position + 28));
        let extra_length = usize::from(le_u16(bytes, position + 30));
        let comment_length = usize::from(le_u16(bytes, position + 32));
        let local = le_u32(bytes, position + 42) as usize;
        assert_eq!(le_u32(bytes, local), 0x0403_4b50);
        assert_eq!(le_u32(bytes, local + 14), crc);
        assert_eq!(le_u32(bytes, local + 22) as usize, uncompressed);
        let content = zip.read(&entry.name).expect("entry reads");
        assert_eq!(content.len(), uncompressed);
        assert_eq!(crc32fast::hash(&content), crc, "crc of {}", entry.name);
        position += 46 + name_length + extra_length + comment_length;
    }
}

fn name_paragraph() -> String {
    format!(
        r#"<hp:p id="1" paraPrIDRef="7" styleIDRef="3" pageBreak="0"><hp:run charPrIDRef="12"><hp:t>담당 김햔지 선생님</hp:t></hp:run>{LINE}</hp:p>"#
    )
}

fn score_paragraph() -> String {
    format!(
        r#"<hp:p id="2" paraPrIDRef="8" styleIDRef="0" pageBreak="0"><hp:run charPrIDRef="13"><hp:t>점수는 67점</hp:t></hp:run>{LINE}</hp:p>"#
    )
}

#[test]
fn fixes_a_misspelled_name_and_changes_nothing_else() {
    let before = section(&format!("{}{}", name_paragraph(), score_paragraph()));
    let bytes = package(&before);

    let edited = replace_text(&bytes, "김햔지", "김현지", None).expect("replace");

    assert_eq!(
        read(&edited, "Contents/section0.xml"),
        before.replace("김햔지", "김현지")
    );
    assert_eq!(names(&edited), names(&bytes));
    for name in ["mimetype", "Contents/header.xml", "Contents/content.hpf"] {
        assert_eq!(read(&edited, name), read(&bytes, name), "{name}");
    }
    assert_container_is_consistent(&edited);
    assert!(texts(&edited).contains(&"담당 김현지 선생님".to_string()));
}

#[test]
fn changes_a_number_to_a_longer_string_without_touching_styles() {
    let before = section(&format!("{}{}", name_paragraph(), score_paragraph()));
    let bytes = package(&before);

    let edited = replace_text(&bytes, "67", "6.7", None).expect("replace");

    let after = read(&edited, "Contents/section0.xml");
    // 글자 수가 바뀐 점수 문단만 줄 배치를 지운다. 이름 문단의 줄 배치는 그대로다.
    let (head, tail) = before.rsplit_once(LINE).expect("score line layout");
    assert_eq!(after, format!("{head}{tail}").replace("67", "6.7"));
    for attribute in [
        r#"paraPrIDRef="8""#,
        r#"styleIDRef="0""#,
        r#"charPrIDRef="13""#,
    ] {
        assert_eq!(
            after.matches(attribute).count(),
            before.matches(attribute).count()
        );
    }
    assert!(texts(&edited).contains(&"점수는 6.7점".to_string()));
    assert_container_is_consistent(&edited);
}

#[test]
fn round_trips_the_existing_fixture() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/synthetic.hwpx");
    let bytes = std::fs::read(path).expect("fixture");

    let edited = replace_text(&bytes, "ALPHA", "ALPHA-2", None).expect("replace");

    assert_eq!(names(&edited), names(&bytes));
    for name in names(&bytes) {
        let (old, new) = (read(&bytes, &name), read(&edited, &name));
        if name == "Contents/section0.xml" {
            assert_eq!(new, old.replace("ALPHA", "ALPHA-2"));
        } else {
            assert_eq!(new, old, "{name}");
        }
    }
    assert_container_is_consistent(&edited);
    let model = format!(
        "{:?}",
        parse(&edited, &ParseOptions::default())
            .expect("parse")
            .document
    );
    assert!(model.contains("ALPHA-2") && model.contains("BETA"));
    let before = parse(&bytes, &ParseOptions::default()).expect("parse");
    let after = parse(&edited, &ParseOptions::default()).expect("parse");
    assert_eq!(
        before.document.sections.len(),
        after.document.sections.len()
    );
}

#[test]
fn leaves_untouched_entries_byte_for_byte() {
    let bytes = package(&section(&name_paragraph()));
    let edited = replace_text(&bytes, "김햔지", "김현지", None).expect("replace");
    let (old, new) = (
        MiniZip::new(&bytes).unwrap(),
        MiniZip::new(&edited).unwrap(),
    );
    let raw = |data: &[u8], zip: &MiniZip<'_>, name: &str| -> Vec<u8> {
        let entry = zip
            .entries()
            .iter()
            .find(|entry| entry.name == name)
            .unwrap();
        let local = entry_local_range(data, entry.name.as_str());
        data[local.0..local.1].to_vec()
    };
    for name in ["mimetype", "Contents/header.xml", "Contents/content.hpf"] {
        assert_eq!(raw(&bytes, &old, name), raw(&edited, &new, name), "{name}");
    }
}

fn entry_local_range(data: &[u8], name: &str) -> (usize, usize) {
    let mut at = 0;
    while le_u32(data, at) == 0x0403_4b50 {
        let compressed = le_u32(data, at + 18) as usize;
        let name_length = usize::from(le_u16(data, at + 26));
        let extra_length = usize::from(le_u16(data, at + 28));
        let end = at + 30 + name_length + extra_length + compressed;
        if &data[at + 30..at + 30 + name_length] == name.as_bytes() {
            return (at, end);
        }
        at = end;
    }
    panic!("entry {name} not found");
}

#[test]
fn refuses_to_replace_every_match_when_the_text_is_not_unique() {
    let body = format!(
        r#"<hp:p paraPrIDRef="0"><hp:run charPrIDRef="12"><hp:t>가나다 가나라</hp:t></hp:run>{LINE}</hp:p>"#
    );
    let bytes = package(&section(&body));

    assert_eq!(
        replace_text(&bytes, "가나", "하나", None),
        Err(EditError::Ambiguous { count: 2 })
    );
    assert_eq!(
        replace_text(&bytes, "가나", "하나", Some(3)),
        Err(EditError::OccurrenceOutOfRange {
            occurrence: 3,
            count: 2
        })
    );
    let edited = replace_text(&bytes, "가나", "하나", Some(2)).expect("replace");
    assert_eq!(
        read(&edited, "Contents/section0.xml"),
        section(&body.replace("가나라", "하나라"))
    );

    let found = find_text(&bytes, "가나").expect("find");
    assert_eq!(found.len(), 2);
    assert_eq!((found[0].occurrence, found[1].occurrence), (1, 2));
    assert_eq!(found[0].before, "");
    assert_eq!(found[0].after, "다 가나라");
    assert_eq!(found[1].before, "가나다 ");
    assert_eq!(found[1].paragraph, "가나다 가나라");
}

#[test]
fn counts_matches_across_paragraphs_in_document_order() {
    let body = format!("{}{}", name_paragraph(), name_paragraph());
    let bytes = package(&section(&body));
    let found = find_text(&bytes, "김햔지").expect("find");
    assert_eq!(found.len(), 2);
    let first = replace_text(&bytes, "김햔지", "김현지", Some(1)).expect("replace");
    assert_eq!(
        texts(&first),
        vec!["담당 김현지 선생님", "담당 김햔지 선생님"]
    );
}

#[test]
fn refuses_a_lone_match_while_the_same_text_also_spans_runs() {
    let body = format!(
        r#"<hp:p paraPrIDRef="0"><hp:run charPrIDRef="12"><hp:t>AB</hp:t></hp:run>{LINE}</hp:p><hp:p paraPrIDRef="0"><hp:run charPrIDRef="12"><hp:t>A</hp:t></hp:run><hp:run charPrIDRef="13"><hp:t>B</hp:t></hp:run>{LINE}</hp:p>"#
    );
    let before = section(&body);
    let bytes = package(&before);
    assert_eq!(
        replace_text(&bytes, "AB", "CD", None),
        Err(EditError::AlsoSpansRuns)
    );
    let edited = replace_text(&bytes, "AB", "CD", Some(1)).expect("explicit occurrence");
    assert_eq!(texts(&edited), vec!["CD", "A", "B"]);
}

#[test]
fn rejects_characters_xml_cannot_hold() {
    let body = format!(
        r#"<hp:p paraPrIDRef="0"><hp:run charPrIDRef="12"><hp:t>AB</hp:t></hp:run>{LINE}</hp:p>"#
    );
    let bytes = package(&section(&body));
    for bad in ["x\u{FFFE}", "\u{FFFF}"] {
        assert_eq!(
            replace_text(&bytes, "AB", bad, None),
            Err(EditError::InvalidReplacement)
        );
    }
}

#[test]
fn reports_missing_and_run_spanning_text() {
    let body = format!(
        r#"<hp:p paraPrIDRef="0"><hp:run charPrIDRef="12"><hp:t>A</hp:t></hp:run><hp:run charPrIDRef="13"><hp:t>B</hp:t></hp:run>{LINE}</hp:p>"#
    );
    let bytes = package(&section(&body));
    assert_eq!(
        replace_text(&bytes, "AB", "CD", None),
        Err(EditError::SpansRuns)
    );
    assert_eq!(
        replace_text(&bytes, "Z", "CD", None),
        Err(EditError::NotFound)
    );
    assert_eq!(find_text(&bytes, "AB"), Err(EditError::SpansRuns));
    assert_eq!(
        replace_text(&bytes, "", "CD", None),
        Err(EditError::EmptySearch)
    );
    assert_eq!(
        replace_text(&bytes, "A", "C\tD", None),
        Err(EditError::InvalidReplacement)
    );
    assert_eq!(
        replace_text(&bytes, "A", "C\u{7}D", None),
        Err(EditError::InvalidReplacement)
    );
}

#[test]
fn keeps_entities_and_cdata_valid() {
    let body = format!(
        r#"<hp:p paraPrIDRef="0"><hp:run charPrIDRef="12"><hp:t>A&amp;B &lt;C&gt;</hp:t></hp:run>{LINE}</hp:p><hp:p paraPrIDRef="0"><hp:run charPrIDRef="12"><hp:t><![CDATA[x < y]]></hp:t></hp:run>{LINE}</hp:p>"#
    );
    let bytes = package(&section(&body));

    let edited = replace_text(&bytes, "A&B", "A&C<D", None).expect("replace");
    assert_eq!(
        read(&edited, "Contents/section0.xml"),
        section(
            &body
                .replace("A&amp;B", "A&amp;C&lt;D")
                .replacen(LINE, "", 1)
        )
    );
    assert!(texts(&edited).contains(&"A&C<D <C>".to_string()));

    let edited = replace_text(&bytes, "x < y", "x > y", None).expect("replace");
    let after = read(&edited, "Contents/section0.xml");
    assert!(after.contains("x &gt; y"));
    assert!(texts(&edited).contains(&"x > y".to_string()));

    let edited = replace_text(&bytes, "< y", "&& y", None).expect("replace");
    assert!(texts(&edited).contains(&"x && y".to_string()));
}

fn two_line_paragraph(runs: &str) -> String {
    format!(
        r#"<hp:p paraPrIDRef="0" styleIDRef="0">{runs}<hp:linesegarray><hp:lineseg textpos="0" vertpos="0" spacing="600"/><hp:lineseg textpos="10" vertpos="1600" spacing="600"/><hp:lineseg textpos="20" vertpos="3200" spacing="600"/></hp:linesegarray></hp:p>"#
    )
}

const THREE_LINES: &str = r#"<hp:linesegarray><hp:lineseg textpos="0" vertpos="0" spacing="600"/><hp:lineseg textpos="10" vertpos="1600" spacing="600"/><hp:lineseg textpos="20" vertpos="3200" spacing="600"/></hp:linesegarray>"#;

#[test]
fn drops_the_line_layout_of_a_paragraph_whose_length_changes() {
    let runs = r#"<hp:run charPrIDRef="12"><hp:t>가나다라마바사아자차카타파하</hp:t></hp:run>"#;
    let other = format!(
        r#"<hp:p paraPrIDRef="0"><hp:run charPrIDRef="12"><hp:t>다음 문단</hp:t></hp:run>{LINE}</hp:p>"#
    );
    let before = section(&format!("{}{other}", two_line_paragraph(runs)));
    let bytes = package(&before);

    for (search, replacement) in [("나다", "나다다다"), ("나다라", "나"), ("자차카", "자")]
    {
        let edited = replace_text(&bytes, search, replacement, None).expect("replace");
        assert_eq!(
            read(&edited, "Contents/section0.xml"),
            before.replace(search, replacement).replace(THREE_LINES, ""),
            "{search} -> {replacement}"
        );
        let parsed = parse(&edited, &ParseOptions::default()).expect("parse");
        let paragraphs = &parsed.document.sections[0].paragraphs;
        assert!(paragraphs[0].line_segs.is_empty());
        assert_eq!(paragraphs[1].line_segs.len(), 1);
    }
}

#[test]
fn keeps_line_starts_when_the_length_is_the_same() {
    let runs = r#"<hp:run charPrIDRef="12"><hp:t>가나다라마바사아자차카타파하</hp:t></hp:run>"#;
    let before = section(&two_line_paragraph(runs));
    let bytes = package(&before);
    let edited = replace_text(&bytes, "라마바사아자", "라마바사아차", None).expect("replace");
    assert_eq!(
        read(&edited, "Contents/section0.xml"),
        before.replace("라마바사아자", "라마바사아차")
    );
}

#[test]
fn changes_the_length_even_when_earlier_content_hides_the_line_position() {
    let runs = r#"<hp:run charPrIDRef="12"><hp:ctrl><hp:colPr/></hp:ctrl></hp:run><hp:run charPrIDRef="12"><hp:t>가나다라마바사아자차카타파하</hp:t></hp:run>"#;
    let before = section(&two_line_paragraph(runs));
    let bytes = package(&before);
    let edited = replace_text(&bytes, "나다", "나다다다", None).expect("replace");
    assert_eq!(
        read(&edited, "Contents/section0.xml"),
        before.replace("나다", "나다다다").replace(THREE_LINES, "")
    );
    let edited = replace_text(&bytes, "나다", "다나", None).expect("same length");
    assert_eq!(
        read(&edited, "Contents/section0.xml"),
        before.replace("나다", "다나")
    );
}

#[test]
fn edits_text_inside_a_table_cell_without_touching_the_outer_paragraph_lines() {
    let body = format!(
        r#"<hp:p paraPrIDRef="0"><hp:run charPrIDRef="12"><hp:tbl rowCnt="1" colCnt="1"><hp:tr><hp:tc><hp:subList><hp:p paraPrIDRef="5"><hp:run charPrIDRef="13"><hp:t>점수 67</hp:t></hp:run>{LINE}</hp:p></hp:subList></hp:tc></hp:tr></hp:tbl></hp:run><hp:linesegarray><hp:lineseg textpos="0" vertpos="0"/><hp:lineseg textpos="8" vertpos="9000"/></hp:linesegarray></hp:p>"#
    );
    let before = section(&body);
    let bytes = package(&before);
    let edited = replace_text(&bytes, "67", "6.7", None).expect("replace");
    // 칸 안 문단의 줄 배치만 지우고 표를 품은 바깥 문단의 줄 배치는 그대로 둔다.
    assert_eq!(
        read(&edited, "Contents/section0.xml"),
        before.replace("67", "6.7").replace(LINE, "")
    );
}

#[test]
fn refuses_documents_with_tracked_changes() {
    let body = format!(
        r#"<hp:p paraPrIDRef="0"><hp:run charPrIDRef="12"><hp:t>본문 <hp:deleteBegin id="1"/>지운 글<hp:deleteEnd id="1"/></hp:t></hp:run>{LINE}</hp:p>"#
    );
    let bytes = package(&section(&body));
    assert_eq!(
        replace_text(&bytes, "본문", "본", None),
        Err(EditError::TrackedChanges)
    );
}

#[test]
fn splits_text_around_inline_tabs_into_separate_pieces() {
    let body = format!(
        r#"<hp:p paraPrIDRef="0"><hp:run charPrIDRef="12"><hp:t>왼쪽<hp:tab width="100" leader="0" type="1"/>오른쪽 67</hp:t></hp:run>{LINE}</hp:p>"#
    );
    let before = section(&body);
    let bytes = package(&before);
    assert_eq!(
        replace_text(&bytes, "왼쪽오른쪽", "x", None),
        Err(EditError::SpansRuns)
    );
    let edited = replace_text(&bytes, "67", "6.7", None).expect("replace");
    assert_eq!(
        read(&edited, "Contents/section0.xml"),
        before.replace("67", "6.7").replace(LINE, "")
    );
}

#[test]
fn rejects_files_that_are_not_hwpx() {
    assert_eq!(
        replace_text(b"not a zip at all, just text", "a", "b", None),
        Err(EditError::NotHwpx)
    );
    let hwp = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/synthetic-password-v0.hwp"
    ))
    .expect("fixture");
    assert_eq!(find_text(&hwp, "a"), Err(EditError::NotHwpx));
}

#[test]
fn refuses_encrypted_packages() {
    let body = section(&name_paragraph());
    let header = odfcrypt::seal(
        "Contents/header.xml",
        HEADER.as_bytes(),
        "비밀번호",
        &[1; 16],
        &[2; 16],
    );
    let sealed = odfcrypt::seal(
        "Contents/section0.xml",
        body.as_bytes(),
        "비밀번호",
        &[3; 16],
        &[4; 16],
    );
    let manifest = odfcrypt::manifest(&[&header, &sealed]);
    let bytes = zipw::build(&[
        Entry {
            name: "mimetype",
            data: b"application/hwp+zip",
            deflate: false,
        },
        Entry {
            name: "Contents/header.xml",
            data: &header.data,
            deflate: false,
        },
        Entry {
            name: "Contents/section0.xml",
            data: &sealed.data,
            deflate: false,
        },
        Entry {
            name: "META-INF/manifest.xml",
            data: manifest.as_bytes(),
            deflate: true,
        },
    ]);
    assert_eq!(
        replace_text(&bytes, "김햔지", "김현지", None),
        Err(EditError::Encrypted)
    );
}

fn items(bytes: &[u8], paragraph: usize) -> Vec<RunItem> {
    let parsed = parse(bytes, &ParseOptions::default()).expect("parse");
    parsed.document.sections[0].paragraphs[paragraph]
        .runs
        .iter()
        .flat_map(|run| run.items.clone())
        .collect()
}

#[test]
fn inserts_and_removes_line_breaks_inside_one_text_piece() {
    let body = format!(
        r#"<hp:p paraPrIDRef="0"><hp:run charPrIDRef="12"><hp:t>첫 줄<hp:lineBreak/>둘째 줄</hp:t></hp:run>{LINE}</hp:p>"#
    );
    let before = section(&body);
    let bytes = package(&before);

    // 줄 바꿈은 글자 `\n`으로 찾는다. 줄 바꿈을 넘는 글도 한 조각이다.
    let found = find_text(&bytes, "줄\n둘").expect("find across the break");
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].paragraph, "첫 줄\n둘째 줄");

    // 줄 바꿈을 지운다.
    let joined = replace_text(&bytes, "줄\n둘", "줄 둘", None).expect("remove break");
    assert_eq!(
        read(&joined, "Contents/section0.xml"),
        before
            .replace("줄<hp:lineBreak/>둘", "줄 둘")
            .replace(LINE, "")
    );
    assert_eq!(
        items(&joined, 0),
        vec![RunItem::Text("첫 줄 둘째 줄".into())]
    );

    // 줄 바꿈을 넣는다. 같은 앞붙이의 요소가 된다.
    let split = replace_text(&bytes, "둘째", "둘\n째", None).expect("insert break");
    let xml = read(&split, "Contents/section0.xml");
    assert!(xml.contains("둘<hp:lineBreak/>째 줄"), "{xml}");
    assert!(!xml.contains("linesegarray"));
    assert_eq!(
        items(&split, 0),
        vec![
            RunItem::Text("첫 줄".into()),
            RunItem::LineBreak,
            RunItem::Text("둘".into()),
            RunItem::LineBreak,
            RunItem::Text("째 줄".into()),
        ]
    );
    assert_container_is_consistent(&split);

    // 탭은 여전히 넣지 못한다.
    assert_eq!(
        replace_text(&bytes, "첫", "첫\t", None),
        Err(EditError::InvalidReplacement)
    );
}

#[test]
fn fills_an_empty_paragraph_found_by_its_origin() {
    let body = format!(
        r#"<hp:p paraPrIDRef="0"><hp:run charPrIDRef="12"><hp:t>위</hp:t></hp:run>{LINE}</hp:p><hp:p paraPrIDRef="0"><hp:run charPrIDRef="13"/>{LINE}</hp:p><hp:p paraPrIDRef="0"><hp:run charPrIDRef="12"><hp:tbl rowCnt="1" colCnt="1"><hp:tr><hp:tc><hp:subList><hp:p paraPrIDRef="5"><hp:run charPrIDRef="12"></hp:run>{LINE}</hp:p></hp:subList></hp:tc></hp:tr></hp:tbl></hp:run>{LINE}</hp:p><hp:p paraPrIDRef="0">{LINE}</hp:p>"#
    );
    let bytes = package(&section(&body));
    let parsed = parse(&bytes, &ParseOptions::default()).expect("parse");
    let paragraphs = &parsed.document.sections[0].paragraphs;
    let origins: Vec<Option<ParagraphOrigin>> = paragraphs.iter().map(|p| p.origin).collect();
    let at = |ordinal| {
        Some(ParagraphOrigin {
            section: 0,
            ordinal,
        })
    };
    // 표 칸 안 문단(차례 3)도 세므로 마지막 문단은 4번째다.
    assert_eq!(origins, vec![at(0), at(1), at(2), at(4)]);
    let cell = match &paragraphs[2].runs[0].items[0] {
        RunItem::Table(table) => table.cells[0].paragraphs[0].origin,
        other => panic!("table expected, got {other:?}"),
    };
    assert_eq!(cell, at(3));

    // 빈 run(`<hp:run …/>`)을 열어 글을 넣고 그 문단의 줄 배치를 지운다.
    let filled = fill_empty_paragraph(&bytes, 0, 1, "새 글\n둘째 줄").expect("fill");
    let xml = read(&filled, "Contents/section0.xml");
    assert!(
        xml.contains(
            r#"<hp:run charPrIDRef="13"><hp:t>새 글<hp:lineBreak/>둘째 줄</hp:t></hp:run></hp:p>"#
        ),
        "{xml}"
    );
    // 여는 태그와 닫는 태그: 다섯 문단 가운데 그 문단만 줄 배치를 잃는다.
    assert_eq!(
        xml.matches("linesegarray>").count(),
        2 * 4,
        "only that paragraph loses its lines"
    );
    assert_container_is_consistent(&filled);

    // 표 칸 안의 빈 문단, run 없는 문단.
    let cell = fill_empty_paragraph(&bytes, 0, 3, "칸").expect("fill cell");
    assert!(read(&cell, "Contents/section0.xml")
        .contains(r#"<hp:run charPrIDRef="12"><hp:t>칸</hp:t></hp:run>"#));
    let bare = fill_empty_paragraph(&bytes, 0, 4, "끝 & <꺾쇠>").expect("fill bare");
    assert!(read(&bare, "Contents/section0.xml")
        .contains(r#"<hp:run charPrIDRef="0"><hp:t>끝 &amp; &lt;꺾쇠&gt;</hp:t></hp:run></hp:p>"#));
    assert_eq!(texts(&bare).last().map(String::as_str), Some("끝 & <꺾쇠>"));

    // 글이 있는 문단, 없는 문단, 없는 구역은 거절한다.
    assert_eq!(
        fill_empty_paragraph(&bytes, 0, 0, "x"),
        Err(EditError::NotEmpty)
    );
    assert_eq!(
        fill_empty_paragraph(&bytes, 0, 9, "x"),
        Err(EditError::NotFound)
    );
    assert_eq!(
        fill_empty_paragraph(&bytes, 1, 1, "x"),
        Err(EditError::NotFound)
    );
    assert_eq!(fill_empty_paragraph(&bytes, 0, 1, "").unwrap(), bytes);
}

#[test]
fn origins_of_the_fixture_point_at_the_paragraphs_the_editor_sees() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/synthetic.hwpx");
    let bytes = std::fs::read(path).expect("fixture");
    let parsed = parse(&bytes, &ParseOptions::default()).expect("parse");
    for section in &parsed.document.sections {
        for paragraph in &section.paragraphs {
            let origin = paragraph.origin.expect("body paragraphs have an origin");
            let empty = paragraph.runs.iter().all(|run| {
                run.items
                    .iter()
                    .all(|item| !matches!(item, RunItem::Text(_)))
            });
            let filled = fill_empty_paragraph(&bytes, origin.section, origin.ordinal, "ㄱ");
            if empty {
                assert!(filled.is_ok(), "{origin:?}: {filled:?}");
            } else {
                assert_eq!(filled, Err(EditError::NotEmpty), "{origin:?}");
            }
        }
    }
}
