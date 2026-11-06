use hwp_core::*;
use serde_json::{json, to_value, Value};
use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

fn minimal_paragraph() -> Paragraph {
    Paragraph {
        line_segs: vec![LineSeg::default()],
        ..Paragraph::default()
    }
}

fn item_samples() -> Vec<RunItem> {
    let child = ShapeChild {
        item: Box::new(RunItem::Text("child".to_string())),
        frame: [[0.0, 0.0], [10.0, 10.0]],
        transform: Some([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]),
    };
    let caption = Caption {
        paragraphs: vec![minimal_paragraph()],
        ..Caption::default()
    };
    let shape = Shape {
        paragraphs: vec![minimal_paragraph()],
        text_margins: Margins::default(),
        points: vec![[0.0, 0.0], [1.0, 1.0]],
        arrow_start: Some(Arrow {
            style: "SPEAR".into(),
            size: "MEDIUM_MEDIUM".into(),
            fill: false,
        }),
        arrow_end: Some(Arrow {
            style: "ARROW".into(),
            size: "MEDIUM_MEDIUM".into(),
            fill: true,
        }),
        gradient: Some(Gradient {
            kind: "LINEAR".into(),
            angle: 45.0,
            center_x: 50.0,
            center_y: 50.0,
            step: 10,
            step_center: 50,
            colors: vec![0, 0xFFFFFF],
            positions: vec![0.0, 100.0],
        }),
        image_fill: Some(ImageFill {
            mode: "TOTAL".into(),
            binary_id: "bin1".into(),
            bright: 0,
            contrast: 0,
            effect: "REAL_PIC".into(),
        }),
        segment_types: vec!["CURVE".into()],
        arc: Some(Arc {
            kind: "NORMAL".into(),
            center: [0.0, 0.0],
            axis1: [1.0, 0.0],
            axis2: [0.0, 1.0],
        }),
        equation: Some(Equation::default()),
        children: vec![child],
        caption: Some(caption.clone()),
        ..Shape::default()
    };
    let table = Table {
        cells: vec![Cell {
            paragraphs: vec![minimal_paragraph()],
            ..Cell::default()
        }],
        caption: Some(caption.clone()),
        ..Table::default()
    };
    let picture = Picture {
        clip: Some([[0.0, 0.0], [100.0, 100.0]]),
        caption: Some(caption),
        ..Picture::default()
    };
    let note = Note {
        paragraphs: vec![minimal_paragraph()],
        ..Note::default()
    };
    vec![
        RunItem::Text("text".to_string()),
        RunItem::Table(table),
        RunItem::Picture(picture),
        RunItem::Shape(shape),
        RunItem::Tab,
        RunItem::SizedTab(100.0),
        RunItem::FormattedTab(Tab {
            width: Some(100.0),
            ..Tab::default()
        }),
        RunItem::LineBreak,
        RunItem::AutoNum("PAGE".to_string()),
        RunItem::Control("secd".to_string()),
        RunItem::Note(note),
        RunItem::Skip(1),
        RunItem::Header {
            paragraphs: vec![minimal_paragraph()],
            applies: "BOTH".to_string(),
        },
        RunItem::Footer {
            paragraphs: vec![minimal_paragraph()],
            applies: "EVEN".to_string(),
        },
        RunItem::PageHiding {
            header: true,
            footer: false,
            page_num: true,
        },
        RunItem::NewNum(1),
        RunItem::PageNum {
            pos: "BOTTOM".to_string(),
            format: "DIGIT".to_string(),
            side: "CENTER".to_string(),
        },
    ]
}

fn sample_document() -> Document {
    let paragraph = Paragraph {
        range_tags: vec![RangeTag {
            start: 0,
            end: 1,
            tag: 0x0377_8899,
        }],
        markpens: vec![Markpen {
            start: 0,
            end: 1,
            color: 0x778899,
        }],
        columns: Some(ColumnDef {
            count: 2,
            same_width: false,
            widths: vec![12000.0, 18000.0],
            ..ColumnDef::default()
        }),
        runs: vec![Run {
            char_pr_id: 1,
            items: item_samples(),
            supscript: true,
        }],
        line_segs: vec![LineSeg::default()],
        memos: vec![Memo {
            shape_id: 1,
            paragraphs: vec![minimal_paragraph()],
        }],
        ..Paragraph::default()
    };
    let mut font_faces = std::collections::BTreeMap::new();
    for (index, language) in [
        "hangul", "latin", "hanja", "japanese", "other", "symbol", "user",
    ]
    .into_iter()
    .enumerate()
    {
        font_faces.insert(
            language.to_string(),
            vec![FontFace {
                id: index as i32 + 1,
                name: "face".to_string(),
                face_type: 1,
                alt_name: Some("alt".to_string()),
                alt_type: 2,
                default_name: Some("default".to_string()),
                embedded: true,
                binary_ref: Some("bin1".to_string()),
            }],
        );
    }
    Document {
        font_faces,
        char_prs: vec![CharPr {
            id: 1,
            shade_color: Some(0),
            ..CharPr::default()
        }],
        para_prs: vec![ParaPr::default()],
        tab_prs: vec![TabPr {
            stops: vec![TabStop::default()],
            ..TabPr::default()
        }],
        border_fills: vec![BorderFill {
            id: 1,
            fill_color: Some(0),
            gradient: Some(vec![(0, 0.0)]),
            ..BorderFill::default()
        }],
        numberings: vec![Numbering {
            id: 1,
            heads: vec![NumberingHead {
                char_pr_id: Some(1),
                ..NumberingHead::default()
            }],
            ..Numbering::default()
        }],
        bullets: vec![Bullet {
            id: 1,
            char_pr_id: Some(1),
            ..Bullet::default()
        }],
        styles: vec![Style {
            id: 1,
            para_pr_id: 2,
            char_pr_id: 3,
            eng_name: "Page Number".to_string(),
        }],
        sections: vec![Section {
            page_border_fills: vec![PageBorderFill::default()],
            paragraphs: vec![paragraph],
            header: vec![minimal_paragraph()],
            footer: vec![minimal_paragraph()],
            ..Section::default()
        }],
        memo_shapes: vec![MemoShape {
            id: 1,
            width: 15591.0,
            line_type: 1,
            line_width: 1,
            line_color: 0xB6D7AE,
            fill_color: 0xF0FFE9,
            active_color: 0xCFF1C7,
            memo_type: 0,
        }],
        wchar_positions: true,
        track_changes: false,
        track_markup_hidden: false,
        bins: vec![Blob {
            id: "bin1".to_string(),
            kind: BlobKind::Picture,
            data: vec![1, 2, 3].into(),
        }],
    }
}

fn collect_keys(value: &Value, keys: &mut BTreeSet<String>) {
    match value {
        Value::Object(map) => {
            for (key, value) in map {
                keys.insert(key.clone());
                collect_keys(value, keys);
            }
        }
        Value::Array(values) => {
            for value in values {
                collect_keys(value, keys);
            }
        }
        _ => {}
    }
}

fn schema_keys() -> BTreeSet<String> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("schema/v1.keys");
    let raw = fs::read_to_string(path).expect("read schema/v1.keys");
    let listed: Vec<String> = serde_json::from_str(&raw).expect("parse schema/v1.keys");
    listed.into_iter().collect()
}

#[test]
fn model_keys_match_schema_v1() {
    let value = to_value(sample_document()).expect("serialize model");
    let mut actual = BTreeSet::new();
    collect_keys(&value, &mut actual);
    let expected = schema_keys();
    let missing: Vec<_> = expected.difference(&actual).cloned().collect();
    let unexpected: Vec<_> = actual.difference(&expected).cloned().collect();
    assert!(
        missing.is_empty() && unexpected.is_empty(),
        "schema/v1.keys mismatch: missing {missing:?}, unexpected {unexpected:?}"
    );
}

#[test]
fn defaults_match_swift_model() {
    let char_pr = CharPr::default();
    assert_eq!(char_pr.height, 1000.0);
    assert_eq!(char_pr.underline, "NONE");
    assert_eq!(char_pr.shade_color, None);

    let para_pr = ParaPr::default();
    assert_eq!(para_pr.align, "JUSTIFY");
    assert_eq!(para_pr.line_spacing, 160.0);
    assert_eq!(para_pr.line_spacing_type, "PERCENT");
    assert_eq!(para_pr.vert_align, "BASELINE");
    assert_eq!(para_pr.break_latin_word, "KEEP_WORD");
    assert_eq!(para_pr.break_non_latin_word, "BREAK_WORD");

    let page = Page::default();
    assert_eq!(page.width, 59528.0);
    assert_eq!(page.height, 84186.0);
    assert_eq!(page.margin_left, 8504.0);
    assert_eq!(page.margin_right, 8504.0);
    assert_eq!(page.margin_top, 5668.0);
    assert_eq!(page.margin_bottom, 4252.0);
    assert_eq!(page.header, 4252.0);
    assert_eq!(page.footer, 4252.0);
    assert_eq!(page.gutter, 0.0);
    assert!(!page.landscape);

    let shape = Shape::default();
    assert_eq!(shape.kind, "rect");
    assert_eq!(shape.line_width, 0.12);
    assert_eq!(shape.line_type, "SOLID");
    assert_eq!(shape.fill_alpha, 1.0);

    let equation = Equation::default();
    assert_eq!(equation.size, 1000.0);
    assert_eq!(equation.baseline, 85.0);
    assert_eq!(equation.script, "");
    assert_eq!(equation.font, "");

    let note_shape = NoteShape::default();
    assert_eq!(note_shape.format, 0);
    assert_eq!(note_shape.line_length, -1.0);
    assert_eq!(note_shape.above_line, 850.0);
    assert_eq!(note_shape.below_line, 567.0);
    assert_eq!(note_shape.between_notes, 283.0);
    assert_eq!(note_shape.line_type, 1);
    assert_eq!(note_shape.line_width, 0.12);
    assert_eq!(note_shape.line_color, 0);

    assert_eq!(Tab::default().kind, "LEFT");
    assert_eq!(Tab::default().leader, "NONE");
    assert_eq!(TabStop::default().kind, "LEFT");
    assert_eq!(Border::default().width, 0.1);
    assert_eq!(BorderFill::default().slash, "NONE");
    assert_eq!(BorderFill::default().back_slash, "NONE");

    let pos = Pos::default();
    assert!(pos.treat_as_char);
    assert_eq!(pos.vert_rel_to, "PARA");
    assert_eq!(pos.horz_rel_to, "COLUMN");
    assert_eq!(pos.width_rel_to, "ABSOLUTE");
    assert_eq!(pos.height_rel_to, "ABSOLUTE");
    assert_eq!(pos.text_wrap, "SQUARE");

    assert_eq!(Table::default().page_break_mode, "CELL");
    assert!(Table::default().caption.is_none());
    assert!(Picture::default().caption.is_none());
    assert!(Shape::default().caption.is_none());
    let caption = Caption::default();
    assert_eq!(caption.side, "LEFT");
    assert_eq!(caption.gap, 0.0);
    assert_eq!(caption.width, 0.0);
    assert!(!caption.full_size);
    assert!(caption.paragraphs.is_empty());
    let memo = Memo::default();
    assert_eq!(memo.shape_id, 0);
    assert!(memo.paragraphs.is_empty());
    let memo_shape = MemoShape::default();
    assert_eq!(memo_shape.id, 0);
    assert_eq!(memo_shape.width, 0.0);
    assert_eq!(memo_shape.line_type, 0);
    assert_eq!(memo_shape.line_width, 0);
    assert_eq!(memo_shape.line_color, 0);
    assert_eq!(memo_shape.fill_color, 0);
    assert_eq!(memo_shape.active_color, 0);
    assert_eq!(memo_shape.memo_type, 0);
    assert_eq!(Cell::default().col_span, 1);
    assert_eq!(Cell::default().row_span, 1);
    assert_eq!(Cell::default().vert_align, "TOP");

    let section = Section::default();
    assert_eq!(section.columns, 1);
    assert_eq!(section.column_gap, 1134.0);
    assert_eq!(section.tab_stop, 4000.0);
    assert!(section.page_border_fills.is_empty());
    let fill = PageBorderFill::default();
    assert_eq!(fill.kind, "BOTH");
    assert_eq!(fill.border_fill_id, 0);
    assert_eq!(fill.text_border, "PAPER");
    assert!(!fill.header_inside);
    assert!(!fill.footer_inside);
    assert_eq!(fill.fill_area, "PAPER");
    assert_eq!(fill.offsets, Margins::default());

    let paragraph = Paragraph::default();
    assert_eq!(paragraph.para_pr_id, 0);
    assert_eq!(paragraph.style_id, 0);

    assert_eq!(NumberingHead::default().level, 1);
    assert_eq!(NumberingHead::default().format, "DIGIT");
    assert_eq!(NumberingHead::default().text, "^1.");
    assert_eq!(NumberingHead::default().start, 1);
    assert_eq!(Bullet::default().char, "•");
    assert!(ParaHead::default().text_offset_percent);

    let face = FontFace::new("face");
    assert_eq!(face.id, 0);
    assert_eq!(face.face_type, 0);
    assert_eq!(face.alt_name, None);
    assert_eq!(face.alt_type, 0);
    assert_eq!(face.default_name, None);
    assert!(!face.embedded);
    assert_eq!(face.binary_ref, None);

    let document = Document::default();
    assert!(document.font_faces.is_empty());
    assert!(document.char_prs.is_empty());
    assert!(document.sections.is_empty());
    assert!(document.memo_shapes.is_empty());
    assert!(!document.wchar_positions);

    let section = to_value(Section::default()).expect("serialize section");
    assert!(section.get("columnsDefined").is_none());
    assert!(section.get("noteCount").is_none());
    let line_seg = to_value(LineSeg::default()).expect("serialize line segment");
    assert!(line_seg.get("isPageFirstLine").is_none());
    assert!(line_seg.get("isColumnFirstLine").is_none());
    let page = to_value(Page::default()).expect("serialize page");
    assert!(page.get("bodyLeft").is_none());
    assert!(page.get("bodyTop").is_none());
    assert!(page.get("bodyWidth").is_none());
    assert!(page.get("bodyHeight").is_none());
}

#[test]
fn non_finite_floats_are_replaced() {
    let mut document = sample_document();
    document.char_prs[0].height = f64::NAN;
    document.char_prs[0]
        .ratio
        .insert("hangul".to_string(), f64::INFINITY);
    document.para_prs[0].line_spacing = f64::INFINITY;
    document.para_prs[0].margin_left = f64::NEG_INFINITY;
    document.sections[0].page.width = f64::NEG_INFINITY;
    document.sections[0].page.margin_top = f64::NAN;
    document.sections[0].column_gap = f64::NAN;
    document.sections[0].page_border_fills[0].offsets.left = f64::INFINITY;
    let columns = document.sections[0].paragraphs[0].columns.as_mut().unwrap();
    columns.gap = f64::NAN;
    columns.line_width = f64::INFINITY;
    columns.widths[0] = f64::NEG_INFINITY;
    document.sections[0].footnote_shape.between_notes = f64::NAN;

    let items = &mut document.sections[0].paragraphs[0].runs[0].items;
    if let Some(RunItem::Picture(picture)) = items
        .iter_mut()
        .find(|item| matches!(item, RunItem::Picture(_)))
    {
        picture.clip = Some([[f64::NAN, 0.0], [1.0, 1.0]]);
    }
    if let Some(RunItem::FormattedTab(tab)) = items
        .iter_mut()
        .find(|item| matches!(item, RunItem::FormattedTab(_)))
    {
        tab.width = Some(f64::NAN);
    }
    if let Some(RunItem::Shape(shape)) = items
        .iter_mut()
        .find(|item| matches!(item, RunItem::Shape(_)))
    {
        shape.line_width = f64::NAN;
        shape.fill_alpha = f64::INFINITY;
        shape.points[0][0] = f64::NAN;
        let gradient = shape.gradient.as_mut().unwrap();
        gradient.angle = f64::INFINITY;
        gradient.center_x = f64::NEG_INFINITY;
        gradient.center_y = f64::NAN;
        gradient.positions[0] = f64::NAN;
        let arc = shape.arc.as_mut().unwrap();
        arc.center[0] = f64::NAN;
        arc.axis1[1] = f64::INFINITY;
        arc.axis2[0] = f64::NEG_INFINITY;
        if let Some(equation) = shape.equation.as_mut() {
            equation.size = f64::NAN;
        }
        shape.children[0].frame[0][0] = f64::NAN;
        shape.children[0].transform = Some([f64::NAN, 0.0, 0.0, 1.0, 0.0, 0.0]);
    }

    document.sanitize_non_finite();

    assert_eq!(document.char_prs[0].height, 1000.0);
    assert_eq!(document.char_prs[0].ratio["hangul"], 100.0);
    assert_eq!(document.para_prs[0].line_spacing, 160.0);
    assert_eq!(document.para_prs[0].margin_left, 0.0);
    assert_eq!(document.sections[0].page.width, 59528.0);
    assert_eq!(document.sections[0].page.margin_top, 5668.0);
    assert_eq!(document.sections[0].column_gap, 1134.0);
    assert_eq!(document.sections[0].page_border_fills[0].offsets.left, 0.0);
    let columns = document.sections[0].paragraphs[0].columns.as_ref().unwrap();
    assert_eq!(columns.gap, 1134.0);
    assert_eq!(columns.line_width, 0.1);
    assert_eq!(columns.widths, [0.0, 18000.0]);
    assert_eq!(document.sections[0].footnote_shape.between_notes, 283.0);

    let items = &document.sections[0].paragraphs[0].runs[0].items;
    for item in items {
        match item {
            RunItem::Picture(picture) => assert_eq!(picture.clip, None),
            RunItem::FormattedTab(tab) => assert_eq!(tab.width, None),
            RunItem::Shape(shape) => {
                assert_eq!(shape.line_width, 0.12);
                assert_eq!(shape.fill_alpha, 1.0);
                assert_eq!(shape.points[0][0], 0.0);
                let gradient = shape.gradient.as_ref().unwrap();
                assert_eq!(
                    (gradient.angle, gradient.center_x, gradient.center_y),
                    (0.0, 0.0, 0.0)
                );
                assert_eq!(gradient.positions, [0.0, 100.0]);
                let arc = shape.arc.as_ref().unwrap();
                assert_eq!((arc.center[0], arc.axis1[1], arc.axis2[0]), (0.0, 0.0, 0.0));
                assert_eq!(
                    shape.equation.as_ref().map(|value| value.size),
                    Some(1000.0)
                );
                assert_eq!(shape.children[0].frame[0][0], 0.0);
                assert_eq!(
                    shape.children[0].transform,
                    Some([1.0, 0.0, 0.0, 1.0, 0.0, 0.0])
                );
            }
            _ => {}
        }
    }
    assert!(serde_json::to_string(&document).is_ok());
}

#[test]
fn run_item_tagging_shapes() {
    let simple = [
        (RunItem::Text("x".to_string()), "text"),
        (RunItem::Table(Table::default()), "table"),
        (RunItem::Picture(Picture::default()), "picture"),
        (RunItem::Shape(Shape::default()), "shape"),
        (RunItem::FormattedTab(Tab::default()), "formattedTab"),
        (RunItem::Note(Note::default()), "note"),
        (RunItem::Compose(Compose::default()), "compose"),
    ];
    for (item, tag) in simple {
        let value = to_value(item).expect("serialize item");
        assert_eq!(value["t"], tag);
        assert!(value.get("v").is_some(), "{tag} must carry a value");
    }
    let scalar = [
        (RunItem::SizedTab(10.0), "sizedTab"),
        (RunItem::Skip(2), "skip"),
        (RunItem::NewNum(3), "newNum"),
    ];
    for (item, tag) in scalar {
        let value = to_value(item).expect("serialize item");
        assert_eq!(value["t"], tag);
        assert!(value["v"].is_number(), "{tag} must carry a number");
    }
    let text = [
        (RunItem::AutoNum("PAGE".to_string()), "autoNum"),
        (RunItem::Control("secd".to_string()), "control"),
    ];
    for (item, tag) in text {
        let value = to_value(item).expect("serialize item");
        assert_eq!(value["t"], tag);
        assert!(value["v"].is_string(), "{tag} must carry a string");
    }
    for (item, tag) in [(RunItem::Tab, "tab"), (RunItem::LineBreak, "lineBreak")] {
        let value = to_value(item).expect("serialize item");
        assert_eq!(value, json!({ "t": tag }));
    }
    let header = to_value(RunItem::Header {
        paragraphs: vec![],
        applies: "BOTH".to_string(),
    })
    .expect("serialize item");
    assert_eq!(
        header,
        json!({"t": "header", "v": {"paragraphs": [], "applies": "BOTH"}})
    );
    let footer = to_value(RunItem::Footer {
        paragraphs: vec![],
        applies: "ODD".to_string(),
    })
    .expect("serialize item");
    assert_eq!(
        footer,
        json!({"t": "footer", "v": {"paragraphs": [], "applies": "ODD"}})
    );
    let hiding = to_value(RunItem::PageHiding {
        header: true,
        footer: false,
        page_num: true,
    })
    .expect("serialize item");
    assert_eq!(
        hiding,
        json!({"t": "pageHiding", "v": {"header": true, "footer": false, "pageNum": true}})
    );
    let page_num = to_value(RunItem::PageNum {
        pos: "BOTTOM".to_string(),
        format: "DIGIT".to_string(),
        side: "CENTER".to_string(),
    })
    .expect("serialize item");
    assert_eq!(
        page_num,
        json!({"t": "pageNum", "v": {"pos": "BOTTOM", "format": "DIGIT", "side": "CENTER"}})
    );
}

#[test]
fn shape_extensions_are_omitted_by_default() {
    let shape = to_value(Shape::default()).unwrap();
    for key in [
        "arrowStart",
        "arrowEnd",
        "gradient",
        "imageFill",
        "segmentTypes",
        "arc",
    ] {
        assert!(shape.get(key).is_none(), "{key}");
    }
    let gradient = Gradient {
        kind: "LINEAR".into(),
        angle: 0.0,
        center_x: 0.0,
        center_y: 0.0,
        step: 0,
        step_center: 50,
        colors: vec![],
        positions: vec![],
    };
    let gradient = to_value(gradient).unwrap();
    assert_eq!(gradient.get("colors"), Some(&json!([])));
    assert_eq!(gradient.get("positions"), Some(&json!([])));
}
