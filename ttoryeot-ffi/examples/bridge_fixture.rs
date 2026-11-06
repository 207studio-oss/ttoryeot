use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::error::Error;
use std::fs;
use std::path::Path;

use hwp_core::*;

fn values(base: f64) -> BTreeMap<String, HwpUnit> {
    [
        "hangul", "latin", "hanja", "japanese", "other", "symbol", "user",
    ]
    .into_iter()
    .enumerate()
    .map(|(index, language)| (language.to_string(), base + index as f64))
    .collect()
}

fn font_refs() -> BTreeMap<String, i32> {
    [
        "hangul", "latin", "hanja", "japanese", "other", "symbol", "user",
    ]
    .into_iter()
    .enumerate()
    .map(|(index, language)| (language.to_string(), index as i32 + 1))
    .collect()
}

fn paragraph(text: &str) -> Paragraph {
    Paragraph {
        runs: vec![Run {
            char_pr_id: 1,
            items: vec![RunItem::Text(text.to_string())],
            ..Run::default()
        }],
        line_segs: vec![LineSeg {
            textpos: 1,
            vertpos: 2.5,
            vertsize: 3.5,
            textheight: 4.5,
            baseline: 5.5,
            spacing: 6.5,
            horzpos: 7.5,
            horzsize: 8.5,
            flags: 3,
        }],
        page_break: true,
        column_break: true,
        para_pr_id: 1,
        style_id: 2,
        ..Paragraph::default()
    }
}

fn caption(text: &str) -> Caption {
    Caption {
        side: "BOTTOM".to_string(),
        gap: 850.0,
        width: 42520.0,
        full_size: true,
        paragraphs: vec![paragraph(text)],
    }
}

fn items() -> Vec<RunItem> {
    let cell = Cell {
        col: 1,
        row: 2,
        col_span: 2,
        row_span: 3,
        width: 101.5,
        height: 102.5,
        border_fill_id: 3,
        margins: Margins {
            left: 1.5,
            right: 2.5,
            top: 3.5,
            bottom: 4.5,
        },
        vert_align: "CENTER".to_string(),
        paragraphs: vec![paragraph("cell")],
        has_margin: true,
        header: true,
    };
    let table = Table {
        rows: 3,
        cols: 4,
        width: 201.5,
        height: 202.5,
        pos: Pos {
            treat_as_char: false,
            vert_rel_to: "PAGE".to_string(),
            horz_rel_to: "PAPER".to_string(),
            width_rel_to: "COLUMN".to_string(),
            height_rel_to: "PAGE".to_string(),
            vert_align: "BOTTOM".to_string(),
            horz_align: "RIGHT".to_string(),
            vert_offset: 11.5,
            horz_offset: 12.5,
            flow_with_text: false,
            allow_overlap: true,
            affect_l_spacing: true,
            text_wrap: "TIGHT".to_string(),
        },
        out_margins: Margins {
            left: 21.5,
            right: 22.5,
            top: 23.5,
            bottom: 24.5,
        },
        in_margins: Margins {
            left: 31.5,
            right: 32.5,
            top: 33.5,
            bottom: 34.5,
        },
        border_fill_id: 4,
        cells: vec![cell],
        cell_spacing: 41.5,
        z_order: 5,
        repeat_header: true,
        page_break_mode: "TABLE".to_string(),
        caption: Some(caption("table caption")),
    };
    let picture = Picture {
        width: 301.5,
        height: 302.5,
        pos: Pos::default(),
        out_margins: Margins::default(),
        binary_id: "bin1".to_string(),
        z_order: 6,
        border_fill_id: 7,
        flip_h: true,
        flip_v: true,
        rotation: 42.5,
        crop_left: 1.0,
        crop_right: 2.0,
        crop_top: 3.0,
        crop_bottom: 4.0,
        orig_width: 501.5,
        orig_height: 502.5,
        clip: Some([[10.5, 20.5], [30.5, 40.5]]),
        caption: Some(caption("picture caption")),
        outline: None,
    };
    let child = ShapeChild {
        item: Box::new(RunItem::Picture(picture.clone())),
        frame: [[51.5, 52.5], [53.5, 54.5]],
        transform: Some([1.0, 0.5, 0.25, 1.0, 10.5, -20.5]),
    };
    let shape = Shape {
        kind: "container".to_string(),
        width: 601.5,
        height: 602.5,
        pos: Pos::default(),
        out_margins: Margins::default(),
        line_color: 0x102030,
        line_width: 0.25,
        line_type: "DASH".to_string(),
        arrow_start: Some(Arrow {
            style: "SPEAR".into(),
            size: "SMALL_MEDIUM".into(),
            fill: false,
        }),
        arrow_end: Some(Arrow {
            style: "EMPTY_DIAMOND".into(),
            size: "MEDIUM_LARGE".into(),
            fill: true,
        }),
        gradient: Some(Gradient {
            kind: "LINEAR".into(),
            angle: 45.0,
            center_x: 25.0,
            center_y: 75.0,
            step: 10,
            step_center: 50,
            colors: vec![0x112233, 0x445566],
            positions: vec![],
        }),
        image_fill: Some(ImageFill {
            mode: "TOTAL".into(),
            binary_id: "bin1".into(),
            bright: -20,
            contrast: 30,
            effect: "GRAY_SCALE".into(),
        }),
        segment_types: vec!["CURVE".into()],
        arc: Some(Arc {
            kind: "CHORD".into(),
            center: [50.0, 25.0],
            axis1: [100.0, 25.0],
            axis2: [50.0, 50.0],
        }),
        fill_color: Some(0x405060),
        fill_alpha: 0.75,
        round_ratio: 12.5,
        z_order: 8,
        paragraphs: vec![paragraph("textbox")],
        text_margins: Margins {
            left: 61.5,
            right: 62.5,
            top: 63.5,
            bottom: 64.5,
        },
        text_vert_align: "TOP".to_string(),
        points: vec![[71.5, 72.5], [73.5, 74.5]],
        rotation: 81.5,
        equation: Some(Equation {
            script: "x^2".to_string(),
            size: 1200.5,
            color: 0x112233,
            baseline: 90.5,
            font: "Equation".to_string(),
        }),
        children: vec![child],
        caption: Some(caption("shape caption")),
    };
    let note = Note {
        endnote: true,
        number: 2,
        seq: 3,
        format: 4,
        prefix: "[".to_string(),
        suffix: "]".to_string(),
        paragraphs: vec![paragraph("note")],
    };
    vec![
        RunItem::Text("text \u{2022}\n\t\"\\".to_string()),
        RunItem::Table(table),
        RunItem::Picture(picture),
        RunItem::Shape(shape),
        RunItem::Tab,
        RunItem::SizedTab(77.25),
        RunItem::FormattedTab(Tab {
            width: Some(88.5),
            kind: "RIGHT".to_string(),
            leader: "DOT".to_string(),
        }),
        RunItem::LineBreak,
        RunItem::AutoNum("PAGE".to_string()),
        RunItem::Control("secd".to_string()),
        RunItem::Note(note),
        RunItem::Skip(5),
        RunItem::Header {
            paragraphs: vec![paragraph("header")],
            applies: "BOTH".to_string(),
        },
        RunItem::Footer {
            paragraphs: vec![paragraph("footer")],
            applies: "EVEN".to_string(),
        },
        RunItem::PageHiding {
            header: true,
            footer: false,
            page_num: true,
        },
        RunItem::NewNum(6),
        RunItem::PageNum {
            pos: "BOTTOM".to_string(),
            format: "DIGIT".to_string(),
            side: "CENTER".to_string(),
        },
    ]
}

fn document() -> Document {
    let mut font_faces = BTreeMap::new();
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
                name: format!("{language} face"),
                face_type: 1,
                alt_name: Some(format!("{language} alt")),
                alt_type: 2,
                default_name: Some(format!("{language} default")),
                embedded: true,
                binary_ref: Some("fontbin".to_string()),
            }],
        );
    }
    Document {
        font_faces,
        char_prs: vec![CharPr {
            id: 1,
            height: 1234.5,
            text_color: 0x112233,
            shade_color: Some(0x445566),
            font_ref: font_refs(),
            ratio: values(90.0),
            spacing: values(-1.5),
            rel_sz: values(100.0),
            offset: values(0.5),
            bold: true,
            italic: true,
            underline: "BOTTOM".to_string(),
            underline_color: 0x778899,
            strikeout: true,
            supscript: true,
            sub_script: true,
            emboss: true,
            engrave: true,
            outline: true,
            shadow: true,
            use_font_space: true,
            kerning: true,
            underline_shape: "WAVE".to_string(),
            sym_mark: "CARON".to_string(),
            border_fill_id_ref: 513,
            strikeout_color: 0xAABBCC,
            shadow_type: "CONTINUOUS".to_string(),
            shadow_offset_x: -12.0,
            shadow_offset_y: 34.0,
            shadow_color: 0x778899,
        }],
        para_prs: vec![ParaPr {
            id: 1,
            align: "CENTER".to_string(),
            vert_align: "TOP".to_string(),
            margin_left: 1.5,
            margin_right: 2.5,
            indent: 3.5,
            prev: 4.5,
            next: 5.5,
            line_spacing_type: "FIXED".to_string(),
            line_spacing: 180.5,
            heading_type: "NUMBER".to_string(),
            heading_id_ref: 2,
            heading_level: 3,
            tab_pr_id_ref: 4,
            border_fill_id_ref: 5,
            break_latin_word: "BREAK_WORD".to_string(),
            break_non_latin_word: "KEEP_WORD".to_string(),
            line_wrap: "BREAK".to_string(),
            keep_with_next: true,
            page_break_before: true,
            widow_orphan: true,
            keep_lines: true,
            snap_to_grid: true,
            condense: 37,
            font_line_height: true,
            connect: true,
            ignore_margin: true,
            tail_shape: true,
        }],
        tab_prs: vec![TabPr {
            id: 1,
            auto_left: true,
            auto_right: true,
            stops: vec![TabStop {
                position: 123.5,
                kind: "RIGHT".to_string(),
                leader: "DASH".to_string(),
            }],
        }],
        border_fills: vec![BorderFill {
            id: 1,
            left: Border {
                border_type: "SOLID".to_string(),
                width: 0.25,
                color: 0x111111,
            },
            right: Border {
                border_type: "DASH".to_string(),
                width: 0.5,
                color: 0x222222,
            },
            top: Border {
                border_type: "DOT".to_string(),
                width: 0.75,
                color: 0x333333,
            },
            bottom: Border {
                border_type: "DOUBLE".to_string(),
                width: 1.0,
                color: 0x444444,
            },
            diagonal: Border {
                border_type: "NONE".to_string(),
                width: 0.1,
                color: 0x555555,
            },
            slash: "SLASH".to_string(),
            back_slash: "BACK_SLASH".to_string(),
            fill_color: Some(0x667788),
            gradient: Some(vec![(0x0A0B0C, 12.5), (0xDDEEFF, 87.5)]),
            gradation: None,
        }],
        numberings: vec![Numbering {
            id: 1,
            start: 2,
            heads: vec![NumberingHead {
                level: 1,
                format: "ROMAN_CAPITAL".to_string(),
                text: "^1.".to_string(),
                align: "RIGHT".to_string(),
                start: 3,
                char_pr_id: Some(1),
                para: ParaHead {
                    auto_indent: true,
                    use_inst_width: true,
                    width_adjust: 4.5,
                    text_offset_percent: false,
                    text_offset: 5.5,
                },
            }],
        }],
        bullets: vec![Bullet {
            id: 1,
            char: "-".to_string(),
            char_pr_id: Some(1),
            para: ParaHead {
                auto_indent: true,
                use_inst_width: false,
                width_adjust: 6.5,
                text_offset_percent: true,
                text_offset: 7.5,
            },
        }],
        styles: vec![Style {
            id: 1,
            para_pr_id: 2,
            char_pr_id: 3,
            ..Style::default()
        }],
        sections: vec![Section {
            page: Page {
                width: 60000.0,
                height: 90000.0,
                margin_left: 1000.0,
                margin_right: 2000.0,
                margin_top: 3000.0,
                margin_bottom: 4000.0,
                header: 5000.0,
                footer: 6000.0,
                gutter: 7000.0,
                landscape: true,
            },
            paragraphs: vec![Paragraph {
                head_text: Some("①.".to_string()),
                range_tags: vec![RangeTag {
                    start: 0,
                    end: 4,
                    tag: 0xA112_3456,
                }],
                markpens: vec![Markpen {
                    start: 1,
                    end: 3,
                    color: 0x778899,
                }],
                tracks: Vec::new(),
                columns: Some(ColumnDef {
                    count: 2,
                    gap: 500.0,
                    same_width: false,
                    widths: vec![12000.0, 18000.0],
                    line_type: "DOT".to_string(),
                    line_width: 0.3,
                    line_color: 0x123456,
                    direction: "RIGHT".to_string(),
                }),
                para_pr_id: 1,
                style_id: 2,
                runs: vec![
                    Run {
                        char_pr_id: 1,
                        items: items(),
                        ..Run::default()
                    },
                    Run {
                        char_pr_id: 2,
                        items: vec![RunItem::Text("second run".to_string())],
                        supscript: true,
                    },
                ],
                line_segs: vec![LineSeg {
                    textpos: 1,
                    vertpos: 2.5,
                    vertsize: 3.5,
                    textheight: 4.5,
                    baseline: 5.5,
                    spacing: 6.5,
                    horzpos: 7.5,
                    horzsize: 8.5,
                    flags: 3,
                }],
                page_break: true,
                column_break: true,
                memos: vec![Memo {
                    shape_id: 1,
                    paragraphs: vec![paragraph("memo")],
                }],
            }],
            header: vec![paragraph("section header")],
            footer: vec![paragraph("section footer")],
            columns: 2,
            column_gap: 1234.5,
            tab_stop: 4321.5,
            outline_numbering_id: None,
            master_pages: Vec::new(),
            page_border_fills: vec![PageBorderFill {
                kind: "EVEN".to_string(),
                border_fill_id: 7,
                text_border: "CONTENT".to_string(),
                header_inside: true,
                footer_inside: false,
                fill_area: "BORDER".to_string(),
                offsets: Margins {
                    left: 100.0,
                    right: 200.0,
                    top: 300.0,
                    bottom: 400.0,
                },
            }],
            footnote_shape: NoteShape {
                format: 8,
                line_length: 12345.5,
                above_line: 850.5,
                below_line: 567.5,
                between_notes: 283.5,
                line_type: 2,
                line_width: 0.35,
                line_color: 0x998877,
            },
        }],
        memo_shapes: vec![MemoShape {
            id: 1,
            width: 15591.5,
            line_type: 1,
            line_width: 2,
            line_color: 0x00A1B2,
            fill_color: 0x00C3D4,
            active_color: 0x00E5F6,
            memo_type: 1,
        }],
        wchar_positions: true,
        track_changes: false,
        track_markup_hidden: false,
        bins: vec![
            Blob {
                id: "bin1".to_string(),
                kind: BlobKind::Picture,
                data: vec![0x00, 0xFF, 0x10, 0x7B, 0x22, 0x5C, 0x0A].into(),
            },
            Blob {
                id: "fontbin".to_string(),
                kind: BlobKind::Font,
                data: vec![0xDE, 0xAD, 0xBE, 0xEF, 0x00, 0x01].into(),
            },
        ],
    }
}

fn array_to_id_map(value: &mut Value) -> Result<(), String> {
    let array = value.as_array().ok_or("expected array")?;
    let mut map = Map::new();
    for item in array {
        let id = item
            .get("id")
            .and_then(Value::as_i64)
            .ok_or("array item without id")?;
        map.insert(id.to_string(), item.clone());
    }
    *value = Value::Object(map);
    Ok(())
}

fn font_faces_to_map(value: &mut Value) -> Result<(), String> {
    array_to_id_map(value)?;
    if let Some(map) = value.as_object_mut() {
        for face in map.values_mut() {
            if let Some(object) = face.as_object_mut() {
                object.remove("id");
            }
        }
    }
    Ok(())
}

fn styles_to_map(value: &mut Value) -> Result<(), String> {
    let array = value.as_array().ok_or("expected styles array")?;
    let mut map = Map::new();
    for item in array {
        let id = item
            .get("id")
            .and_then(Value::as_i64)
            .ok_or("style without id")?;
        let para = item
            .get("paraPrID")
            .cloned()
            .ok_or("style without paraPrID")?;
        let character = item
            .get("charPrID")
            .cloned()
            .ok_or("style without charPrID")?;
        map.insert(id.to_string(), Value::Array(vec![para, character]));
    }
    *value = Value::Object(map);
    Ok(())
}

fn document_value(document: &Document) -> Result<Value, Box<dyn Error>> {
    let mut value = serde_json::to_value(document)?;
    let root = value.as_object_mut().ok_or("document is not an object")?;
    for key in [
        "charPrs",
        "paraPrs",
        "tabPrs",
        "borderFills",
        "numberings",
        "bullets",
    ] {
        if let Some(field) = root.get_mut(key) {
            array_to_id_map(field)?;
        }
    }
    if let Some(field) = root.get_mut("styles") {
        styles_to_map(field)?;
    }
    if let Some(Value::Object(languages)) = root.get_mut("fontFaces") {
        for faces in languages.values_mut() {
            font_faces_to_map(faces)?;
        }
    }
    let mut fonts = Map::new();
    for (language, faces) in document.font_faces.iter() {
        let mut named = Map::new();
        for face in faces {
            named.insert(face.id.to_string(), Value::String(face.name.clone()));
        }
        fonts.insert(language.clone(), Value::Object(named));
    }
    root.insert("fonts".to_string(), Value::Object(fonts));
    let mut bin_data = Map::new();
    let mut embedded_fonts = Vec::new();
    for blob in &document.bins {
        let hex = blob
            .data
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        match blob.kind {
            BlobKind::Picture => {
                bin_data.insert(blob.id.clone(), Value::String(hex));
            }
            BlobKind::Font => embedded_fonts.push(Value::String(hex)),
        }
    }
    root.insert("binData".to_string(), Value::Object(bin_data));
    root.insert("embeddedFonts".to_string(), Value::Array(embedded_fonts));
    Ok(value)
}

fn canonical(value: &Value, path: &str, lines: &mut Vec<String>) {
    match value {
        Value::Object(map) => {
            lines.push(format!("{path}.count={}", map.len()));
            for (key, value) in map {
                canonical(value, &format!("{path}.{key}"), lines);
            }
        }
        Value::Array(values) => {
            lines.push(format!("{path}.count={}", values.len()));
            for (index, value) in values.iter().enumerate() {
                canonical(value, &format!("{path}.[{index}]"), lines);
            }
        }
        Value::String(text) => lines.push(format!(
            "{path}={}",
            serde_json::to_string(text).unwrap_or_default()
        )),
        Value::Number(number) => {
            let text = if let Some(value) = number.as_i64() {
                value.to_string()
            } else if let Some(value) = number.as_u64() {
                value.to_string()
            } else {
                format!("{:.4}", number.as_f64().unwrap_or_default())
            };
            lines.push(format!("{path}={text}"));
        }
        Value::Bool(value) => lines.push(format!("{path}={value}")),
        Value::Null => lines.push(format!("{path}=null")),
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let out = std::env::args_os()
        .nth(1)
        .ok_or("usage: bridge_fixture <out-dir>")?;
    let out = Path::new(&out);
    fs::create_dir_all(out)?;
    let mut model = document();
    model.sanitize_non_finite();
    let warning = Warning::new(WarningKind::MissingBinData, Some(0), "bin2");
    let frame = hwp_core::frame::success_frame(&model, std::slice::from_ref(&warning))?;
    fs::write(out.join("frame.bin"), frame)?;
    let value = document_value(&model)?;
    let mut lines = Vec::new();
    canonical(&value, "document", &mut lines);
    lines.sort();
    fs::write(out.join("rust.canon"), lines.join("\n") + "\n")?;
    Ok(())
}
