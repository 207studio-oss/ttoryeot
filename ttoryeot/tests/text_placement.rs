//! 한 줄 안 글자 자리: 한/글 기준 PDF에서 잰 규칙.

use hwp_core::{
    Blob, BlobKind, Bullet, CharPr, Document, FontFace, ImageFill, LineSeg, Margins, ParaHead,
    ParaPr, Paragraph, Pos, Run, RunItem, Section, Shape, ShapeChild, Tab,
};
use std::collections::BTreeMap;
use ttoryeot::fonts::FontBook;
use ttoryeot::scene::{Item, Page, PathOp};

/// 10pt Liberation Sans, 자간 `spacing`%인 글자 모양 하나로 쓴 한 줄.
fn one_line(spacing: f64, items: Vec<RunItem>) -> Document {
    Document {
        font_faces: BTreeMap::from([("latin".to_string(), vec![FontFace::new("Liberation Sans")])]),
        char_prs: vec![CharPr {
            font_ref: BTreeMap::from([("latin".to_string(), 0)]),
            spacing: BTreeMap::from([("latin".to_string(), spacing)]),
            ..CharPr::default()
        }],
        sections: vec![Section {
            paragraphs: vec![Paragraph {
                runs: vec![Run {
                    char_pr_id: 0,
                    items,
                    supscript: false,
                }],
                ..Paragraph::default()
            }],
            ..Section::default()
        }],
        ..Document::default()
    }
}

fn first_page(doc: &Document) -> Page {
    let font = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../assets/fonts/LiberationSans-Regular.ttf"
    );
    let mut book = FontBook::scan(&[]);
    book.add_bytes(font.into(), std::fs::read(font).unwrap());
    ttoryeot::render(doc, &mut book).swap_remove(0)
}

/// 첫 쪽 글자들의 (글자, 쪽 위 x).
fn placed_chars(doc: &Document) -> Vec<(char, f32)> {
    first_page(doc)
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Glyphs(run) => Some(run),
            _ => None,
        })
        .flat_map(|run| {
            run.ranges
                .iter()
                .zip(&run.glyphs)
                .map(|(range, (_, offset))| {
                    (
                        run.text[range.clone()].chars().next().unwrap(),
                        run.origin.0 + offset,
                    )
                })
        })
        .collect()
}

#[test]
fn dot_leader_fills_quarter_em_slots_leaving_first_and_last_two_empty() {
    let tab = Tab {
        width: Some(3600.0),
        kind: "LEFT".to_string(),
        leader: "DOT".to_string(),
    };
    let chars = placed_chars(&one_line(
        0.0,
        vec![
            RunItem::Text("A".to_string()),
            RunItem::FormattedTab(tab),
            RunItem::Text("B".to_string()),
        ],
    ));
    let dots: Vec<f32> = chars
        .iter()
        .filter(|(ch, _)| *ch == '\u{B7}')
        .map(|(_, x)| *x)
        .collect();
    let end = chars.iter().find(|(ch, _)| *ch == 'B').unwrap().1;
    // 36pt를 10pt 글자의 1/4em(2.5pt) 이상인 칸으로 나누면 14칸이고, 점은 둘째 칸부터 끝에서 셋째 칸까지 12개.
    let step = 36.0 / 14.0;
    assert_eq!(dots.len(), 12, "dots {dots:?}");
    for (index, x) in dots.iter().enumerate() {
        let expected = end - step * (13 - index) as f32;
        assert!(
            (x - expected).abs() < 0.01,
            "dot {index} at {x}, expected {expected}"
        );
    }
}

#[test]
fn fixed_space_is_a_quarter_em_and_no_break_space_ignores_letter_spacing() {
    // 자간 +20%에서 고정폭 빈칸은 1/4em에 자간을 넣고, 묶음 빈칸은 1/2em 그대로다.
    let added = |space: char| {
        let gap = |text: String| {
            let chars = placed_chars(&one_line(20.0, vec![RunItem::Text(text)]));
            let x = |want: char| chars.iter().find(|(ch, _)| *ch == want).unwrap().1;
            x('B') - x('A')
        };
        gap(format!("A{space}{space}B")) - gap("AB".to_string())
    };
    let fixed = added(hwp_core::FIXED_SPACE);
    let no_break = added(hwp_core::NO_BREAK_SPACE);
    assert!((fixed - 2.0 * 2.5 * 1.2).abs() < 0.01, "fixed {fixed}");
    assert!((no_break - 2.0 * 5.0).abs() < 0.01, "no-break {no_break}");
}

#[test]
fn space_inside_superscript_keeps_the_full_size_width() {
    // 위 첨자(9.6pt)의 빈칸은 15pt 글자의 1/2em만큼 띄운다.
    let added = |text: &str| {
        let mut doc = one_line(0.0, vec![RunItem::Text(text.to_string())]);
        doc.char_prs[0].supscript = true;
        let chars = placed_chars(&doc);
        let x = |want: char| chars.iter().find(|(ch, _)| *ch == want).unwrap().1;
        x('B') - x('A')
    };
    let space = added("A  B") - added("AB");
    assert!((space - 2.0 * 5.0).abs() < 0.01, "space {space}");
}

#[test]
fn subscript_shrinks_to_064_and_drops_by_012_of_the_full_size() {
    // 15pt 글자의 아래 첨자는 9.6pt로 그리고 기준선을 1.8pt 내린다.
    let placed = |sub: bool| {
        let mut doc = one_line(0.0, vec![RunItem::Text("AB".to_string())]);
        doc.char_prs[0].sub_script = sub;
        let page = first_page(&doc);
        let run = page
            .items
            .iter()
            .find_map(|item| match item {
                Item::Glyphs(run) => Some(run),
                _ => None,
            })
            .unwrap();
        (run.glyphs[1].1 - run.glyphs[0].1, run.origin.1)
    };
    let ((plain_gap, plain_y), (sub_gap, sub_y)) = (placed(false), placed(true));
    assert!(
        (sub_gap - plain_gap * 0.64).abs() < 0.01,
        "gap {plain_gap} → {sub_gap}"
    );
    assert!(
        (sub_y - plain_y - 1.2).abs() < 0.01,
        "y {plain_y} → {sub_y}"
    );
}

#[test]
fn letter_spacing_scales_each_glyph_width_not_the_font_size() {
    let gap = |spacing: f64| {
        let chars = placed_chars(&one_line(spacing, vec![RunItem::Text("11".to_string())]));
        chars[1].1 - chars[0].1
    };
    let (plain, tight) = (gap(0.0), gap(-10.0));
    // 숫자 폭이 1em보다 좁으니 글자 크기(10pt)의 10%를 빼면 1pt, 숫자 폭의 10%를 빼면 그보다 적게 준다.
    assert!(
        (tight - plain * 0.9).abs() < 0.01,
        "plain {plain}, tight {tight}"
    );
}

#[test]
fn strikeout_uses_its_own_color_and_splits_when_that_color_changes() {
    let mut doc = one_line(0.0, Vec::new());
    doc.char_prs[0].text_color = 0x112233;
    doc.char_prs[0].strikeout = true;
    doc.char_prs[0].strikeout_color = 0xAABBCC;
    let mut black_strikeout = doc.char_prs[0].clone();
    black_strikeout.id = 1;
    black_strikeout.strikeout_color = 0;
    doc.char_prs.push(black_strikeout);
    doc.sections[0].paragraphs[0].runs = vec![
        Run {
            char_pr_id: 0,
            items: vec![RunItem::Text("A".to_string())],
            supscript: false,
        },
        Run {
            char_pr_id: 1,
            items: vec![RunItem::Text("B".to_string())],
            supscript: false,
        },
    ];

    let page = first_page(&doc);
    let text: Vec<_> = page
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Glyphs(run) => Some((run.text.as_str(), run.color)),
            _ => None,
        })
        .collect();
    let colors: Vec<_> = page
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Fill { color, .. } => Some(*color),
            _ => None,
        })
        .collect();
    assert_eq!(text, [("AB", 0x112233)]);
    assert_eq!(colors, [0xAABBCC, 0]);
}

#[test]
fn outline_head_is_drawn_with_the_sections_numbering() {
    let mut doc = one_line(0.0, vec![RunItem::Text("A".to_string())]);
    doc.para_prs = vec![ParaPr {
        heading_type: "OUTLINE".to_string(),
        heading_id_ref: 0,
        ..ParaPr::default()
    }];
    doc.numberings = vec![hwp_core::Numbering {
        id: 1,
        heads: vec![hwp_core::NumberingHead::default()],
        ..hwp_core::Numbering::default()
    }];
    doc.sections[0].outline_numbering_id = Some(1);
    hwp_core::num::resolve_heads(&mut doc);
    let text: String = placed_chars(&doc).iter().map(|(ch, _)| ch).collect();
    assert_eq!(text, "1.A");
}

#[test]
fn inline_object_sits_inside_its_outer_margins() {
    let placed = |margin: f64| {
        let shape = Shape {
            width: 2000.0,
            height: 2000.0,
            out_margins: Margins {
                left: margin,
                right: margin,
                top: margin,
                bottom: margin,
            },
            fill_color: Some(0xFF0000),
            ..Shape::default()
        };
        let doc = one_line(
            0.0,
            vec![
                RunItem::Text("A".to_string()),
                RunItem::Shape(shape),
                RunItem::Text("B".to_string()),
            ],
        );
        let page = first_page(&doc);
        let corner = page
            .items
            .iter()
            .find_map(|item| match item {
                Item::Fill { path, .. } => match path[0] {
                    PathOp::Move(x, y) => Some((x, y)),
                    _ => None,
                },
                _ => None,
            })
            .unwrap();
        let after = placed_chars(&doc)
            .iter()
            .find(|(ch, _)| *ch == 'B')
            .unwrap()
            .1;
        (corner, after)
    };
    let ((x0, y0), after0) = placed(0.0);
    let ((x1, y1), after1) = placed(283.0);
    // 바깥 여백 283(1mm = 2.83pt): 개체는 왼쪽·위 여백만큼 안쪽에, 다음 글자는 좌우 여백만큼 뒤에 온다.
    let margin = 2.83;
    assert!((x1 - x0 - margin).abs() < 0.01, "left {x0} -> {x1}");
    assert!((y1 - y0 - margin).abs() < 0.01, "top {y0} -> {y1}");
    assert!(
        (after1 - after0 - 2.0 * margin).abs() < 0.01,
        "next glyph {after0} -> {after1}"
    );
}

#[test]
fn stored_lines_get_the_paragraph_indent_when_drawn() {
    // 줄 배치에 저장된 시작 자리는 왼쪽 여백만 담고, 첫 줄 들여쓰기·다음 줄 내어쓰기는 그릴 때 더한다.
    let line_starts = |indent: f64| {
        let line = |textpos: i32, vertpos: f64| LineSeg {
            textpos,
            vertpos,
            vertsize: 1000.0,
            textheight: 1000.0,
            baseline: 850.0,
            spacing: 600.0,
            horzpos: 0.0,
            horzsize: 40000.0,
            flags: 0,
        };
        let mut doc = one_line(0.0, vec![RunItem::Text("AAAAABBBBB".to_string())]);
        doc.para_prs = vec![ParaPr {
            align: "LEFT".to_string(),
            indent,
            ..ParaPr::default()
        }];
        doc.sections[0].paragraphs[0].line_segs = vec![line(0, 0.0), line(5, 1600.0)];
        let chars = placed_chars(&doc);
        let x = |ch: char| chars.iter().find(|(c, _)| *c == ch).unwrap().1;
        (x('A'), x('B'))
    };
    let (first, rest) = line_starts(0.0);
    assert!((first - rest).abs() < 0.01, "no indent: {first} vs {rest}");
    let (first, rest) = line_starts(-1980.0);
    assert!(
        (rest - first - 19.8).abs() < 0.01,
        "hanging: {first} vs {rest}"
    );
    let (first, rest) = line_starts(1000.0);
    assert!(
        (first - rest - 10.0).abs() < 0.01,
        "first line: {first} vs {rest}"
    );
}

#[test]
fn container_child_frame_is_origin_and_size() {
    let rect = |frame: [[f64; 2]; 2]| ShapeChild {
        item: Box::new(RunItem::Shape(Shape {
            width: frame[1][0],
            height: frame[1][1],
            fill_color: Some(0xFF0000),
            ..Shape::default()
        })),
        frame,
        transform: None,
    };
    let container = Shape {
        kind: "container".to_string(),
        width: 5000.0,
        height: 5000.0,
        children: vec![
            rect([[0.0, 0.0], [1000.0, 1000.0]]),
            rect([[1000.0, 2000.0], [3000.0, 1500.0]]),
        ],
        ..Shape::default()
    };
    let page = first_page(&one_line(0.0, vec![RunItem::Shape(container)]));
    let boxes: Vec<(f32, f32, f32, f32)> = page
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Fill { path, .. } => Some(path),
            _ => None,
        })
        .map(|path| {
            let points = path.iter().filter_map(|op| match *op {
                PathOp::Move(x, y) | PathOp::Line(x, y) => Some((x, y)),
                _ => None,
            });
            points.fold(
                (f32::MAX, f32::MAX, f32::MIN, f32::MIN),
                |(x0, y0, x1, y1), (x, y)| (x0.min(x), y0.min(y), x1.max(x), y1.max(y)),
            )
        })
        .collect();
    assert_eq!(boxes.len(), 2, "fills {boxes:?}");
    let (ox, oy, _, _) = boxes[0];
    let (x0, y0, x1, y1) = boxes[1];
    // 틀 [[1000, 2000], [3000, 1500]]: 묶음 왼쪽 위에서 (10pt, 20pt) 떨어진 30pt × 15pt.
    for (got, want) in [
        (x0 - ox, 10.0),
        (y0 - oy, 20.0),
        (x1 - x0, 30.0),
        (y1 - y0, 15.0),
    ] {
        assert!((got - want).abs() < 0.01, "child box {:?}", boxes[1]);
    }
}

#[test]
fn paragraph_anchored_object_sits_at_paragraph_start_above_its_pushed_line() {
    let line = |vertpos: f64| LineSeg {
        textpos: 0,
        vertpos,
        vertsize: 1000.0,
        textheight: 1000.0,
        baseline: 850.0,
        spacing: 600.0,
        horzpos: 0.0,
        horzsize: 40000.0,
        flags: 0,
    };
    let block = Shape {
        width: 4000.0,
        height: 3000.0,
        pos: Pos {
            treat_as_char: false,
            text_wrap: "TOP_AND_BOTTOM".to_string(),
            ..Pos::default()
        },
        fill_color: Some(0xFF0000),
        ..Shape::default()
    };
    let mut doc = one_line(0.0, vec![RunItem::Text("A".to_string())]);
    let paragraphs = &mut doc.sections[0].paragraphs;
    paragraphs[0].line_segs = vec![line(0.0)];
    // 둘째 문단의 첫 줄은 앞 문단 끝(1600)에서 개체 높이(3000)만큼 밀려 4600에 저장돼 있다.
    paragraphs.push(Paragraph {
        runs: vec![Run {
            char_pr_id: 0,
            items: vec![RunItem::Shape(block), RunItem::Text("B".to_string())],
            supscript: false,
        }],
        line_segs: vec![line(4600.0)],
        ..Paragraph::default()
    });
    let page = first_page(&doc);
    let baseline = |ch: char| {
        page.items
            .iter()
            .find_map(|item| match item {
                Item::Glyphs(run) if run.text.starts_with(ch) => Some(run.origin.1),
                _ => None,
            })
            .unwrap()
    };
    let block_top = page
        .items
        .iter()
        .find_map(|item| match item {
            Item::Fill { path, .. } => path
                .iter()
                .filter_map(|op| match *op {
                    PathOp::Move(_, y) | PathOp::Line(_, y) => Some(y),
                    _ => None,
                })
                .reduce(f32::min),
            _ => None,
        })
        .unwrap();
    let first_top = baseline('A') - 8.5;
    assert!(
        (block_top - first_top - 16.0).abs() < 0.01,
        "block top {block_top}, first line top {first_top}"
    );
    assert!((baseline('B') - 8.5 - first_top - 46.0).abs() < 0.01);
}

#[test]
fn horizontal_paragraph_anchor_uses_paragraph_margins() {
    let left = |relative_to: &str, align: &str, margin_left: f64, margin_right: f64| {
        let shape = Shape {
            width: 2000.0,
            height: 2000.0,
            fill_color: Some(0xFF0000),
            pos: Pos {
                treat_as_char: false,
                horz_rel_to: relative_to.to_string(),
                horz_align: align.to_string(),
                ..Pos::default()
            },
            ..Shape::default()
        };
        let mut doc = one_line(0.0, vec![RunItem::Shape(shape)]);
        doc.para_prs = vec![ParaPr {
            margin_left,
            margin_right,
            ..ParaPr::default()
        }];
        first_page(&doc)
            .items
            .iter()
            .find_map(|item| match item {
                Item::Fill { path, color, .. } if *color == 0xFF0000 => match path[0] {
                    PathOp::Move(x, _) => Some(x),
                    _ => None,
                },
                _ => None,
            })
            .unwrap()
    };
    let with_margins = |relative_to, align| {
        left(relative_to, align, 1500.0, 500.0) - left(relative_to, align, 0.0, 0.0)
    };
    // 문단 기준은 왼쪽 15pt와 오른쪽 5pt 여백 사이에서 정렬한다.
    for (align, expected) in [("LEFT", 15.0), ("CENTER", 5.0), ("RIGHT", -5.0)] {
        let shift = with_margins("PARA", align);
        assert!((shift - expected).abs() < 0.01, "{align} shift {shift}");
    }
    // 단·쪽 기준의 개체 자리는 문단 여백의 영향을 받지 않는다.
    for relative_to in ["COLUMN", "PAGE", "PAPER"] {
        let shift = with_margins(relative_to, "LEFT");
        assert!(shift.abs() < 0.01, "{relative_to} shift {shift}");
    }
}

#[test]
fn image_fill_stretches_over_the_shape_or_centres_at_natural_size() {
    // 1×1 PNG.
    const PNG: [u8; 67] = [
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F,
        0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0x00,
        0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00, 0x00, 0x00, 0x00, 0x49,
        0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
    ];
    let drawn = |mode: &str| {
        let shape = Shape {
            width: 4000.0,
            height: 2000.0,
            image_fill: Some(ImageFill {
                mode: mode.to_string(),
                binary_id: "bin1".to_string(),
                bright: 0,
                contrast: 0,
                effect: "REAL_PIC".to_string(),
            }),
            ..Shape::default()
        };
        let mut doc = one_line(0.0, vec![RunItem::Shape(shape)]);
        doc.bins = vec![Blob {
            id: "bin1".to_string(),
            kind: BlobKind::Picture,
            data: PNG.to_vec().into(),
        }];
        let page = first_page(&doc);
        let clip = page.items.iter().find_map(|item| match item {
            Item::Clip(rect) => Some(*rect),
            _ => None,
        });
        let image = page
            .items
            .iter()
            .find_map(|item| match item {
                Item::Image { rect, .. } => Some(*rect),
                _ => None,
            })
            .expect("image drawn");
        (clip, image)
    };
    let (clip, image) = drawn("TOTAL");
    assert!(clip.is_none());
    assert!((image.width - 40.0).abs() < 0.01 && (image.height - 20.0).abs() < 0.01);
    // 가운데: 1화소 = 0.75pt 그림을 도형 가운데에 두고 도형으로 오려 낸다.
    let (clip, image) = drawn("CENTER");
    let clip = clip.expect("clipped to the shape");
    assert!((clip.width - 40.0).abs() < 0.01 && (clip.height - 20.0).abs() < 0.01);
    assert!((image.width - 0.75).abs() < 0.01 && (image.height - 0.75).abs() < 0.01);
    assert!((image.x + image.width / 2.0 - (clip.x + clip.width / 2.0)).abs() < 0.01);
    assert!((image.y + image.height / 2.0 - (clip.y + clip.height / 2.0)).abs() < 0.01);
}

#[test]
fn centred_line_puts_the_baseline_at_the_text_height_not_the_stored_value() {
    let line = LineSeg {
        textpos: 0,
        vertpos: 0.0,
        vertsize: 1000.0,
        textheight: 1000.0,
        // 한/글은 세로 정렬이 가운데인 줄에 글자 높이의 절반을 저장하지만 그 값으로 그리지 않는다.
        baseline: 500.0,
        spacing: 500.0,
        horzpos: 0.0,
        horzsize: 40000.0,
        flags: 0,
    };
    let mut doc = one_line(0.0, vec![RunItem::Text("A".to_string())]);
    doc.para_prs = vec![ParaPr {
        vert_align: "CENTER".to_string(),
        ..ParaPr::default()
    }];
    doc.sections[0].paragraphs[0].line_segs = vec![line];
    let page = first_page(&doc);
    let baseline = page
        .items
        .iter()
        .find_map(|item| match item {
            Item::Glyphs(run) if run.text.starts_with('A') => Some(run.origin.1),
            _ => None,
        })
        .unwrap();
    // 본문 위 99.2 + 글자 높이의 85%(8.5)
    assert!((baseline - 107.7).abs() < 0.01, "baseline {baseline}");
}

#[test]
fn centred_line_with_a_taller_line_box_sits_half_the_spare_height_lower() {
    let line = LineSeg {
        textpos: 0,
        vertpos: 0.0,
        vertsize: 2000.0,
        textheight: 1000.0,
        baseline: 500.0,
        spacing: 0.0,
        horzpos: 0.0,
        horzsize: 40000.0,
        flags: 0,
    };
    let mut doc = one_line(0.0, vec![RunItem::Text("A".to_string())]);
    doc.para_prs = vec![ParaPr {
        vert_align: "CENTER".to_string(),
        ..ParaPr::default()
    }];
    doc.sections[0].paragraphs[0].line_segs = vec![line];
    let page = first_page(&doc);
    let baseline = page
        .items
        .iter()
        .find_map(|item| match item {
            Item::Glyphs(run) if run.text.starts_with('A') => Some(run.origin.1),
            _ => None,
        })
        .unwrap();
    // 본문 위 99.2 + 남는 줄 높이 절반(5) + 글자 높이의 85%(8.5)
    assert!((baseline - 112.7).abs() < 0.01, "baseline {baseline}");
}

/// 10pt 'a'와 20pt 'B'를 한 줄(높이 20pt)에 두고 세로 정렬 `align`으로 그렸을 때 두 글자의 기준선.
fn mixed_baselines(align: &str) -> (f32, f32) {
    let mut doc = one_line(0.0, vec![RunItem::Text("a".to_string())]);
    doc.char_prs.push(CharPr {
        id: 1,
        height: 2000.0,
        ..doc.char_prs[0].clone()
    });
    doc.sections[0].paragraphs[0].runs.push(Run {
        char_pr_id: 1,
        items: vec![RunItem::Text("B".to_string())],
        supscript: false,
    });
    doc.para_prs = vec![ParaPr {
        vert_align: align.to_string(),
        ..ParaPr::default()
    }];
    doc.sections[0].paragraphs[0].line_segs = vec![LineSeg {
        textpos: 0,
        vertpos: 0.0,
        vertsize: 2000.0,
        textheight: 2000.0,
        // 한/글은 위 정렬에 0, 아래 정렬에 줄 높이를 저장하지만 그 값으로 그리지 않는다.
        baseline: if align == "TOP" { 0.0 } else { 2000.0 },
        spacing: 0.0,
        horzpos: 0.0,
        horzsize: 40000.0,
        flags: 0,
    }];
    let page = first_page(&doc);
    let baseline = |ch: char| {
        page.items
            .iter()
            .find_map(|item| match item {
                Item::Glyphs(run) if run.text.starts_with(ch) => Some(run.origin.1),
                _ => None,
            })
            .unwrap_or_else(|| panic!("{ch} not drawn"))
    };
    (baseline('a'), baseline('B'))
}

#[test]
fn top_aligned_glyphs_hang_from_the_line_top_by_their_own_ascent() {
    // Liberation Sans hhea ascent 0.905em, 본문 위 99.2
    let (small, large) = mixed_baselines("TOP");
    assert!((small - (99.2 + 9.05)).abs() < 0.02, "small {small}");
    assert!((large - (99.2 + 18.1)).abs() < 0.02, "large {large}");
}

#[test]
fn bottom_aligned_glyphs_stand_on_the_line_bottom_by_their_own_descent() {
    // Liberation Sans hhea descent 0.212em, 줄 아래 99.2 + 20
    let (small, large) = mixed_baselines("BOTTOM");
    assert!((small - (119.2 - 2.12)).abs() < 0.02, "small {small}");
    assert!((large - (119.2 - 4.24)).abs() < 0.02, "large {large}");
}

#[test]
fn centred_glyphs_of_mixed_sizes_each_centre_by_their_own_size() {
    // 남는 줄 높이 절반 + 그 글자 크기의 85%
    let (small, large) = mixed_baselines("CENTER");
    assert!((small - (99.2 + 5.0 + 8.5)).abs() < 0.02, "small {small}");
    assert!((large - (99.2 + 17.0)).abs() < 0.02, "large {large}");
}

/// 줄 배치가 없는 문단에서 줄마다 첫 `A`의 x. 낱말은 `A`로 시작하는 한글 다섯 자이고,
/// 시험 글꼴에 한글이 없어 한글은 그려지지 않고 반 칸씩 자리만 차지한다.
fn first_a_per_line(break_non_latin_word: &str) -> Vec<f32> {
    let mut doc = one_line(0.0, vec![RunItem::Text("A가나다라 ".repeat(60))]);
    doc.para_prs = vec![ParaPr {
        break_non_latin_word: break_non_latin_word.to_string(),
        ..ParaPr::default()
    }];
    let mut lines: Vec<(f32, f32)> = Vec::new();
    for item in first_page(&doc).items {
        if let Item::Glyphs(run) = item {
            for (range, (_, offset)) in run.ranges.iter().zip(&run.glyphs) {
                let new_line = lines
                    .last()
                    .is_none_or(|(y, _)| (run.origin.1 - y).abs() > 0.5);
                if new_line && run.text[range.clone()].starts_with('A') {
                    lines.push((run.origin.1, run.origin.0 + offset));
                }
            }
        }
    }
    lines.into_iter().map(|(_, x)| x).collect()
}

/// 한/글은 한글 줄 나눔 기준이 글자(HWPX KEEP_WORD)면 낱말 가운데서도 끊고, 어절(BREAK_WORD)이면 빈칸에서만 끊는다.
#[test]
fn hangul_lines_break_inside_words_only_when_set_to_letters() {
    let words = first_a_per_line("BREAK_WORD");
    assert!(words.len() > 2, "{words:?}");
    assert!(
        words.iter().all(|x| (x - words[0]).abs() < 0.01),
        "{words:?}"
    );
    let letters = first_a_per_line("KEEP_WORD");
    assert!(
        letters.iter().any(|x| (x - letters[0]).abs() > 1.0),
        "{letters:?}"
    );
}

/// 어울림 개체 옆 문단의 저장된 줄 폭이 개체 왼쪽 바깥 여백을 빼지 않았으면 한/글은 여백을 뺀 폭으로 줄을 다시 나눈다
/// (그림 horzOffset 32578, 바깥 여백 566, 저장된 줄 폭 32578).
#[test]
fn stored_lines_beside_a_float_rebreak_when_they_ignore_its_outer_margin() {
    let baselines = |margin: f64| {
        let float = Shape {
            width: 4000.0,
            height: 4000.0,
            pos: Pos {
                treat_as_char: false,
                text_wrap: "SQUARE".to_string(),
                horz_offset: 20000.0,
                ..Pos::default()
            },
            out_margins: Margins {
                left: margin,
                ..Margins::default()
            },
            ..Shape::default()
        };
        let mut doc = one_line(
            0.0,
            vec![RunItem::Shape(float), RunItem::Text("abcd ".repeat(7))],
        );
        doc.sections[0].paragraphs[0].line_segs = vec![LineSeg {
            textpos: 0,
            vertpos: 0.0,
            vertsize: 1000.0,
            textheight: 1000.0,
            baseline: 850.0,
            spacing: 600.0,
            horzpos: 0.0,
            horzsize: 20000.0,
            flags: 0,
        }];
        let mut ys: Vec<f32> = first_page(&doc)
            .items
            .iter()
            .filter_map(|item| match item {
                Item::Glyphs(run) => Some(run.origin.1),
                _ => None,
            })
            .collect();
        ys.dedup_by(|a, b| (*a - *b).abs() < 0.5);
        ys.len()
    };
    assert_eq!(baselines(0.0), 1);
    assert_eq!(baselines(5000.0), 2);
}

/// 양쪽 정렬은 줄 앞 빈칸을 늘리지 않지만, 누름틀 시작 뒤의 빈칸은 줄 앞이 아니어서 함께 늘린다
/// (누름틀 "본문" 뒤 빈칸 하나로 시작하는 첫 줄).
#[test]
fn justified_spaces_after_a_field_start_stretch_like_inner_spaces() {
    let first_a = |field: bool| {
        let line = |textpos: i32, vertpos: f64| LineSeg {
            textpos,
            vertpos,
            vertsize: 1000.0,
            textheight: 1000.0,
            baseline: 850.0,
            spacing: 600.0,
            horzpos: 0.0,
            horzsize: 10000.0,
            flags: 0,
        };
        let mut items = vec![RunItem::Text(" A B C".to_string())];
        if field {
            items.insert(0, RunItem::Control("hp:fieldBegin".to_string()));
        }
        let mut doc = one_line(0.0, items);
        doc.para_prs = vec![ParaPr {
            align: "JUSTIFY".to_string(),
            ..ParaPr::default()
        }];
        // 둘째 줄은 "C"만 담아 첫 줄이 마지막 줄이 아니게 한다.
        let second = if field { 6 } else { 5 };
        doc.sections[0].paragraphs[0].line_segs = vec![line(0, 0.0), line(second, 1600.0)];
        let chars = placed_chars(&doc);
        let x = |ch: char| chars.iter().find(|(c, _)| *c == ch).unwrap().1;
        (x('A'), x('B') - x('A'))
    };
    let (plain, plain_gap) = first_a(false);
    let (after_field, field_gap) = first_a(true);
    // 줄 앞 빈칸이 사이 빈칸과 남는 폭을 반씩 나눠 가지므로, 첫 글자가 밀린 만큼 사이가 준다.
    assert!(
        field_gap < plain_gap - 1.0,
        "gap {plain_gap} -> {field_gap}"
    );
    assert!(
        (after_field - plain - (plain_gap - field_gap)).abs() < 0.05,
        "plain {plain}, after field {after_field}, gap {plain_gap} -> {field_gap}"
    );
}

#[test]
fn centred_and_right_lines_leave_out_the_last_glyph_spacing() {
    // 자간 -20%·+33%인 가운데 줄은 끝 글자의 자간을 뺀 폭으로 맞춘다.
    let start = |align: &str, spacing: f64| {
        let mut doc = one_line(spacing, vec![RunItem::Text("AH".to_string())]);
        doc.para_prs = vec![ParaPr {
            align: align.to_string(),
            ..ParaPr::default()
        }];
        let chars = placed_chars(&doc);
        let x = |want: char| chars.iter().find(|(ch, _)| *ch == want).unwrap().1;
        (x('A'), x('H') - x('A'))
    };
    for (align, share) in [("CENTER", 0.5), ("RIGHT", 1.0)] {
        let (plain, _) = start(align, 0.0);
        let (spaced, a) = start(align, -20.0);
        // 줄 폭은 A만 자간만큼 줄어든다: A의 원래 폭은 a / 0.8, 줄어든 폭은 그 20%.
        let expected = share * 0.2 * a / 0.8;
        let shift = spaced - plain;
        assert!(
            (shift - expected).abs() < 0.01,
            "{align} shift {shift}, expected {expected}"
        );
    }
}

#[test]
fn latin_bullet_heads_take_half_an_em_whatever_the_glyph_width() {
    // '-' 글머리표 + 본문과의 간격 50%는 글꼴·자간과 상관없이 한 글자(1em) 뒤에서 본문을 시작한다.
    for spacing in [0.0, -10.0] {
        let mut doc = one_line(spacing, vec![RunItem::Text("AH".to_string())]);
        doc.bullets = vec![Bullet {
            id: 1,
            char: "-".to_string(),
            char_pr_id: None,
            para: ParaHead {
                text_offset: 50.0,
                ..ParaHead::default()
            },
        }];
        doc.para_prs = vec![ParaPr {
            align: "LEFT".to_string(),
            heading_type: "BULLET".to_string(),
            heading_id_ref: 1,
            ..ParaPr::default()
        }];
        doc.sections[0].paragraphs[0].head_text = Some("-".to_string());
        let chars = placed_chars(&doc);
        let x = |want: char| chars.iter().find(|(ch, _)| *ch == want).unwrap().1;
        let gap = x('A') - x('-');
        assert!((gap - 10.0).abs() < 0.01, "spacing {spacing}: gap {gap}");
    }
}

#[test]
fn overflowing_lines_shrink_spaces_to_four_fifths_then_letter_spacing() {
    // 넘치는 줄은 빈칸을 80%까지 줄이고 나머지는 글자마다 같은 폭씩 뺀다.
    // 문단 끝 줄이 아니면 줄 앞 빈칸은 '한 줄로 입력(SQUEEZE)' 문단에서만 줄어든다.
    // 문단 끝 줄은 빈칸만으로 모자라면 앞 빈칸도 줄어든다.
    let place_at = |line_wrap: &str, last: bool| {
        let text = if last { " AB CD" } else { " AB CD EF" };
        let mut doc = one_line(0.0, vec![RunItem::Text(text.to_string())]);
        doc.para_prs = vec![ParaPr {
            align: "LEFT".to_string(),
            line_wrap: line_wrap.to_string(),
            ..ParaPr::default()
        }];
        let seg = |textpos: i32, vertpos: f64| LineSeg {
            textpos,
            vertpos,
            vertsize: 1000.0,
            textheight: 1000.0,
            baseline: 850.0,
            spacing: 600.0,
            horzpos: 0.0,
            horzsize: 3500.0,
            flags: 0,
        };
        doc.sections[0].paragraphs[0].line_segs = if last {
            vec![seg(0, 0.0)]
        } else {
            vec![seg(0, 0.0), seg(7, 1600.0)]
        };
        let chars = placed_chars(&doc);
        let x = |want: char| chars.iter().find(|(ch, _)| *ch == want).unwrap().1;
        (x('A'), x('B'), x('C'))
    };
    let place = |line_wrap: &str| place_at(line_wrap, false);
    let (break_a, break_b, break_c) = place("BREAK");
    let (squeeze_a, squeeze_b, squeeze_c) = place("SQUEEZE");
    let (last_a, _, _) = place_at("BREAK", true);
    assert!(
        (last_a - squeeze_a).abs() < 0.01,
        "last line leading space: {last_a} vs {squeeze_a}"
    );
    let space = |a: f32, b: f32, c: f32| (c - b) - (b - a);
    assert!(
        (space(break_a, break_b, break_c) - 4.0).abs() < 0.01,
        "inner space {}",
        space(break_a, break_b, break_c)
    );
    assert!(
        (space(squeeze_a, squeeze_b, squeeze_c) - 4.0).abs() < 0.01,
        "inner space {}",
        space(squeeze_a, squeeze_b, squeeze_c)
    );
    assert!(
        (break_a - squeeze_a - 1.0).abs() < 0.01,
        "leading space: {break_a} vs {squeeze_a}"
    );
    // 남은 0.78pt(SQUEEZE)·1.78pt(BREAK)를 글자마다 같은 폭씩 뺀다. 마지막 글자 뒤 자간은
    // 줄 폭에 들지 않으므로 마지막 글자를 뺀 셋으로 나눈다.
    let letter_cut = |a: f32, b: f32| 1366.0 / 2048.0 * 10.0 - (b - a);
    assert!((letter_cut(squeeze_a, squeeze_b) - 0.78 / 3.0).abs() < 0.01);
    assert!((letter_cut(break_a, break_b) - 1.78 / 3.0).abs() < 0.01);
}

/// 양쪽 정렬은 고정폭 빈칸을 늘리지 않고 남는 폭을 보통 빈칸에만 나눈다.
#[test]
fn justify_leaves_fixed_width_spaces_alone() {
    let gaps = |align: &str| {
        let line = |textpos: i32, vertpos: f64| LineSeg {
            textpos,
            vertpos,
            vertsize: 1000.0,
            textheight: 1000.0,
            baseline: 850.0,
            spacing: 600.0,
            horzpos: 0.0,
            horzsize: 10000.0,
            flags: 0,
        };
        let mut doc = one_line(0.0, vec![RunItem::Text("A\u{2005}B C D".to_string())]);
        doc.para_prs = vec![ParaPr {
            align: align.to_string(),
            ..ParaPr::default()
        }];
        // 둘째 줄은 "D"만 담아 첫 줄이 마지막 줄이 아니게 한다.
        doc.sections[0].paragraphs[0].line_segs = vec![line(0, 0.0), line(6, 1600.0)];
        let chars = placed_chars(&doc);
        let x = |ch: char| chars.iter().find(|(c, _)| *c == ch).unwrap().1;
        (x('B') - x('A'), x('C') - x('B'))
    };
    let (fixed_left, plain_left) = gaps("LEFT");
    let (fixed, plain) = gaps("JUSTIFY");
    assert!((fixed - fixed_left).abs() < 0.01, "{fixed_left} -> {fixed}");
    assert!(plain > plain_left + 1.0, "{plain_left} -> {plain}");
}

/// 저장된 한 줄(폭 `horzsize` HWPUNIT)에 `font`로 적은 글자 30개를 그린 첫 글자 묶음.
fn stored_line_run(font: &str, horzsize: f64) -> ttoryeot::scene::GlyphRun {
    let mut doc = one_line(
        0.0,
        vec![RunItem::Text("ABCDEFGHIJABCDEFGHIJABCDEFGHIJ".to_string())],
    );
    doc.font_faces = BTreeMap::from([("latin".to_string(), vec![FontFace::new(font)])]);
    doc.para_prs = vec![ParaPr {
        align: "LEFT".to_string(),
        ..ParaPr::default()
    }];
    doc.sections[0].paragraphs[0].line_segs = vec![LineSeg {
        textpos: 0,
        vertpos: 0.0,
        vertsize: 1000.0,
        textheight: 1000.0,
        baseline: 850.0,
        spacing: 600.0,
        horzpos: 0.0,
        horzsize,
        flags: 0,
    }];
    first_page(&doc)
        .items
        .into_iter()
        .find_map(|item| match item {
            Item::Glyphs(run) => Some(run),
            _ => None,
        })
        .expect("glyph run")
}

#[test]
fn substituted_font_is_narrowed_to_the_stored_line_instead_of_overlapping() {
    // 문서 글꼴이 없어 더 넓은 대체 글꼴로 그리면 저장된 줄이 크게 넘친다. 글자를 가로로 줄여 맞추고 겹치지 않게 한다.
    let run = stored_line_run("Missing Narrow Font", 10000.0);
    let wide = stored_line_run("Missing Narrow Font", 60000.0);
    assert!(run.scale_x < 0.7, "scale_x {}", run.scale_x);
    assert_eq!(wide.scale_x, 1.0);
    let steps = |run: &ttoryeot::scene::GlyphRun| -> Vec<f32> {
        run.glyphs.windows(2).map(|w| w[1].1 - w[0].1).collect()
    };
    // 글자 간격이 글자 폭과 같은 비율로 줄었으면 글자끼리 겹치지 않는다(자간으로 뺐다면 좁은 글자가 겹친다).
    for (narrowed, natural) in steps(&run).iter().zip(steps(&wide)) {
        assert!(
            (narrowed - natural * run.scale_x).abs() < 0.01,
            "{narrowed} vs {natural} x {}",
            run.scale_x
        );
    }
    let last = run.glyphs.last().unwrap().1;
    assert!(last < 100.0, "line ends inside the stored width: {last}");
}

#[test]
fn installed_font_keeps_hancom_letter_spacing_squeeze() {
    // 문서 글꼴이 있으면 한/글처럼 글자 모양은 그대로 두고 빈칸·자간에서 뺀다.
    let run = stored_line_run("Liberation Sans", 15000.0);
    assert_eq!(run.scale_x, 1.0);
}
