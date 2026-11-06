use hwp_core::hwp5::control::{ParseContext, ParseState};
use hwp_core::hwp5::para::parse_paragraph;
use hwp_core::hwp5::record::Record;
use hwp_core::model::{Document, Paragraph, RunItem, Section, Style};
use hwp_core::WarningKind;

fn header(style: u8, column_type: u8) -> Vec<u8> {
    let mut data = vec![0u8; 22];
    data[10] = style;
    data[11] = column_type;
    data
}

fn record(tag: u16, data: Vec<u8>) -> Record {
    Record {
        tag,
        level: 1,
        data,
        children: Vec::new(),
    }
}

fn paragraph(
    children: Vec<Record>,
    style: u8,
    column_type: u8,
    document: &Document,
) -> (Paragraph, Vec<hwp_core::Warning>) {
    let record = Record {
        tag: 66,
        level: 0,
        data: header(style, column_type),
        children,
    };
    let mut section = Section::default();
    let mut state = ParseState::default();
    let mut warnings = Vec::new();
    let parsed = {
        let mut context =
            ParseContext::new(document, &mut section, &mut state, Some(2), &mut warnings);
        parse_paragraph(&record, &mut context)
    };
    (parsed, warnings)
}

fn text(units: &[u16]) -> Record {
    let data = units
        .iter()
        .flat_map(|unit| unit.to_le_bytes())
        .collect::<Vec<_>>();
    record(67, data)
}

fn shapes(entries: &[(u32, u32)]) -> Record {
    let data = entries
        .iter()
        .flat_map(|(position, id)| {
            position
                .to_le_bytes()
                .into_iter()
                .chain(id.to_le_bytes())
                .collect::<Vec<_>>()
        })
        .collect();
    record(68, data)
}

fn line_segments(segments: &[[u32; 9]], trailing: usize) -> Record {
    let mut data = Vec::new();
    for segment in segments {
        for field in segment {
            data.extend_from_slice(&field.to_le_bytes());
        }
    }
    data.extend(std::iter::repeat_n(0u8, trailing));
    record(69, data)
}

#[test]
fn wchar_positions_skip() {
    let document = Document::default();
    let (parsed, warnings) = paragraph(
        vec![
            text(&[0, 25, 26, 27, 28, 29, 13, 65]),
            line_segments(&[[7, 0, 1000, 1000, 850, 600, 0, 42520, 0]], 0),
        ],
        0,
        0,
        &document,
    );
    assert!(warnings.is_empty());
    assert_eq!(parsed.runs.len(), 1);
    assert_eq!(parsed.runs[0].char_pr_id, 0);
    assert_eq!(
        parsed.runs[0].items,
        vec![
            RunItem::Skip(1),
            RunItem::Skip(1),
            RunItem::Skip(1),
            RunItem::Skip(1),
            RunItem::Skip(1),
            RunItem::Skip(1),
            RunItem::Text("A".to_string()),
        ]
    );
    assert_eq!(parsed.line_segs.len(), 1);
    assert_eq!(parsed.line_segs[0].textpos, 7);

    let control = record(71, [0x64, 0x63, 0x65, 0x73, 0, 0, 0, 0].to_vec());
    let (field, warnings) = paragraph(
        vec![
            text(&[18, 0, 0, 0, 0, 0, 0, 0, 88, 89]),
            shapes(&[(0, 0), (8, 1), (9, 2)]),
            control,
            line_segments(
                &[
                    [0, 0, 1000, 1000, 850, 600, 0, 42520, 0],
                    [8, 1600, 1000, 1000, 850, 600, 0, 42520, 0],
                    [9, 3200, 1000, 1000, 850, 600, 0, 42520, 0],
                ],
                0,
            ),
        ],
        0,
        0,
        &document,
    );
    assert!(warnings.is_empty());
    assert_eq!(
        field
            .runs
            .iter()
            .map(|run| run.char_pr_id)
            .collect::<Vec<_>>(),
        vec![0, 1, 2]
    );
    assert_eq!(field.runs[0].items, vec![RunItem::Skip(8)]);
    assert_eq!(field.runs[1].items, vec![RunItem::Text("X".to_string())]);
    assert_eq!(field.runs[2].items, vec![RunItem::Text("Y".to_string())]);
    assert_eq!(
        field
            .line_segs
            .iter()
            .map(|segment| segment.textpos)
            .collect::<Vec<_>>(),
        vec![0, 8, 9]
    );
}

#[test]
fn tab_inline_width_leader_kind() {
    let document = Document::default();
    let mut words = vec![9, 1980, 0, 0, 0, 0, 0, 0, 65];
    words.extend([9, 1176, 0, 0, 0, 0, 0, 0, 66]);
    words.extend([9, 1200, 0, 0x0303, 0, 0, 0, 0, 67]);
    words.extend([9, 0xFFFF, 0xFFFF, 0, 0, 0, 0, 0, 68]);
    words.push(9);
    let (parsed, warnings) = paragraph(vec![text(&words)], 0, 0, &document);
    assert!(warnings.is_empty());
    let items = &parsed.runs[0].items;
    assert_eq!(items[0], RunItem::SizedTab(1980.0));
    assert_eq!(items[1], RunItem::Text("A".to_string()));
    assert_eq!(items[2], RunItem::SizedTab(1176.0));
    assert_eq!(items[3], RunItem::Text("B".to_string()));
    match &items[4] {
        RunItem::FormattedTab(tab) => {
            assert_eq!(tab.width, Some(1200.0));
            assert_eq!(tab.kind, "CENTER");
            assert_eq!(tab.leader, "DOT");
        }
        other => panic!("unexpected tab item: {other:?}"),
    }
    assert_eq!(items[5], RunItem::Text("C".to_string()));
    assert_eq!(items[6], RunItem::Tab);
    assert_eq!(items[7], RunItem::Text("D".to_string()));
    assert_eq!(items[8], RunItem::Tab);
}

#[test]
fn surrogate_pairs() {
    let document = Document::default();
    let (parsed, warnings) = paragraph(
        vec![text(&[0xD83D, 0xDE00, 0x0041, 0x00B0])],
        0,
        0,
        &document,
    );
    assert!(warnings.is_empty());
    assert_eq!(parsed.runs.len(), 1);
    assert_eq!(
        parsed.runs[0].items,
        vec![RunItem::Text("\u{1F600}A\u{00B0}".to_string())]
    );
}

#[test]
fn lone_surrogate_warns() {
    let document = Document::default();
    let (parsed, warnings) = paragraph(
        vec![text(&[0xD800, 0x0041, 0xDC00, 0xD83D, 0x0042])],
        0,
        0,
        &document,
    );
    assert_eq!(
        parsed.runs[0].items,
        vec![RunItem::Text("\u{FFFD}A\u{FFFD}\u{FFFD}B".to_string())]
    );
    assert_eq!(warnings.len(), 3);
    assert!(warnings
        .iter()
        .all(|warning| warning.kind == WarningKind::LoneSurrogate));
    assert!(warnings.iter().all(|warning| warning.section == Some(2)));
}

#[test]
fn char_shape_runs() {
    let document = Document::default();
    let (parsed, warnings) = paragraph(
        vec![text(&[97, 98, 99, 100]), shapes(&[(0, 2), (2, 5), (2, 5)])],
        0,
        0,
        &document,
    );
    assert!(warnings.is_empty());
    assert_eq!(parsed.runs.len(), 2);
    assert_eq!(parsed.runs[0].char_pr_id, 2);
    assert_eq!(parsed.runs[0].items, vec![RunItem::Text("ab".to_string())]);
    assert_eq!(parsed.runs[1].char_pr_id, 5);
    assert_eq!(parsed.runs[1].items, vec![RunItem::Text("cd".to_string())]);

    let mut styled = Document::default();
    styled.styles.push(Style {
        id: 0,
        para_pr_id: 0,
        char_pr_id: 7,
        ..Style::default()
    });
    let (fallback, _) = paragraph(vec![text(&[65])], 0, 0, &styled);
    assert_eq!(fallback.runs.len(), 1);
    assert_eq!(fallback.runs[0].char_pr_id, 7);
}

#[test]
fn line_segs_36_bytes() {
    let document = Document::default();
    let (parsed, warnings) = paragraph(
        vec![
            text(&[65]),
            line_segments(
                &[
                    [0, 10, 1000, 900, 800, 600, 12, 3456, 1],
                    [7, 20, 2000, 1900, 1800, 1600, 22, 4456, 2],
                ],
                35,
            ),
        ],
        0,
        0,
        &document,
    );
    assert!(warnings.is_empty());
    assert_eq!(parsed.line_segs.len(), 2);
    assert_eq!(parsed.line_segs[0].textpos, 0);
    assert_eq!(parsed.line_segs[0].vertpos, 10.0);
    assert_eq!(parsed.line_segs[0].vertsize, 1000.0);
    assert_eq!(parsed.line_segs[0].textheight, 900.0);
    assert_eq!(parsed.line_segs[0].baseline, 800.0);
    assert_eq!(parsed.line_segs[0].spacing, 600.0);
    assert_eq!(parsed.line_segs[0].horzpos, 12.0);
    assert_eq!(parsed.line_segs[0].horzsize, 3456.0);
    assert_eq!(parsed.line_segs[0].flags, 1);
    assert_eq!(parsed.line_segs[1].textpos, 7);
    assert_eq!(parsed.line_segs[1].flags, 2);
}

fn memo_list(shape_id: u32) -> Record {
    Record {
        tag: 93,
        level: 1,
        data: shape_id.to_le_bytes().to_vec(),
        children: Vec::new(),
    }
}

fn list_header() -> Record {
    Record {
        tag: 72,
        level: 1,
        data: vec![0; 16],
        children: Vec::new(),
    }
}

fn nested_paragraph(units: &[u16]) -> Record {
    Record {
        tag: 66,
        level: 2,
        data: header(0, 0),
        children: vec![text(units)],
    }
}

#[test]
fn memo_list_attached_to_paragraph() {
    let document = Document::default();
    let (parsed, warnings) = paragraph(
        vec![
            text(&[0x0062, 0x006F, 0x0064, 0x0079, 13]),
            memo_list(3),
            list_header(),
            nested_paragraph(&[0x006D, 0x0065, 0x006D, 0x006F]),
        ],
        0,
        0,
        &document,
    );
    assert!(warnings.is_empty());
    assert_eq!(parsed.memos.len(), 1);
    assert_eq!(parsed.memos[0].shape_id, 3);
    assert_eq!(
        parsed.memos[0].paragraphs[0].runs[0].items,
        vec![RunItem::Text("memo".to_string())]
    );
}

#[test]
fn memo_not_in_body_runs() {
    let document = Document::default();
    let (parsed, warnings) = paragraph(
        vec![
            text(&[0x0062, 0x006F, 0x0064, 0x0079, 13]),
            line_segments(&[[5, 0, 1000, 1000, 850, 600, 0, 42520, 0]], 0),
            memo_list(1),
            list_header(),
            nested_paragraph(&[0x006D, 0x0065, 0x006D, 0x006F]),
        ],
        0,
        0,
        &document,
    );
    assert!(warnings.is_empty());
    assert_eq!(
        parsed.runs[0].items,
        vec![RunItem::Text("body".to_string())]
    );
    assert_eq!(parsed.runs.len(), 1);
    assert_eq!(parsed.line_segs.len(), 1);
    assert_eq!(parsed.line_segs[0].textpos, 5);

    let mut summary_document = Document::default();
    summary_document.sections.push(Section {
        paragraphs: vec![parsed],
        ..Section::default()
    });
    let summary = hwp_core::text::text_summary(&summary_document);
    assert_eq!(
        summary.paragraphs,
        vec!["body".to_string(), "memo".to_string()]
    );
}
