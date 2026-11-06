use hwp_core::hwp5::control::{parse_control, ParseContext, ParseState};
use hwp_core::hwp5::record::Record;
use hwp_core::model::{Document, Margins, RunItem, Section};
use hwp_core::Warning;

fn u16_bytes(value: u16) -> [u8; 2] {
    value.to_le_bytes()
}

fn u32_bytes(value: u32) -> [u8; 4] {
    value.to_le_bytes()
}

fn i16_bytes(value: i16) -> [u8; 2] {
    value.to_le_bytes()
}

fn i32_bytes(value: i32) -> [u8; 4] {
    value.to_le_bytes()
}

fn record(tag: u16, level: u16, data: Vec<u8>, children: Vec<Record>) -> Record {
    Record {
        tag,
        level,
        data,
        children,
    }
}

fn control(id: &[u8; 4], extra: &[u8], children: Vec<Record>) -> Record {
    let mut data: Vec<u8> = id.iter().rev().copied().collect();
    data.extend_from_slice(extra);
    record(71, 1, data, children)
}

#[test]
fn section_control_reads_the_outline_numbering_id() {
    let mut data = vec![0u8; 14];
    data.extend(7u16.to_le_bytes());
    let mut section = Section::default();
    let mut state = ParseState::default();
    let record = control(b"secd", &data, Vec::new());
    assert_eq!(run(&record, &mut section, &mut state), None);
    assert_eq!(section.outline_numbering_id, Some(7));

    let mut section = Section::default();
    let record = control(b"secd", &data[..15], Vec::new());
    assert_eq!(run(&record, &mut section, &mut state), None);
    assert_eq!(section.outline_numbering_id, None);
}

fn paragraph(style: u8) -> Record {
    let mut data = vec![0u8; 22];
    data[10] = style;
    record(66, 3, data, Vec::new())
}

fn text_paragraph(style: u8, text: &str) -> Record {
    let mut data = vec![0u8; 22];
    data[10] = style;
    let text = text
        .encode_utf16()
        .flat_map(|unit| unit.to_le_bytes())
        .collect();
    record(66, 3, data, vec![record(67, 4, text, Vec::new())])
}

fn caption_header(props: u32, width: u32, gap: u16, children: Vec<Record>) -> Record {
    let mut data = Vec::new();
    data.extend_from_slice(&u16_bytes(children.len() as u16));
    data.extend_from_slice(&u32_bytes(0));
    data.extend_from_slice(&u16_bytes(0));
    data.extend_from_slice(&u32_bytes(props));
    data.extend_from_slice(&u32_bytes(width));
    data.extend_from_slice(&u16_bytes(gap));
    data.extend_from_slice(&u32_bytes(0));
    record(72, 2, data, children)
}

fn list_header(attr: u32, cell: &[u8], children: Vec<Record>) -> Record {
    let mut data = Vec::new();
    data.extend_from_slice(&i16_bytes(0));
    data.extend_from_slice(&u16_bytes(0));
    data.extend_from_slice(&u32_bytes(attr));
    data.extend_from_slice(cell);
    record(72, 2, data, children)
}

fn cell_body(
    position: [u16; 4],
    size: [u32; 2],
    margins: [i16; 4],
    border_fill_id: u16,
) -> Vec<u8> {
    let mut data = Vec::new();
    for value in position {
        data.extend_from_slice(&u16_bytes(value));
    }
    for value in size {
        data.extend_from_slice(&u32_bytes(value));
    }
    for margin in margins {
        data.extend_from_slice(&i16_bytes(margin));
    }
    data.extend_from_slice(&u16_bytes(border_fill_id));
    data
}

fn table(attr: u32, rows: u16, cols: u16, border_fill_id: u16) -> Record {
    let mut data = Vec::new();
    data.extend_from_slice(&u32_bytes(attr));
    data.extend_from_slice(&u16_bytes(rows));
    data.extend_from_slice(&u16_bytes(cols));
    data.extend_from_slice(&u16_bytes(0));
    for margin in [10i16, 20, 30, 40] {
        data.extend_from_slice(&i16_bytes(margin));
    }
    for _ in 0..rows {
        data.extend_from_slice(&u16_bytes(0));
    }
    data.extend_from_slice(&u16_bytes(border_fill_id));
    record(77, 2, data, Vec::new())
}

fn common(width: u32, height: u32) -> Vec<u8> {
    let mut data = Vec::new();
    data.extend_from_slice(&u32_bytes(0));
    data.extend_from_slice(&i32_bytes(0));
    data.extend_from_slice(&i32_bytes(0));
    data.extend_from_slice(&u32_bytes(width));
    data.extend_from_slice(&u32_bytes(height));
    data.extend_from_slice(&i32_bytes(3));
    for margin in [1i16, 2, 3, 4] {
        data.extend_from_slice(&i16_bytes(margin));
    }
    data.extend_from_slice(&u32_bytes(0));
    data
}

fn table_control(children: Vec<Record>) -> Record {
    let mut extra = common(10_000, 20_000);
    extra.extend_from_slice(&u32_bytes(0));
    control(b"tbl ", &extra, children)
}

fn run(record: &Record, section: &mut Section, state: &mut ParseState) -> Option<RunItem> {
    let mut warnings: Vec<Warning> = Vec::new();
    let document = Document::default();
    let mut context = ParseContext::new(&document, section, state, Some(0), &mut warnings);
    parse_control(record, &mut context)
}

fn table_of(item: Option<RunItem>) -> hwp_core::model::Table {
    match item {
        Some(RunItem::Table(table)) => table,
        other => panic!("expected table, got {other:?}"),
    }
}

#[test]
fn table_cells_after_table_record() {
    let caption = list_header(0, &cell_body([9, 9, 1, 1], [1, 1], [0; 4], 1), Vec::new());
    let first = list_header(
        0,
        &cell_body([0, 0, 1, 1], [100, 200], [1, 2, 3, 4], 7),
        Vec::new(),
    );
    let second = list_header(
        0,
        &cell_body([1, 0, 1, 1], [300, 400], [5, 6, 7, 8], 8),
        vec![paragraph(11)],
    );
    let record = table_control(vec![
        caption,
        table(0, 1, 2, 5),
        first,
        paragraph(9),
        second,
    ]);
    let mut section = Section::default();
    let mut state = ParseState::default();
    let table = table_of(run(&record, &mut section, &mut state));
    assert_eq!(table.cells.len(), 2);
    assert_eq!(table.cells[0].col, 0);
    assert_eq!(table.cells[0].paragraphs.len(), 1);
    assert_eq!(table.cells[0].paragraphs[0].style_id, 9);
    assert_eq!(table.cells[1].col, 1);
    assert_eq!(table.cells[1].paragraphs.len(), 1);
    assert_eq!(table.cells[1].paragraphs[0].style_id, 11);
    assert_eq!(table.rows, 1);
    assert_eq!(table.cols, 2);
    assert_eq!(table.in_margins.left, 10.0);
    assert_eq!(table.in_margins.bottom, 40.0);
    assert_eq!(table.border_fill_id, 5);
    assert_eq!(table.pos.vert_align, "TOP");
    assert_eq!(table.z_order, 3);
}

#[test]
fn hwp5_table_caption_before_table() {
    let caption = caption_header(2, 8504, 850, Vec::new());
    let cell = list_header(
        0,
        &cell_body([0, 0, 1, 1], [100, 200], [1, 2, 3, 4], 7),
        vec![paragraph(9)],
    );
    let record = table_control(vec![
        caption,
        text_paragraph(3, "table caption"),
        table(0, 1, 1, 5),
        cell,
    ]);
    let mut section = Section::default();
    let mut state = ParseState::default();
    let table = table_of(run(&record, &mut section, &mut state));
    assert_eq!(table.cells.len(), 1);
    assert_eq!(table.cells[0].paragraphs[0].style_id, 9);
    let caption = table.caption.expect("table caption");
    assert_eq!(caption.side, "TOP");
    assert_eq!(caption.width, 8504.0);
    assert_eq!(caption.gap, 850.0);
    assert!(!caption.full_size);
    assert_eq!(caption.paragraphs.len(), 1);
    assert_eq!(caption.paragraphs[0].style_id, 3);
    assert_eq!(
        caption.paragraphs[0].runs[0].items,
        vec![RunItem::Text("table caption".to_string())]
    );
}

#[test]
fn cell_list_header_layout() {
    let attr = (2 << 5) | (1 << 16) | (1 << 18);
    let cell = list_header(
        attr,
        &cell_body([3, 4, 2, 5], [1234, 5678], [11, 12, 13, 14], 21),
        Vec::new(),
    );
    let record = table_control(vec![table(2, 1, 1, 0), cell]);
    let mut section = Section::default();
    let mut state = ParseState::default();
    let table = table_of(run(&record, &mut section, &mut state));
    assert_eq!(table.page_break_mode, "CELL");
    let cell = &table.cells[0];
    assert_eq!(cell.col, 3);
    assert_eq!(cell.row, 4);
    assert_eq!(cell.col_span, 2);
    assert_eq!(cell.row_span, 5);
    assert_eq!(cell.width, 1234.0);
    assert_eq!(cell.height, 5678.0);
    assert_eq!(cell.margins.left, 11.0);
    assert_eq!(cell.margins.right, 12.0);
    assert_eq!(cell.margins.top, 13.0);
    assert_eq!(cell.margins.bottom, 14.0);
    assert_eq!(cell.border_fill_id, 21);
    assert_eq!(cell.vert_align, "BOTTOM");
    assert!(cell.header);
    assert_eq!(table.cols, 5);
    assert_eq!(table.rows, 9);
    assert!(!table.repeat_header);
}

#[test]
fn cell_margin_unset_uses_table() {
    for attr in [0, 1 << 16, (1 << 17) | (1 << 18), u32::MAX] {
        for own in [[11, 12, 13, 14], [10, 20, 30, 40], [0; 4]] {
            let cell = list_header(
                attr,
                &cell_body([0, 0, 1, 1], [100, 200], own, 7),
                Vec::new(),
            );
            let record = table_control(vec![table(0, 1, 1, 5), cell]);
            let table = table_of(run(
                &record,
                &mut Section::default(),
                &mut ParseState::default(),
            ));
            assert_eq!(table.cells.len(), 1);
            let cell = &table.cells[0];
            let has_margin = attr & 0x0001_0000 != 0;
            assert_eq!(cell.has_margin, has_margin, "attr={attr:#x}");
            let expected = if has_margin { own } else { [10, 20, 30, 40] };
            assert_eq!(
                cell.margins,
                Margins {
                    left: f64::from(expected[0]),
                    right: f64::from(expected[1]),
                    top: f64::from(expected[2]),
                    bottom: f64::from(expected[3]),
                },
                "attr={attr:#x}, own={own:?}"
            );
        }
    }
}

#[test]
fn page_def_landscape_swap() {
    let mut data = Vec::new();
    for value in [
        59_528u32, 84_186, 8_504, 8_504, 5_668, 4_252, 4_252, 4_252, 0,
    ] {
        data.extend_from_slice(&u32_bytes(value));
    }
    data.extend_from_slice(&u32_bytes(1));
    let page_def = record(73, 2, data, Vec::new());
    let record = control(b"secd", &[], vec![page_def]);
    let mut section = Section::default();
    let mut state = ParseState::default();
    assert_eq!(run(&record, &mut section, &mut state), None);
    assert_eq!(section.page.width, 84_186.0);
    assert_eq!(section.page.height, 59_528.0);
    assert_eq!(section.page.margin_left, 8_504.0);
    assert_eq!(section.page.margin_bottom, 4_252.0);
    assert_eq!(section.page.header, 4_252.0);
    assert_eq!(section.page.footer, 4_252.0);
    assert_eq!(section.page.gutter, 0.0);
    assert!(section.page.landscape);
}

#[test]
fn footnote_shape_and_seq() {
    let mut shape = Vec::new();
    shape.extend_from_slice(&u32_bytes(3));
    for _ in 0..4 {
        shape.extend_from_slice(&u16_bytes(0));
    }
    shape.extend_from_slice(&i32_bytes(-1));
    shape.extend_from_slice(&u16_bytes(850));
    shape.extend_from_slice(&u16_bytes(567));
    shape.extend_from_slice(&u16_bytes(283));
    shape.push(1);
    shape.push(2);
    shape.extend_from_slice(&u32_bytes(0x0000FF));
    let footnote_shape = record(74, 2, shape, Vec::new());
    let section_control = control(b"secd", &[], vec![footnote_shape]);

    let mut section = Section::default();
    let mut state = ParseState::default();
    assert_eq!(run(&section_control, &mut section, &mut state), None);
    assert_eq!(section.footnote_shape.format, 3);
    assert_eq!(section.footnote_shape.line_length, -1.0);
    assert_eq!(section.footnote_shape.above_line, 850.0);
    assert_eq!(section.footnote_shape.below_line, 567.0);
    assert_eq!(section.footnote_shape.between_notes, 283.0);
    assert_eq!(section.footnote_shape.line_type, 1);
    assert_eq!(section.footnote_shape.line_width, 0.15);
    assert_eq!(section.footnote_shape.line_color, 0xFF0000);

    let mut note = Vec::new();
    note.extend_from_slice(&u32_bytes(7));
    note.extend_from_slice(&u16_bytes(u16::from(b'*')));
    note.extend_from_slice(&u16_bytes(u16::from(b'(')));
    let first = control(b"fn  ", &note, Vec::new());
    let mut endnote = Vec::new();
    endnote.extend_from_slice(&u32_bytes(2));
    endnote.extend_from_slice(&u16_bytes(0));
    endnote.extend_from_slice(&u16_bytes(0));
    let second = control(b"en  ", &endnote, Vec::new());
    let third = control(b"fn  ", &note, Vec::new());

    let first = match run(&first, &mut section, &mut state) {
        Some(RunItem::Note(note)) => note,
        other => panic!("expected note, got {other:?}"),
    };
    assert!(!first.endnote);
    assert_eq!(first.number, 7);
    assert_eq!(first.seq, 1);
    assert_eq!(first.format, 3);
    assert_eq!(first.prefix, "*");
    assert_eq!(first.suffix, "(");

    let second = match run(&second, &mut section, &mut state) {
        Some(RunItem::Note(note)) => note,
        other => panic!("expected note, got {other:?}"),
    };
    assert!(second.endnote);
    assert_eq!(second.number, 2);
    assert_eq!(second.seq, 2);
    assert_eq!(second.format, 0);

    let third = match run(&third, &mut section, &mut state) {
        Some(RunItem::Note(note)) => note,
        other => panic!("expected note, got {other:?}"),
    };
    assert_eq!(third.seq, 3);
}

#[test]
fn header_footer_applies() {
    let mut section = Section::default();
    let mut state = ParseState::default();
    for (attr, applies, style) in [
        (0u32, "BOTH", 1u8),
        (1, "EVEN", 2),
        (2, "ODD", 3),
        (3, "UNKNOWN", 4),
    ] {
        let head = control(
            b"head",
            &u32_bytes(attr),
            vec![list_header(0, &[], vec![paragraph(style)])],
        );
        match run(&head, &mut section, &mut state) {
            Some(RunItem::Header {
                paragraphs,
                applies: got,
            }) => {
                assert_eq!(got, applies);
                assert_eq!(paragraphs[0].style_id, i32::from(style));
            }
            other => panic!("expected header, got {other:?}"),
        }
        assert_eq!(section.header.len(), 1);
        assert_eq!(section.header[0].style_id, 1);
    }

    let foot = control(
        b"foot",
        &u32_bytes(2),
        vec![list_header(0, &[], vec![paragraph(8)])],
    );
    match run(&foot, &mut section, &mut state) {
        Some(RunItem::Footer {
            paragraphs,
            applies,
        }) => {
            assert_eq!(applies, "ODD");
            assert_eq!(paragraphs[0].style_id, 8);
        }
        other => panic!("expected footer, got {other:?}"),
    }
    assert_eq!(section.footer.len(), 1);
    assert_eq!(section.footer[0].style_id, 8);
}

#[test]
fn autonum_page_and_number() {
    let mut section = Section::default();
    let mut state = ParseState::default();

    let auto = control(b"atno", &[0, 0, 0, 0, 0, 0], Vec::new());
    assert_eq!(
        run(&auto, &mut section, &mut state),
        Some(RunItem::AutoNum("PAGE".to_string()))
    );

    let mut numbered = Vec::new();
    numbered.extend_from_slice(&u32_bytes(1));
    numbered.extend_from_slice(&u16_bytes(7));
    numbered.extend_from_slice(&u16_bytes(0));
    numbered.extend_from_slice(&u16_bytes(u16::from(b'*')));
    numbered.extend_from_slice(&u16_bytes(u16::from(b')')));
    let numbered = control(b"atno", &numbered, Vec::new());
    assert_eq!(
        run(&numbered, &mut section, &mut state),
        Some(RunItem::Text("*7)".to_string()))
    );

    let mut zero = Vec::new();
    zero.extend_from_slice(&u32_bytes(1));
    zero.extend_from_slice(&u16_bytes(0));
    zero.extend_from_slice(&u16_bytes(0));
    zero.extend_from_slice(&u16_bytes(0));
    zero.extend_from_slice(&u16_bytes(0));
    let zero = control(b"atno", &zero, Vec::new());
    assert_eq!(run(&zero, &mut section, &mut state), None);
}

#[test]
fn newnum_page_only() {
    let mut section = Section::default();
    let mut state = ParseState::default();

    let mut page = Vec::new();
    page.extend_from_slice(&u32_bytes(0));
    page.extend_from_slice(&u16_bytes(0));
    let page = control(b"nwno", &page, Vec::new());
    assert_eq!(
        run(&page, &mut section, &mut state),
        Some(RunItem::NewNum(1))
    );

    let mut five = Vec::new();
    five.extend_from_slice(&u32_bytes(0));
    five.extend_from_slice(&u16_bytes(5));
    let five = control(b"nwno", &five, Vec::new());
    assert_eq!(
        run(&five, &mut section, &mut state),
        Some(RunItem::NewNum(5))
    );

    let mut other = Vec::new();
    other.extend_from_slice(&u32_bytes(2));
    other.extend_from_slice(&u16_bytes(3));
    let other = control(b"nwno", &other, Vec::new());
    assert_eq!(run(&other, &mut section, &mut state), None);
}

#[test]
fn first_cold_wins() {
    let mut section = Section::default();
    let mut state = ParseState::default();

    let first = control(b"cold", &[8, 0, 0xE8, 0x03], Vec::new());
    assert_eq!(run(&first, &mut section, &mut state), None);
    assert_eq!(section.columns, 2);
    assert_eq!(section.column_gap, 1000.0);

    let second = control(b"cold", &[12, 0, 0xD0, 0x07], Vec::new());
    assert_eq!(run(&second, &mut section, &mut state), None);
    assert_eq!(section.columns, 2);
    assert_eq!(section.column_gap, 1000.0);

    let mut fresh_section = Section::default();
    let mut fresh_state = ParseState::default();
    let empty = control(b"cold", &[0, 0, 0x10, 0x0E], Vec::new());
    assert_eq!(run(&empty, &mut fresh_section, &mut fresh_state), None);
    assert_eq!(fresh_section.columns, 1);
    let defined = control(b"cold", &[16, 0, 0xBC, 0x02], Vec::new());
    assert_eq!(run(&defined, &mut fresh_section, &mut fresh_state), None);
    assert_eq!(fresh_section.columns, 4);
    assert_eq!(fresh_section.column_gap, 700.0);
}

#[test]
fn compose_control_keeps_text_and_frame() {
    let mut data = Vec::new();
    data.extend_from_slice(&u16_bytes(2));
    data.extend_from_slice(&u16_bytes(u16::from(b'1')));
    data.extend_from_slice(&u16_bytes(u16::from(b'0')));
    data.push(1);
    data.push((-3i8).to_le_bytes()[0]);
    data.push(1);
    let record = control(b"tcmp", &data, Vec::new());
    let mut section = Section::default();
    let mut state = ParseState::default();
    match run(&record, &mut section, &mut state) {
        Some(RunItem::Compose(compose)) => {
            assert_eq!(compose.text, "10");
            assert_eq!(compose.shape, "SHAPE_REVERSAL_CIRCLE");
            assert_eq!(compose.char_size, -3);
            assert!(!compose.overlap);
        }
        other => panic!("expected compose, got {other:?}"),
    }
}
