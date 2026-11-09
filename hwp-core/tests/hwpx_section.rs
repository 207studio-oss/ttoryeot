#[path = "support/mod.rs"]
mod support;

use hwp_core::model::{Compose, Margins, Note, NoteShape, RunItem, Section, Tab};
use hwp_core::text::text_summary;
use hwp_core::{parse, ParseOptions, WarningKind};

use support::zipw::{self, Entry};

fn build_hwpx(header: &str, section: &str) -> Vec<u8> {
    let content = br#"<?xml version="1.0" encoding="UTF-8"?><opf:package xmlns:opf="http://www.idpf.org/2007/opf/"><opf:manifest><opf:item id="section0" href="Contents/section0.xml"/></opf:manifest></opf:package>"#;
    zipw::build(&[
        Entry {
            name: "mimetype",
            data: b"application/hwp+zip",
            deflate: false,
        },
        Entry {
            name: "Contents/header.xml",
            data: header.as_bytes(),
            deflate: true,
        },
        Entry {
            name: "Contents/section0.xml",
            data: section.as_bytes(),
            deflate: true,
        },
        Entry {
            name: "Contents/content.hpf",
            data: content,
            deflate: true,
        },
    ])
}

fn parse_document(section: &str) -> hwp_core::Parsed {
    let header = r#"<?xml version="1.0" encoding="UTF-8"?><hh:head xmlns:hh="urn:hh" secCnt="1"/>"#;
    parse(&build_hwpx(header, section), &ParseOptions::default()).expect("parse hwpx section")
}

fn parse_section(section: &str) -> Section {
    parse_document(section)
        .document
        .sections
        .into_iter()
        .next()
        .expect("section")
}

fn wrap(body: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><hs:sec xmlns:hs="urn:hs" xmlns:hp="urn:hp">{body}</hs:sec>"#
    )
}

#[test]
fn t_mixed_content_tab_linebreak() {
    let section = parse_section(&wrap(
        r#"<hp:p paraPrIDRef="3" styleIDRef="4"><hp:run charPrIDRef="5"><hp:t>가<hp:tab width="900" leader="DOT" type="RIGHT"/>나<hp:lineBreak/>다<hp:unknown>라</hp:unknown></hp:t></hp:run><hp:run charPrIDRef="6"><hp:tab/></hp:run></hp:p>"#,
    ));
    let paragraph = &section.paragraphs[0];
    assert_eq!(paragraph.para_pr_id, 3);
    assert_eq!(paragraph.style_id, 4);
    assert_eq!(paragraph.runs.len(), 2);
    assert_eq!(paragraph.runs[0].char_pr_id, 5);
    assert_eq!(
        paragraph.runs[0].items,
        vec![
            RunItem::Text("가".to_string()),
            RunItem::FormattedTab(Tab {
                width: Some(900.0),
                kind: "RIGHT".to_string(),
                leader: "DOT".to_string(),
            }),
            RunItem::Text("나".to_string()),
            RunItem::LineBreak,
            RunItem::Text("다".to_string()),
            RunItem::Text("라".to_string()),
        ]
    );
    assert_eq!(paragraph.runs[1].items, vec![RunItem::Tab]);
}

#[test]
fn nbspace_fwspace_hyphen() {
    for (body, expected) in [
        (
            r#"<hp:p><hp:run><hp:t>가<hp:nbSpace/>나<hp:fwSpace/>다<hp:hyphen/>라</hp:t></hp:run></hp:p>"#,
            "가\u{2007}나\u{2005}다-라",
        ),
        (
            r#"<hp:p><hp:run><hp:t> <hp:nbSpace></hp:nbSpace><hp:fwSpace></hp:fwSpace><hp:hyphen></hp:hyphen>  </hp:t></hp:run></hp:p>"#,
            " \u{2007}\u{2005}-  ",
        ),
    ] {
        let parsed = parse_document(&wrap(body));
        assert_eq!(text_summary(&parsed.document).paragraphs, [expected]);
    }
}

#[test]
fn unknown_inline_with_text_kept() {
    let parsed = parse_document(&wrap(
        r#"<hp:p><hp:run><hp:t> 가 <hp:unknown> 나 <hp:nested>다</hp:nested> 라 </hp:unknown><hp:empty/> 마 </hp:t></hp:run></hp:p>"#,
    ));
    assert_eq!(
        text_summary(&parsed.document).paragraphs,
        [" 가  나 다 라  마 "]
    );
}

#[test]
fn whitespace_preserved() {
    let section = parse_section(&wrap(
        r#"<hp:p><hp:run><hp:t>  앞  뒤  </hp:t></hp:run></hp:p><hp:p><hp:run><hp:t> 가
나 </hp:t></hp:run></hp:p>"#,
    ));
    assert_eq!(
        section.paragraphs[0].runs[0].items,
        vec![RunItem::Text("  앞  뒤  ".to_string())]
    );
    assert_eq!(
        section.paragraphs[1].runs[0].items,
        vec![RunItem::Text(" 가\n나 ".to_string())]
    );
}

#[test]
fn ctrl_header_footer_colpr_pagenum() {
    let section = parse_section(&wrap(
        r#"<hp:p><hp:run>
<hp:ctrl>
  <hp:header applyPageType="EVEN"><hp:subList><hp:p><hp:run><hp:t>머리</hp:t></hp:run></hp:p></hp:subList></hp:header>
  <hp:header applyPageType="ODD"><hp:subList><hp:p><hp:run><hp:t>둘째</hp:t></hp:run></hp:p></hp:subList></hp:header>
  <hp:colPr colCount="2" sameGap="500"/>
  <hp:colPr colCount="3" sameGap="900"/>
  <hp:pageHiding hideHeader="1" hideFooter="0" hidePageNum="true"/>
  <hp:newNum numType="PAGE" num="7"/>
  <hp:newNum numType="TOTAL_PAGE" num="9"/>
  <hp:pageNum pos="TOP_CENTER" formatType="ROMAN_CAPITAL" sideChar="-"/>
  <hp:pageNumCtrl/>
  <hp:footNote/>
</hp:ctrl>
</hp:run></hp:p>"#,
    ));
    assert_eq!(section.columns, 2);
    assert_eq!(section.column_gap, 500.0);
    assert_eq!(section.header.len(), 1);
    assert_eq!(
        section.header[0].runs[0].items,
        vec![RunItem::Text("머리".to_string())]
    );
    assert!(section.footer.is_empty());

    assert_eq!(
        section.paragraphs[0].runs[0].items,
        vec![
            RunItem::Header {
                paragraphs: vec![hwp_core::model::Paragraph {
                    runs: vec![hwp_core::model::Run {
                        char_pr_id: 0,
                        items: vec![RunItem::Text("머리".to_string())],
                        ..Default::default()
                    }],
                    // 머리말 문단도 구역 XML의 문단 차례를 가진다(바깥 문단이 0).
                    origin: Some(hwp_core::ParagraphOrigin {
                        section: 0,
                        ordinal: 1,
                    }),
                    ..Default::default()
                }],
                applies: "EVEN".to_string(),
            },
            RunItem::Header {
                paragraphs: vec![hwp_core::model::Paragraph {
                    runs: vec![hwp_core::model::Run {
                        char_pr_id: 0,
                        items: vec![RunItem::Text("둘째".to_string())],
                        ..Default::default()
                    }],
                    // 머리말 문단도 구역 XML의 문단 차례를 가진다(바깥 문단이 0).
                    origin: Some(hwp_core::ParagraphOrigin {
                        section: 0,
                        ordinal: 2,
                    }),
                    ..Default::default()
                }],
                applies: "ODD".to_string(),
            },
            RunItem::Skip(8),
            RunItem::Skip(8),
            RunItem::PageHiding {
                header: true,
                footer: false,
                page_num: true,
            },
            RunItem::NewNum(7),
            RunItem::PageNum {
                pos: "TOP_CENTER".to_string(),
                format: "ROMAN_CAPITAL".to_string(),
                side: "-".to_string(),
            },
            RunItem::Note(Note {
                seq: 1,
                ..Note::default()
            }),
        ]
    );
}

#[test]
fn secpr_landscape_swap() {
    let section = parse_section(&wrap(
        r#"<hp:p><hp:run><hp:secPr tabStopVal="3000"><hp:pagePr width="1000" height="2000" landscape="LANDSCAPE"><hp:margin left="11" right="12" top="13" bottom="14" header="15" footer="16" gutter="17"/></hp:pagePr></hp:secPr></hp:run></hp:p>"#,
    ));
    assert_eq!(section.tab_stop, 3000.0);
    assert_eq!(section.page.width, 2000.0);
    assert_eq!(section.page.height, 1000.0);
    assert!(section.page.landscape);
    assert_eq!(section.page.margin_left, 11.0);
    assert_eq!(section.page.margin_right, 12.0);
    assert_eq!(section.page.margin_top, 13.0);
    assert_eq!(section.page.margin_bottom, 14.0);
    assert_eq!(section.page.header, 15.0);
    assert_eq!(section.page.footer, 16.0);
    assert_eq!(section.page.gutter, 17.0);

    let portrait = parse_section(&wrap(
        r#"<hp:p><hp:run><hp:secPr tabStop="2500"><hp:pagePr width="1000" height="2000" landscape="WIDELY"><hp:margin/></hp:pagePr></hp:secPr></hp:run></hp:p>"#,
    ));
    assert_eq!(portrait.tab_stop, 2500.0);
    assert!(!portrait.page.landscape);
    assert_eq!(portrait.page.width, 1000.0);
    assert_eq!(portrait.page.height, 2000.0);
}

#[test]
fn table_cells_margins() {
    let section = parse_section(&wrap(
        r#"<hp:p><hp:run><hp:tbl rowCnt="1" colCnt="2" cellSpacing="10" borderFillIDRef="7" zOrder="2" repeatHeader="1" pageBreak="TABLE" textWrap="TOP_AND_BOTTOM">
<hp:sz width="500" height="600"/>
<hp:pos treatAsChar="0" vertRelTo="PAGE" horzRelTo="PARA" vertAlign="CENTER" horzAlign="RIGHT" vertOffset="5" horzOffset="6" flowWithText="0" allowOverlap="1" affectLSpacing="1"/>
<hp:outMargin left="1" right="2" top="3" bottom="4"/>
<hp:inMargin left="21" right="22" top="23" bottom="24"/>
<hp:tr>
  <hp:tc borderFillIDRef="8" header="1">
    <hp:cellAddr colAddr="0" rowAddr="0"/>
    <hp:cellSpan colSpan="1" rowSpan="1"/>
    <hp:cellSz width="100" height="200"/>
    <hp:cellMargin left="31" right="32" top="33" bottom="34"/>
    <hp:subList vertAlign="CENTER"><hp:p><hp:run><hp:t>칸</hp:t></hp:run></hp:p></hp:subList>
  </hp:tc>
  <hp:tc>
    <hp:cellAddr colAddr="1" rowAddr="0"/>
    <hp:subList><hp:p><hp:run><hp:t>둘</hp:t></hp:run></hp:p></hp:subList>
  </hp:tc>
</hp:tr>
</hp:tbl></hp:run></hp:p>"#,
    ));
    let table = match &section.paragraphs[0].runs[0].items[0] {
        RunItem::Table(table) => table,
        other => panic!("expected table, got {other:?}"),
    };
    assert_eq!(table.rows, 1);
    assert_eq!(table.cols, 2);
    assert_eq!(table.cell_spacing, 10.0);
    assert_eq!(table.border_fill_id, 7);
    assert_eq!(table.z_order, 2);
    assert!(table.repeat_header);
    assert_eq!(table.page_break_mode, "TABLE");
    assert_eq!(table.width, 500.0);
    assert_eq!(table.height, 600.0);
    assert_eq!(table.pos.text_wrap, "TOP_AND_BOTTOM");
    assert_eq!(table.pos.vert_rel_to, "PAGE");
    assert_eq!(table.pos.horz_rel_to, "PARA");
    assert_eq!(table.pos.vert_align, "CENTER");
    assert_eq!(table.pos.horz_align, "RIGHT");
    assert_eq!(table.pos.vert_offset, 5.0);
    assert_eq!(table.pos.horz_offset, 6.0);
    assert!(!table.pos.treat_as_char);
    assert!(!table.pos.flow_with_text);
    assert!(table.pos.allow_overlap);
    assert!(table.pos.affect_l_spacing);
    assert_eq!(
        table.out_margins,
        Margins {
            left: 1.0,
            right: 2.0,
            top: 3.0,
            bottom: 4.0,
        }
    );
    assert_eq!(
        table.in_margins,
        Margins {
            left: 21.0,
            right: 22.0,
            top: 23.0,
            bottom: 24.0,
        }
    );

    let first = &table.cells[0];
    assert_eq!(first.border_fill_id, 8);
    assert!(first.header);
    assert_eq!(first.col, 0);
    assert_eq!(first.row, 0);
    assert!(first.has_margin);
    assert_eq!(
        first.margins,
        Margins {
            left: 31.0,
            right: 32.0,
            top: 33.0,
            bottom: 34.0,
        }
    );
    assert_eq!(first.vert_align, "CENTER");
    assert_eq!(first.width, 100.0);
    assert_eq!(first.height, 200.0);
    assert_eq!(
        first.paragraphs[0].runs[0].items,
        vec![RunItem::Text("칸".to_string())]
    );

    let second = &table.cells[1];
    assert!(!second.has_margin);
    assert_eq!(second.margins, table.in_margins);
    assert_eq!(second.vert_align, "TOP");
}

#[test]
fn hwpx_has_margin_attr() {
    let own = Margins {
        left: 31.0,
        right: 32.0,
        top: 33.0,
        bottom: 34.0,
    };
    let inherited = Margins {
        left: 21.0,
        right: 22.0,
        top: 23.0,
        bottom: 24.0,
    };
    for attr in [Some("0"), Some("1"), Some("false"), Some("true"), None] {
        for present in [false, true] {
            let attribute = attr
                .map(|value| format!(" hasMargin=\"{value}\""))
                .unwrap_or_default();
            let margin = if present {
                r#"<hp:cellMargin left="31" right="32" top="33" bottom="34"/>"#
            } else {
                ""
            };
            let section = parse_section(&wrap(&format!(
                r#"<hp:p><hp:run><hp:tbl rowCnt="1" colCnt="1">
<hp:inMargin left="21" right="22" top="23" bottom="24"/>
<hp:tr><hp:tc{attribute}>{margin}</hp:tc></hp:tr>
</hp:tbl></hp:run></hp:p>"#
            )));
            let RunItem::Table(table) = &section.paragraphs[0].runs[0].items[0] else {
                panic!("expected table");
            };
            assert_eq!(table.in_margins, inherited);
            assert_eq!(table.cells.len(), 1);
            let cell = &table.cells[0];
            let has_margin = attr.map_or(present, |value| matches!(value, "1" | "true"));
            assert_eq!(
                cell.has_margin, has_margin,
                "attr={attr:?}, present={present}"
            );
            let expected = if !has_margin {
                inherited.clone()
            } else if present {
                own.clone()
            } else {
                Margins::default()
            };
            assert_eq!(cell.margins, expected, "attr={attr:?}, present={present}");
        }
    }
}

#[test]
fn hwpx_caption() {
    let section = parse_section(&wrap(
        r#"<hp:p><hp:run>
<hp:tbl rowCnt="1" colCnt="1">
  <hp:sz width="500" height="600"/>
  <hp:caption side="BOTTOM" fullSz="1" width="8504" gap="850">
    <hp:subList><hp:p><hp:run><hp:t>table caption</hp:t></hp:run></hp:p></hp:subList>
  </hp:caption>
  <hp:tr><hp:tc><hp:cellAddr colAddr="0" rowAddr="0"/><hp:cellSpan colSpan="1" rowSpan="1"/><hp:cellSz width="500" height="600"/><hp:subList><hp:p><hp:run><hp:t>cell</hp:t></hp:run></hp:p></hp:subList></hp:tc></hp:tr>
</hp:tbl>
<hp:pic>
  <hp:sz width="300" height="400"/>
  <hp:caption side="TOP" fullSz="0" width="300" gap="120">
    <hp:subList><hp:p><hp:run><hp:t>picture caption</hp:t></hp:run></hp:p></hp:subList>
  </hp:caption>
  <hc:img binaryItemIDRef="bin1"/>
</hp:pic>
<hp:rect>
  <hp:sz width="100" height="100"/>
  <hp:caption side="RIGHT" width="200" gap="50">
    <hp:subList><hp:p><hp:run><hp:t>shape caption</hp:t></hp:run></hp:p></hp:subList>
  </hp:caption>
  <hp:drawText><hp:textMargin left="1" right="2" top="3" bottom="4"/><hp:subList><hp:p><hp:run><hp:t>textbox text</hp:t></hp:run></hp:p></hp:subList></hp:drawText>
</hp:rect>
</hp:run></hp:p>"#,
    ));
    let items = &section.paragraphs[0].runs[0].items;
    let table = match &items[0] {
        RunItem::Table(table) => table,
        other => panic!("expected table, got {other:?}"),
    };
    assert_eq!(table.cells.len(), 1);
    assert_eq!(
        table.cells[0].paragraphs[0].runs[0].items,
        vec![RunItem::Text("cell".to_string())]
    );
    let caption = table.caption.as_ref().expect("table caption");
    assert_eq!(caption.side, "BOTTOM");
    assert!(caption.full_size);
    assert_eq!(caption.width, 8504.0);
    assert_eq!(caption.gap, 850.0);
    assert_eq!(
        caption.paragraphs[0].runs[0].items,
        vec![RunItem::Text("table caption".to_string())]
    );

    let picture = match &items[1] {
        RunItem::Picture(picture) => picture,
        other => panic!("expected picture, got {other:?}"),
    };
    let caption = picture.caption.as_ref().expect("picture caption");
    assert_eq!(caption.side, "TOP");
    assert!(!caption.full_size);
    assert_eq!(caption.gap, 120.0);
    assert_eq!(
        caption.paragraphs[0].runs[0].items,
        vec![RunItem::Text("picture caption".to_string())]
    );

    let shape = match &items[2] {
        RunItem::Shape(shape) => shape,
        other => panic!("expected shape, got {other:?}"),
    };
    let caption = shape.caption.as_ref().expect("shape caption");
    assert_eq!(caption.side, "RIGHT");
    assert_eq!(caption.width, 200.0);
    assert_eq!(caption.gap, 50.0);
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
fn pic_clip_coordinates() {
    let section = parse_section(&wrap(
        r#"<hp:p><hp:run><hp:pic zOrder="3" textWrap="BEHIND_TEXT"><hp:sz width="300" height="400"/><hp:pos treatAsChar="0" vertRelTo="PAPER" horzAlign="CENTER"/><hp:outMargin left="1" right="2" top="3" bottom="4"/><hc:img binaryItemIDRef="bin3"/><hp:flip horizontal="1" vertical="0"/><hp:rotationInfo angle="45"/><hp:imgClip left="10" top="20" right="110" bottom="220"/><hp:imgDim dimwidth="900" dimheight="1000"/></hp:pic><hp:pic><hp:imgClip left="50" top="60" right="40" bottom="30"/></hp:pic></hp:run></hp:p>"#,
    ));
    let picture = match &section.paragraphs[0].runs[0].items[0] {
        RunItem::Picture(picture) => picture,
        other => panic!("expected picture, got {other:?}"),
    };
    assert_eq!(picture.width, 300.0);
    assert_eq!(picture.height, 400.0);
    assert_eq!(picture.z_order, 3);
    assert_eq!(picture.pos.text_wrap, "BEHIND_TEXT");
    assert_eq!(picture.pos.vert_rel_to, "PAPER");
    assert_eq!(picture.pos.horz_align, "CENTER");
    assert!(!picture.pos.treat_as_char);
    assert_eq!(picture.binary_id, "bin3");
    assert!(picture.flip_h);
    assert!(!picture.flip_v);
    assert_eq!(picture.rotation, 45.0);
    assert_eq!(picture.clip, Some([[10.0, 20.0], [100.0, 200.0]]));
    assert_eq!(picture.orig_width, 900.0);
    assert_eq!(picture.orig_height, 1000.0);
    let empty = match &section.paragraphs[0].runs[0].items[1] {
        RunItem::Picture(picture) => picture,
        other => panic!("expected picture, got {other:?}"),
    };
    assert_eq!(empty.clip, None);
    assert_eq!(empty.outline, None);
}

#[test]
fn pic_line_shape_is_an_outline_and_draw_text_keeps_its_vertical_align() {
    let section = parse_section(&wrap(
        r##"<hp:p><hp:run><hp:pic><hp:lineShape color="#FF0000" width="0" style="SOLID"/></hp:pic><hp:pic><hp:lineShape color="#000000" width="283" style="NONE"/></hp:pic><hp:rect><hp:drawText><hp:subList vertAlign="CENTER"><hp:p><hp:run><hp:t>box</hp:t></hp:run></hp:p></hp:subList></hp:drawText></hp:rect></hp:run></hp:p>"##,
    ));
    let items = &section.paragraphs[0].runs[0].items;
    let RunItem::Picture(outlined) = &items[0] else {
        panic!("expected picture, got {:?}", items[0]);
    };
    // 굵기 0은 한/글의 가장 가는 선(0.1mm)이다.
    let outline = outlined.outline.as_ref().expect("outline");
    assert_eq!(
        (outline.border_type.as_str(), outline.width, outline.color),
        ("SOLID", 0.1, 0xFF0000)
    );
    let RunItem::Picture(plain) = &items[1] else {
        panic!("expected picture, got {:?}", items[1]);
    };
    assert_eq!(plain.outline, None);
    let RunItem::Shape(shape) = &items[2] else {
        panic!("expected shape, got {:?}", items[2]);
    };
    assert_eq!(shape.text_vert_align, "CENTER");
}

#[test]
fn lineseg_flags_out_of_range_warns() {
    let parsed = parse_document(&wrap(
        r#"<hp:p><hp:run><hp:t>x</hp:t></hp:run><hp:linesegarray>
<hp:lineseg textpos="1" vertpos="2" vertsize="3" textheight="4" baseline="5" spacing="6" horzpos="7" horzsize="8" flags="4294967295"/>
<hp:lineseg flags="-1"/>
<hp:lineseg flags="4294967296"/>
<hp:lineseg/>
</hp:linesegarray></hp:p>"#,
    ));
    let segments = &parsed.document.sections[0].paragraphs[0].line_segs;
    assert_eq!(segments.len(), 4);
    assert_eq!(segments[0].flags, u32::MAX);
    assert_eq!(segments[0].textpos, 1);
    assert_eq!(segments[0].vertpos, 2.0);
    assert_eq!(segments[0].horzsize, 8.0);
    assert_eq!(segments[1].flags, 0);
    assert_eq!(segments[2].flags, 0);
    assert_eq!(segments[3].flags, 0);
    let warnings: Vec<_> = parsed
        .warnings
        .iter()
        .filter(|warning| warning.kind == WarningKind::InvalidLineSegFlags)
        .collect();
    assert_eq!(warnings.len(), 2);
}

#[test]
fn equation_attrs() {
    let section = parse_section(&wrap(
        r##"<hp:p><hp:run><hp:equation textColor="#112233" baseUnit="1200" baseLine="70" font="HYhwpEQ"><hp:sz width="450" height="120"/><hp:pos treatAsChar="1" vertRelTo="PARA"/><hp:outMargin left="2"/><hp:script> {a} over {b} </hp:script></hp:equation><hp:equation baseUnit="100000" baseLine="0"/></hp:run></hp:p>"##,
    ));
    let shape = match &section.paragraphs[0].runs[0].items[0] {
        RunItem::Shape(shape) => shape,
        other => panic!("expected equation shape, got {other:?}"),
    };
    assert_eq!(shape.kind, "equation");
    assert_eq!(shape.line_type, "NONE");
    assert_eq!(shape.width, 450.0);
    assert_eq!(shape.height, 120.0);
    assert_eq!(shape.pos.vert_rel_to, "PARA");
    assert_eq!(shape.out_margins.left, 2.0);
    let equation = shape.equation.as_ref().expect("equation");
    assert_eq!(equation.script, " {a} over {b} ");
    assert_eq!(equation.size, 1200.0);
    assert_eq!(equation.color, 0x112233);
    assert_eq!(equation.baseline, 70.0);
    assert_eq!(equation.font, "HYhwpEQ");

    let clamped = match &section.paragraphs[0].runs[0].items[1] {
        RunItem::Shape(shape) => shape,
        other => panic!("expected equation shape, got {other:?}"),
    };
    let equation = clamped.equation.as_ref().expect("equation");
    assert_eq!(equation.size, 1000.0);
    assert_eq!(equation.baseline, 0.0);
}

#[test]
fn hwpx_footnote_note_item() {
    let section = parse_section(&wrap(
        r#"<hp:p><hp:run><hp:secPr><hp:footNotePr><hp:autoNumFormat type="CIRCLED_DIGIT" userChar="" prefixChar="" suffixChar=")"/></hp:footNotePr></hp:secPr></hp:run></hp:p>
<hp:p><hp:run><hp:ctrl><hp:footNote number="7"><hp:subList><hp:p><hp:run><hp:ctrl><hp:autoNum num="7" numType="FOOTNOTE"><hp:autoNumFormat type="CIRCLED_DIGIT" prefixChar="[" suffixChar="]"/></hp:autoNum></hp:ctrl><hp:t>각주 내용</hp:t></hp:run></hp:p></hp:subList></hp:footNote></hp:ctrl></hp:run></hp:p>"#,
    ));
    assert_eq!(section.footnote_shape.format, 1);
    let note = match &section.paragraphs[1].runs[0].items[0] {
        RunItem::Note(note) => note,
        other => panic!("expected note, got {other:?}"),
    };
    assert!(!note.endnote);
    assert_eq!(note.number, 7);
    assert_eq!(note.seq, 1);
    assert_eq!(note.format, 1);
    assert_eq!(note.prefix, "[");
    assert_eq!(note.suffix, "]");
    assert_eq!(note.paragraphs.len(), 1);
    assert_eq!(
        note.paragraphs[0].runs[0].items,
        vec![
            RunItem::AutoNum("FOOTNOTE".to_string()),
            RunItem::Text("각주 내용".to_string())
        ]
    );
}

#[test]
fn hwpx_endnote() {
    let section = parse_section(&wrap(
        r#"<hp:p><hp:run><hp:ctrl><hp:footNote number="2"><hp:subList><hp:p><hp:run><hp:ctrl><hp:autoNum num="2" numType="FOOTNOTE"><hp:autoNumFormat type="DIGIT" prefixChar="" suffixChar=")"/></hp:autoNum></hp:ctrl><hp:t>각주</hp:t></hp:run></hp:p></hp:subList></hp:footNote></hp:ctrl><hp:ctrl><hp:endNote number="4"><hp:subList><hp:p><hp:run><hp:ctrl><hp:autoNum num="1" numType="ENDNOTE"><hp:autoNumFormat type="ROMAN_CAPITAL" prefixChar="" suffixChar=")"/></hp:autoNum></hp:ctrl><hp:t>미주 내용</hp:t></hp:run></hp:p></hp:subList></hp:endNote></hp:ctrl></hp:run></hp:p>"#,
    ));
    let items = &section.paragraphs[0].runs[0].items;
    let footnote = match &items[0] {
        RunItem::Note(note) => note,
        other => panic!("expected note, got {other:?}"),
    };
    let endnote = match &items[1] {
        RunItem::Note(note) => note,
        other => panic!("expected note, got {other:?}"),
    };
    assert!(!footnote.endnote);
    assert_eq!(footnote.seq, 1);
    assert!(endnote.endnote);
    assert_eq!(endnote.number, 4);
    assert_eq!(endnote.seq, 2);
    assert_eq!(endnote.format, 2);
    assert_eq!(
        endnote.paragraphs[0].runs[0].items,
        vec![
            RunItem::AutoNum("ENDNOTE".to_string()),
            RunItem::Text("미주 내용".to_string())
        ]
    );
}

#[test]
fn hwpx_footnote_pr_shape() {
    let section = parse_section(&wrap(
        r##"<hp:p><hp:run><hp:secPr><hp:footNotePr><hp:autoNumFormat type="HANGUL_SYLLABLE" userChar="" prefixChar="" suffixChar=")"/><hp:noteLine length="12345" type="DASH" width="0.3 mm" color="#ABCDEF"/><hp:noteSpacing betweenNotes="111" belowLine="222" aboveLine="333"/></hp:footNotePr></hp:secPr></hp:run></hp:p>"##,
    ));
    assert_eq!(
        section.footnote_shape,
        NoteShape {
            format: 8,
            line_length: 12345.0,
            above_line: 333.0,
            below_line: 222.0,
            between_notes: 111.0,
            line_type: 2,
            line_width: 0.3,
            line_color: 0xAB_CD_EF,
        }
    );
}

#[test]
fn hwpx_line_positions_count_tab_as_eight() {
    let parsed = parse_document(&wrap(
        r#"<hp:p><hp:run><hp:t><hp:tab width="1000" leader="0" type="1"/>AAAAA BBBBB</hp:t></hp:run><hp:linesegarray>
<hp:lineseg textpos="0" vertpos="0" vertsize="1000" textheight="1000" baseline="850" spacing="600" horzpos="0" horzsize="40000" flags="393216"/>
<hp:lineseg textpos="14" vertpos="1600" vertsize="1000" textheight="1000" baseline="850" spacing="600" horzpos="0" horzsize="40000" flags="393216"/>
</hp:linesegarray></hp:p>"#,
    ));
    let lines = hwp_core::probe::probe(&parsed.document);
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[1].text, "BBBBB");
}

#[test]
fn hwpx_line_positions_count_section_and_column_definitions() {
    let parsed = parse_document(&wrap(
        r#"<hp:p><hp:run><hp:secPr/><hp:ctrl><hp:colPr type="NEWSPAPER" colCount="1" sameSz="1" sameGap="0"/></hp:ctrl><hp:t>AAAAA BBBBB</hp:t></hp:run><hp:linesegarray>
<hp:lineseg textpos="0" vertpos="0" vertsize="1000" textheight="1000" baseline="850" spacing="600" horzpos="0" horzsize="40000" flags="393216"/>
<hp:lineseg textpos="22" vertpos="1600" vertsize="1000" textheight="1000" baseline="850" spacing="600" horzpos="0" horzsize="40000" flags="393216"/>
</hp:linesegarray></hp:p>"#,
    ));
    let lines = hwp_core::probe::probe(&parsed.document);
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0].text, "AAAAA ");
    assert_eq!(lines[1].text, "BBBBB");
}

#[test]
fn ctrl_auto_num_is_a_page_number() {
    let section = parse_section(&wrap(
        r#"<hp:p><hp:run><hp:ctrl><hp:autoNum num="1" numType="PAGE"><hp:autoNumFormat type="DIGIT"/></hp:autoNum></hp:ctrl></hp:run></hp:p>"#,
    ));
    assert_eq!(
        section.paragraphs[0].runs[0].items,
        vec![RunItem::AutoNum("PAGE".to_string())]
    );
}

#[test]
fn hwpx_compose() {
    let section = parse_section(&wrap(
        r#"<hp:p><hp:run><hp:t>앞</hp:t><hp:compose circleType="SHAPE_REVERSAL_CIRCLE" charSz="-3" composeType="SPREAD" charPrCnt="2" composeText="10"><hp:charPr prIDRef="65"/><hp:charPr prIDRef="4294967295"/></hp:compose><hp:t>뒤</hp:t></hp:run></hp:p>"#,
    ));
    let items = &section.paragraphs[0].runs[0].items;
    assert_eq!(
        items[1],
        RunItem::Compose(Compose {
            text: "10".to_string(),
            shape: "SHAPE_REVERSAL_CIRCLE".to_string(),
            char_size: -3,
            overlap: false,
        })
    );
    assert_eq!(items[2], RunItem::Text("뒤".to_string()));
}
