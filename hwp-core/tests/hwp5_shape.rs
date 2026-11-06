use hwp_core::hwp5::control::{parse_control, ParseContext, ParseState};
use hwp_core::hwp5::record::Record;
use hwp_core::model::{Document, RunItem, Section};
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

fn f64_bytes(value: f64) -> [u8; 8] {
    value.to_le_bytes()
}

fn utf16_bytes(text: &str) -> Vec<u8> {
    text.encode_utf16()
        .flat_map(|unit| unit.to_le_bytes())
        .collect()
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

fn text_box_list(children: Vec<Record>) -> Record {
    let mut data = Vec::new();
    data.extend_from_slice(&i16_bytes(0));
    data.extend_from_slice(&u16_bytes(0));
    data.extend_from_slice(&u32_bytes(0));
    for margin in [1u16, 2, 3, 4] {
        data.extend_from_slice(&u16_bytes(margin));
    }
    record(72, 3, data, children)
}

fn common(attr: u32, width: u32, height: u32) -> Vec<u8> {
    let mut data = Vec::new();
    data.extend_from_slice(&u32_bytes(attr));
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

fn component(
    id: u32,
    matrix: [f64; 6],
    original: (u32, u32),
    current: (u32, u32),
    pairs: &[([f64; 6], [f64; 6])],
    tail: &[u8],
) -> Vec<u8> {
    let mut data = Vec::new();
    data.extend_from_slice(&u32_bytes(id));
    data.extend_from_slice(&u32_bytes(id));
    data.extend_from_slice(&i32_bytes(0));
    data.extend_from_slice(&i32_bytes(0));
    data.extend_from_slice(&u16_bytes(0));
    data.extend_from_slice(&u16_bytes(0));
    data.extend_from_slice(&u32_bytes(original.0));
    data.extend_from_slice(&u32_bytes(original.1));
    data.extend_from_slice(&u32_bytes(current.0));
    data.extend_from_slice(&u32_bytes(current.1));
    data.extend_from_slice(&u32_bytes(0));
    data.extend_from_slice(&i16_bytes(0));
    data.extend_from_slice(&i32_bytes(0));
    data.extend_from_slice(&i32_bytes(0));
    data.extend_from_slice(&u16_bytes(pairs.len() as u16));
    for value in matrix {
        data.extend_from_slice(&f64_bytes(value));
    }
    for (scale, rotation) in pairs {
        for value in scale {
            data.extend_from_slice(&f64_bytes(*value));
        }
        for value in rotation {
            data.extend_from_slice(&f64_bytes(*value));
        }
    }
    data.extend_from_slice(tail);
    data
}

fn identity_matrix() -> [f64; 6] {
    [1.0, 0.0, 0.0, 0.0, 1.0, 0.0]
}

fn translation_matrix(tx: f64, ty: f64) -> [f64; 6] {
    [1.0, 0.0, tx, 0.0, 1.0, ty]
}

fn picture_data(bin: u16, info_size: (u32, u32), trailer: &[u8]) -> Vec<u8> {
    let mut data = Vec::new();
    data.extend_from_slice(&u32_bytes(0));
    data.extend_from_slice(&i32_bytes(0));
    data.extend_from_slice(&u32_bytes(0));
    for point in [(0, 0), (100, 0), (0, 50), (100, 50)] {
        data.extend_from_slice(&i32_bytes(point.0));
        data.extend_from_slice(&i32_bytes(point.1));
    }
    for value in [10, 5, 90, 45] {
        data.extend_from_slice(&i32_bytes(value));
    }
    for _ in 0..4 {
        data.extend_from_slice(&i16_bytes(0));
    }
    data.extend_from_slice(&[0, 0, 0]);
    data.extend_from_slice(&u16_bytes(bin));
    data.push(0);
    data.extend_from_slice(&u32_bytes(0));
    data.extend_from_slice(&u32_bytes(0));
    data.extend_from_slice(&u32_bytes(info_size.0));
    data.extend_from_slice(&u32_bytes(info_size.1));
    data.extend_from_slice(trailer);
    data
}

fn run(
    record: &Record,
    document: &Document,
    section: &mut Section,
    state: &mut ParseState,
) -> Option<RunItem> {
    let mut warnings: Vec<Warning> = Vec::new();
    let mut context = ParseContext::new(document, section, state, Some(0), &mut warnings);
    parse_control(record, &mut context)
}

fn shape_of(item: Option<RunItem>) -> hwp_core::model::Shape {
    match item {
        Some(RunItem::Shape(shape)) => shape,
        other => panic!("expected shape, got {other:?}"),
    }
}

fn picture_of(item: Option<RunItem>) -> hwp_core::model::Picture {
    match item {
        Some(RunItem::Picture(picture)) => picture,
        other => panic!("expected picture, got {other:?}"),
    }
}

fn gso(common: Vec<u8>, component: Record) -> Record {
    control(b"gso ", &common, vec![component])
}

fn shape_with_common_attr(attr: u32) -> hwp_core::model::Shape {
    shape_of(run(
        &gso(
            common(attr, 400, 300),
            record(76, 2, Vec::new(), Vec::new()),
        ),
        &Document::default(),
        &mut Section::default(),
        &mut ParseState::default(),
    ))
}

#[test]
fn common_attr_text_wrap_bits() {
    for (bits, expected) in [
        (0, "SQUARE"),
        (1, "TOP_AND_BOTTOM"),
        (2, "BEHIND_TEXT"),
        (3, "IN_FRONT_OF_TEXT"),
        (4, "SQUARE"),
        (5, "SQUARE"),
        (6, "SQUARE"),
        (7, "SQUARE"),
    ] {
        for neighbors in [0, (1 << 20) | (1 << 24)] {
            let shape = shape_with_common_attr((bits << 21) | neighbors);
            assert_eq!(shape.pos.text_wrap, expected, "wrap code {bits}");
            assert_eq!((shape.width, shape.height), (400.0, 300.0));
        }
    }
}

#[test]
fn size_rel_to() {
    for (width, expected_width) in [
        "PAPER", "PAGE", "COLUMN", "PARA", "ABSOLUTE", "ABSOLUTE", "ABSOLUTE", "ABSOLUTE",
    ]
    .into_iter()
    .enumerate()
    {
        for (height, expected_height) in ["PAPER", "PAGE", "ABSOLUTE", "ABSOLUTE"]
            .into_iter()
            .enumerate()
        {
            let shape = shape_with_common_attr(
                ((width as u32) << 15) | ((height as u32) << 18) | (3 << 21),
            );
            let pos = serde_json::to_value(&shape.pos).expect("serialize position");
            assert_eq!(pos["widthRelTo"], expected_width);
            assert_eq!(pos["heightRelTo"], expected_height);
            assert_eq!(shape.pos.text_wrap, "IN_FRONT_OF_TEXT");
        }
    }
}

#[test]
fn common_attr_alignment_preserves_vert_rel_to() {
    for (vert, expected_vert) in ["PAPER", "PAGE", "PARA", "PARA"].into_iter().enumerate() {
        for (horz, expected_horz) in ["LEFT", "CENTER", "RIGHT", "INSIDE", "OUTSIDE"]
            .into_iter()
            .enumerate()
        {
            let shape = shape_with_common_attr(((vert as u32) << 3) | ((horz as u32) << 10));
            assert_eq!(shape.pos.vert_rel_to, expected_vert);
            assert_eq!(shape.pos.horz_align, expected_horz);
        }
    }
}

#[test]
fn picture_bin_and_orig_size() {
    let picture = record(
        85,
        3,
        picture_data(2, (0, 0), &[200, 0, 0, 0, 100, 0, 0, 0]),
        Vec::new(),
    );
    let component = record(
        76,
        2,
        component(1, identity_matrix(), (500, 300), (500, 300), &[], &[]),
        vec![picture],
    );
    let record = gso(common(0, 400, 300), component);

    let mut section = Section::default();
    let mut state = ParseState::default().with_max_bin(1);
    let missing_bin = picture_of(run(&record, &Document::default(), &mut section, &mut state));
    assert_eq!(missing_bin.binary_id, "bin0");
    assert_eq!(missing_bin.orig_width, 200.0);
    assert_eq!(missing_bin.orig_height, 100.0);
    assert_eq!(missing_bin.clip, Some([[10.0, 5.0], [80.0, 40.0]]));
    assert_eq!(missing_bin.width, 400.0);
    assert_eq!(missing_bin.height, 300.0);

    let mut section = Section::default();
    let mut state = ParseState::default().with_max_bin(3);
    let picture = picture_of(run(&record, &Document::default(), &mut section, &mut state));
    assert_eq!(picture.binary_id, "bin2");
}

#[test]
fn hwp5_picture_caption() {
    let picture = record(85, 3, picture_data(0, (0, 0), &[]), Vec::new());
    let component = record(
        76,
        2,
        component(1, identity_matrix(), (500, 300), (500, 300), &[], &[]),
        vec![picture],
    );
    let record = control(
        b"gso ",
        &common(0, 400, 300),
        vec![
            caption_header(3, 8504, 850, Vec::new()),
            text_paragraph(2, "picture caption"),
            component,
        ],
    );
    let mut section = Section::default();
    let mut state = ParseState::default();
    let picture = picture_of(run(&record, &Document::default(), &mut section, &mut state));
    let caption = picture.caption.expect("picture caption");
    assert_eq!(caption.side, "BOTTOM");
    assert_eq!(caption.width, 8504.0);
    assert_eq!(caption.gap, 850.0);
    assert_eq!(
        caption.paragraphs[0].runs[0].items,
        vec![RunItem::Text("picture caption".to_string())]
    );
}

#[test]
fn hwp5_shape_caption_is_not_textbox() {
    let component = record(
        76,
        2,
        component(1, identity_matrix(), (500, 300), (500, 300), &[], &[]),
        vec![text_box_list(vec![text_paragraph(4, "textbox text")])],
    );
    let record = control(
        b"gso ",
        &common(0, 400, 300),
        vec![
            caption_header(2, 8504, 300, Vec::new()),
            text_paragraph(7, "shape caption"),
            component,
        ],
    );
    let mut section = Section::default();
    let mut state = ParseState::default();
    let shape = shape_of(run(&record, &Document::default(), &mut section, &mut state));
    let caption = shape.caption.expect("shape caption");
    assert_eq!(caption.side, "TOP");
    assert_eq!(caption.gap, 300.0);
    assert_eq!(
        caption.paragraphs[0].runs[0].items,
        vec![RunItem::Text("shape caption".to_string())]
    );
    assert_eq!(
        shape.paragraphs[0].runs[0].items,
        vec![RunItem::Text("textbox text".to_string())]
    );
}

#[test]
fn hwp5_shape_caption_without_textbox_stays_in_caption() {
    let component = record(
        76,
        2,
        component(1, identity_matrix(), (500, 300), (500, 300), &[], &[]),
        Vec::new(),
    );
    let record = control(
        b"gso ",
        &common(0, 400, 300),
        vec![
            caption_header(2, 8504, 300, Vec::new()),
            text_paragraph(7, "shape caption"),
            component,
        ],
    );
    let mut section = Section::default();
    let mut state = ParseState::default();
    let shape = shape_of(run(&record, &Document::default(), &mut section, &mut state));
    let caption = shape.caption.expect("shape caption");
    assert_eq!(
        caption.paragraphs[0].runs[0].items,
        vec![RunItem::Text("shape caption".to_string())]
    );
    assert!(
        shape.paragraphs.is_empty(),
        "caption leaked into textbox: {:?}",
        shape.paragraphs
    );
}

#[test]
fn component_matrix_line_fill() {
    let mut tail = Vec::new();
    tail.extend_from_slice(&u32_bytes(0x0000FF));
    tail.extend_from_slice(&i32_bytes(1_440));
    tail.extend_from_slice(&u32_bytes(2));
    tail.push(0);
    tail.extend_from_slice(&u32_bytes(1));
    tail.extend_from_slice(&u32_bytes(0x0000FF));
    tail.extend_from_slice(&u32_bytes(0));
    tail.extend_from_slice(&i32_bytes(0));
    tail.extend_from_slice(&u32_bytes(0));
    tail.push(51);
    let component_data = component(
        7,
        translation_matrix(100.0, 200.0),
        (100, 50),
        (100, 50),
        &[([2.0, 0.0, 0.0, 0.0, 3.0, 0.0], identity_matrix())],
        &tail,
    );
    let mut line = Vec::new();
    for value in [10, 20, 0, 0] {
        line.extend_from_slice(&i32_bytes(value));
    }
    let component = record(76, 2, component_data, vec![record(78, 3, line, Vec::new())]);
    let record = gso(common(0, 10_000, 20_000), component);

    let mut section = Section::default();
    let mut state = ParseState::default();
    let shape = shape_of(run(&record, &Document::default(), &mut section, &mut state));
    assert_eq!(shape.kind, "line");
    assert_eq!(shape.line_type, "DASH");
    assert_eq!(shape.line_color, 0xFF0000);
    assert!((shape.line_width - 5.08).abs() < 1e-9);
    assert_eq!(shape.fill_color, Some(0xFF0000));
    assert!((shape.fill_alpha - 0.8).abs() < 1e-9);
    assert_eq!(shape.points, vec![[120.0, 260.0], [100.0, 200.0]]);
}

#[test]
fn group_children_frames() {
    let shape_child = record(
        76,
        4,
        component(
            2,
            translation_matrix(10.0, 20.0),
            (100, 50),
            (100, 50),
            &[],
            &[],
        ),
        Vec::new(),
    );
    let picture_record = record(85, 5, picture_data(1, (200, 100), &[]), Vec::new());
    let picture_child = record(
        76,
        4,
        component(
            3,
            translation_matrix(30.0, 40.0),
            (80, 60),
            (80, 60),
            &[],
            &[],
        ),
        vec![picture_record],
    );
    let nested = record(
        76,
        3,
        component(4, identity_matrix(), (1, 1), (1, 1), &[], &[]),
        vec![shape_child],
    );
    let container = record(
        76,
        2,
        component(1, identity_matrix(), (500, 400), (500, 400), &[], &[]),
        vec![nested, picture_child],
    );
    let record = gso(common(0, 500, 400), container);

    let mut section = Section::default();
    let mut state = ParseState::default().with_max_bin(1);
    let shape = shape_of(run(&record, &Document::default(), &mut section, &mut state));
    assert_eq!(shape.kind, "container");
    assert_eq!(shape.line_type, "NONE");
    assert_eq!(shape.width, 500.0);
    assert_eq!(shape.height, 400.0);
    assert_eq!(shape.children.len(), 2);
    assert_eq!(shape.children[0].frame, [[10.0, 20.0], [100.0, 50.0]]);
    assert_eq!(shape.children[1].frame, [[30.0, 40.0], [80.0, 60.0]]);
    assert!(matches!(
        shape.children[0].item.as_ref(),
        RunItem::Shape(child) if child.width == 100.0 && child.height == 50.0
    ));
    assert!(matches!(
        shape.children[1].item.as_ref(),
        RunItem::Picture(picture) if picture.binary_id == "bin1" && picture.width == 80.0
    ));
}

fn list_header(margins: [u16; 4], paragraphs: Vec<Record>) -> Record {
    let mut data = Vec::new();
    data.extend_from_slice(&i16_bytes(paragraphs.len() as i16));
    data.extend_from_slice(&u16_bytes(0));
    data.extend_from_slice(&u32_bytes(0));
    for margin in margins {
        data.extend_from_slice(&u16_bytes(margin));
    }
    record(72, 3, data, paragraphs)
}

fn shape_with_text_box(margins: [u16; 4]) -> Record {
    let header = list_header(margins, vec![paragraph(7)]);
    let component = record(
        76,
        2,
        component(9, identity_matrix(), (100, 100), (100, 100), &[], &[]),
        vec![header],
    );
    gso(common(0, 100, 100), component)
}

#[test]
fn textbox_margins_limit() {
    let mut section = Section::default();
    let mut state = ParseState::default();
    let shape = shape_of(run(
        &shape_with_text_box([10, 20, 30, 40]),
        &Document::default(),
        &mut section,
        &mut state,
    ));
    assert_eq!(shape.text_margins.left, 10.0);
    assert_eq!(shape.text_margins.right, 20.0);
    assert_eq!(shape.text_margins.top, 30.0);
    assert_eq!(shape.text_margins.bottom, 40.0);
    assert_eq!(shape.paragraphs.len(), 1);
    assert_eq!(shape.paragraphs[0].style_id, 7);

    let mut section = Section::default();
    let mut state = ParseState::default();
    let shape = shape_of(run(
        &shape_with_text_box([20_000, 10, 10, 10]),
        &Document::default(),
        &mut section,
        &mut state,
    ));
    assert_eq!(shape.text_margins.left, 0.0);
    assert_eq!(shape.text_margins.right, 0.0);
    assert_eq!(shape.text_margins.top, 0.0);
    assert_eq!(shape.text_margins.bottom, 0.0);
}

fn equation_data(script: &str, size: u32, font: &str) -> Vec<u8> {
    let units = script.encode_utf16().count() as u16;
    let mut data = Vec::new();
    data.extend_from_slice(&u32_bytes(0));
    data.extend_from_slice(&u16_bytes(units));
    data.extend_from_slice(&utf16_bytes(script));
    data.extend_from_slice(&u32_bytes(size));
    data.extend_from_slice(&u32_bytes(0x0000FF));
    data.extend_from_slice(&i16_bytes(85));
    data.extend_from_slice(&u16_bytes(0));
    let font_length = font.encode_utf16().count() as u16;
    data.extend_from_slice(&u16_bytes(font_length));
    data.extend_from_slice(&utf16_bytes(font));
    data
}

fn eqed(data: Vec<u8>) -> Record {
    control(
        b"eqed",
        &common(0, 3_000, 2_000),
        vec![record(88, 2, data, Vec::new())],
    )
}

#[test]
fn eqedit_fields_and_size_clamp() {
    let mut section = Section::default();
    let mut state = ParseState::default();
    let shape = shape_of(run(
        &eqed(equation_data("x^2+y^2", 2_000, "HYhwpEQ")),
        &Document::default(),
        &mut section,
        &mut state,
    ));
    assert_eq!(shape.kind, "equation");
    assert_eq!(shape.line_type, "NONE");
    assert_eq!(shape.width, 3_000.0);
    assert_eq!(shape.height, 2_000.0);
    let equation = shape.equation.expect("equation");
    assert_eq!(equation.script, "x^2+y^2");
    assert_eq!(equation.size, 2_000.0);
    assert_eq!(equation.color, 0xFF0000);
    assert_eq!(equation.baseline, 85.0);
    assert_eq!(equation.font, "HYhwpEQ");

    for size in [0, 100_000] {
        let mut section = Section::default();
        let mut state = ParseState::default();
        let shape = shape_of(run(
            &eqed(equation_data("a", size, "")),
            &Document::default(),
            &mut section,
            &mut state,
        ));
        assert_eq!(shape.equation.expect("equation").size, 1_000.0);
    }

    let mut truncated = Vec::new();
    truncated.extend_from_slice(&u32_bytes(0));
    truncated.extend_from_slice(&u16_bytes(5));
    truncated.extend_from_slice(&[b'a', 0]);
    let mut section = Section::default();
    let mut state = ParseState::default();
    assert_eq!(
        run(
            &eqed(truncated),
            &Document::default(),
            &mut section,
            &mut state,
        ),
        None
    );
}
