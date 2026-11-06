#[path = "support/mod.rs"]
mod support;

use hwp_core::model::{RunItem, Section, Shape, ShapeChild};
use hwp_core::{parse, ParseOptions, Parsed};

use support::zipw::{self, Entry};

fn build_hwpx(section: &str) -> Vec<u8> {
    let header = r#"<?xml version="1.0" encoding="UTF-8"?><hh:head xmlns:hh="urn:hh" secCnt="1"/>"#;
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

fn parse_document(section: &str) -> Parsed {
    parse(&build_hwpx(section), &ParseOptions::default()).expect("parse hwpx shapes")
}

fn parse_section(body: &str) -> Section {
    let section = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><hs:sec xmlns:hs="urn:hs" xmlns:hp="urn:hp" xmlns:hc="urn:hc">{body}</hs:sec>"#
    );
    parse_document(&section)
        .document
        .sections
        .into_iter()
        .next()
        .expect("section")
}

fn first_shape(section: &Section) -> &Shape {
    match &section.paragraphs[0].runs[0].items[0] {
        RunItem::Shape(shape) => shape,
        other => panic!("expected shape, got {other:?}"),
    }
}

fn child_shape(child: &ShapeChild) -> &Shape {
    match child.item.as_ref() {
        RunItem::Shape(shape) => shape,
        other => panic!("expected shape child, got {other:?}"),
    }
}

#[test]
fn size_rel_to() {
    for tag in ["tbl", "pic", "rect", "container", "equation"] {
        for width in ["PAPER", "PAGE", "COLUMN", "PARA", "ABSOLUTE"] {
            for height in ["PAPER", "PAGE", "ABSOLUTE"] {
                let section = parse_section(&format!(
                    r#"<hp:p><hp:run><hp:{tag} textWrap="TOP_AND_BOTTOM"><hp:sz width="400" height="300" hp:widthRelTo="{width}" hp:heightRelTo="{height}"/><hp:pos horzAlign="OUTSIDE"/></hp:{tag}></hp:run></hp:p>"#
                ));
                let item = &section.paragraphs[0].runs[0].items[0];
                let value = serde_json::to_value(item).expect("serialize object");
                assert_eq!(value["v"]["pos"]["widthRelTo"], width, "{tag}");
                assert_eq!(value["v"]["pos"]["heightRelTo"], height, "{tag}");
                assert_eq!(value["v"]["pos"]["textWrap"], "TOP_AND_BOTTOM");
                assert_eq!(value["v"]["pos"]["horzAlign"], "OUTSIDE");
                assert_eq!(value["v"]["width"], 400.0);
                assert_eq!(value["v"]["height"], 300.0);
            }
        }
    }
}

#[test]
fn size_rel_to_defaults_without_pos_or_size() {
    for tag in ["tbl", "pic", "rect", "container", "equation"] {
        for size in [
            "",
            r#"<hp:sz width="400" height="300"/>"#,
            r#"<hp:sz widthRelTo="COLUMN"/>"#,
            r#"<hp:sz heightRelTo="PAGE"/>"#,
        ] {
            let section = parse_section(&format!(
                "<hp:p><hp:run><hp:{tag}>{size}</hp:{tag}></hp:run></hp:p>"
            ));
            let value = serde_json::to_value(&section.paragraphs[0].runs[0].items[0])
                .expect("serialize object");
            let width = if size.contains("COLUMN") {
                "COLUMN"
            } else {
                "ABSOLUTE"
            };
            let height = if size.contains("PAGE") {
                "PAGE"
            } else {
                "ABSOLUTE"
            };
            assert_eq!(value["v"]["pos"]["widthRelTo"], width, "{tag}");
            assert_eq!(value["v"]["pos"]["heightRelTo"], height, "{tag}");
        }
    }
}

#[test]
fn shape_uses_current_size_over_specified_size() {
    let section = parse_section(
        r#"<hp:p><hp:run><hp:rect><hp:curSz width="24520" height="16544"/><hp:sz width="24520" height="10088"/><hp:drawText><hp:subList><hp:p><hp:run><hp:t>글</hp:t></hp:run></hp:p></hp:subList></hp:drawText></hp:rect></hp:run></hp:p>"#,
    );
    let rect = first_shape(&section);
    assert_eq!(rect.width, 24520.0);
    assert_eq!(rect.height, 16544.0);

    let section = parse_section(
        r#"<hp:p><hp:run><hp:rect><hp:curSz width="0" height="0"/><hp:sz width="400" height="300"/></hp:rect></hp:run></hp:p>"#,
    );
    let rect = first_shape(&section);
    assert_eq!(rect.width, 400.0);
    assert_eq!(rect.height, 300.0);

    // 한/글 12.30.0은 글이 넘쳐 높이만 늘린 글상자를 `curSz width="0"`으로 저장한다.
    let section = parse_section(
        r#"<hp:p><hp:run><hp:rect><hp:curSz width="0" height="6366"/><hp:sz width="20000" height="6000"/></hp:rect></hp:run></hp:p>"#,
    );
    let rect = first_shape(&section);
    assert_eq!(rect.width, 20000.0);
    assert_eq!(rect.height, 6366.0);
}

#[test]
fn container_children_frames() {
    let section = parse_section(
        r##"<hp:p><hp:run><hp:container>
<hp:rect>
  <hp:offset x="100" y="200"/><hp:curSz width="300" height="400"/><hp:sz width="300" height="400"/>
  <hp:lineShape color="#112233" width="7200" style="DASH"/>
  <hc:winBrush faceColor="#445566"/>
  <hp:drawText><hp:textMargin left="1" right="2" top="3" bottom="4"/><hp:subList><hp:p><hp:run><hp:t>글</hp:t></hp:run></hp:p></hp:subList></hp:drawText>
</hp:rect>
<hp:line>
  <hp:renderingInfo><hp:transMatrix e1="1" e2="0" e3="50" e4="0" e5="1" e6="60"/></hp:renderingInfo>
  <hp:orgSz width="20" height="10"/>
  <hp:startPt x="0" y="0"/><hp:endPt x="20" y="10"/>
</hp:line>
<hp:pic><hp:offset x="5" y="6"/><hp:curSz width="7" height="8"/><hc:img binaryItemIDRef="bin1"/></hp:pic>
</hp:container></hp:run></hp:p>"##,
    );
    let container = first_shape(&section);
    assert_eq!(container.kind, "container");
    assert_eq!(container.line_type, "NONE");
    assert_eq!(container.children.len(), 3);

    let rect = &container.children[0];
    assert_eq!(rect.frame, [[100.0, 200.0], [300.0, 400.0]]);
    assert_eq!(rect.transform, None);
    let rect = child_shape(rect);
    assert_eq!(rect.kind, "rect");
    assert_eq!(rect.width, 300.0);
    assert_eq!(rect.height, 400.0);
    assert_eq!(rect.line_color, 0x112233);
    assert_eq!(rect.line_width, 25.4);
    assert_eq!(rect.line_type, "DASH");
    assert_eq!(rect.fill_color, Some(0x445566));
    assert_eq!(rect.text_margins.left, 1.0);
    assert_eq!(rect.text_margins.top, 3.0);
    assert_eq!(
        rect.paragraphs[0].runs[0].items,
        vec![RunItem::Text("글".to_string())]
    );

    let line = &container.children[1];
    assert_eq!(line.frame, [[50.0, 60.0], [20.0, 10.0]]);
    assert_eq!(line.transform, None);
    let line = child_shape(line);
    assert_eq!(line.kind, "line");
    assert_eq!(line.width, 20.0);
    assert_eq!(line.height, 10.0);
    assert_eq!(line.points, vec![[0.0, 0.0], [20.0, 10.0]]);

    let picture = &container.children[2];
    assert_eq!(picture.frame, [[5.0, 6.0], [7.0, 8.0]]);
    assert_eq!(picture.transform, None);
    match picture.item.as_ref() {
        RunItem::Picture(picture) => {
            assert_eq!(picture.width, 7.0);
            assert_eq!(picture.height, 8.0);
            assert_eq!(picture.binary_id, "bin1");
        }
        other => panic!("expected picture child, got {other:?}"),
    }
}

#[test]
fn nested_container_offsets() {
    let section = parse_section(
        r#"<hp:p><hp:run><hp:container>
<hp:offset x="500" y="500"/>
<hp:container><hp:offset x="100" y="200"/>
  <hp:container><hp:offset x="10" y="20"/>
    <hp:rect><hp:offset x="1" y="2"/><hp:curSz width="3" height="4"/></hp:rect>
  </hp:container>
</hp:container>
</hp:container></hp:run></hp:p>"#,
    );
    let container = first_shape(&section);
    assert_eq!(container.children.len(), 1);
    let child = &container.children[0];
    assert_eq!(child.frame, [[111.0, 222.0], [3.0, 4.0]]);
    assert_eq!(child_shape(child).width, 3.0);
    assert_eq!(child_shape(child).height, 4.0);
}

#[test]
fn turning_matrix_rotated_child() {
    let section = parse_section(
        r#"<hp:p><hp:run><hp:container>
<hp:rect>
  <hp:sz width="200" height="100"/><hp:orgSz width="200" height="100"/>
  <hp:renderingInfo>
    <hp:transMatrix e1="1" e2="0" e3="0" e4="0" e5="1" e6="0"/>
    <hp:scaMatrix e1="1" e2="0" e3="0" e4="0" e5="1" e6="0"/>
    <hp:rotMatrix e1="0" e2="-1" e3="0" e4="1" e5="0" e6="0"/>
  </hp:renderingInfo>
</hp:rect>
</hp:container></hp:run></hp:p>"#,
    );
    let container = first_shape(&section);
    let child = &container.children[0];
    assert_eq!(child.frame, [[-100.0, 0.0], [100.0, 200.0]]);
    assert_eq!(child.transform, Some([0.0, 1.0, -1.0, 0.0, 0.0, 0.0]));
    let shape = child_shape(child);
    assert_eq!(shape.width, 200.0);
    assert_eq!(shape.height, 100.0);
}

#[test]
fn container_is_parsed_not_rejected() {
    let parsed = parse_document(
        r#"<?xml version="1.0" encoding="UTF-8"?><hs:sec xmlns:hs="urn:hs" xmlns:hp="urn:hp" xmlns:hc="urn:hc"><hp:p><hp:run><hp:container>
<hp:pic><hp:offset x="0" y="0"/><hp:curSz width="10" height="10"/><hc:img binaryItemIDRef="bin1"/></hp:pic>
<hp:rect><hp:offset x="10" y="10"/><hp:curSz width="20" height="20"/></hp:rect>
</hp:container></hp:run></hp:p></hs:sec>"#,
    );
    assert_eq!(parsed.document.sections.len(), 1);
    let container = first_shape(&parsed.document.sections[0]);
    assert_eq!(container.kind, "container");
    assert_eq!(container.children.len(), 2);
}

#[test]
fn curve_seg_points() {
    let section = parse_section(
        r#"<hp:p><hp:run><hp:curve>
<hp:renderingInfo><hp:transMatrix e1="1" e2="0" e3="10" e4="0" e5="1" e6="20"/></hp:renderingInfo>
<hp:seg x1="0" y1="0" x2="100" y2="0"/>
<hp:seg x1="100" y1="0" x2="100" y2="50"/>
</hp:curve></hp:run></hp:p>"#,
    );
    let curve = first_shape(&section);
    assert_eq!(curve.kind, "curve");
    assert_eq!(
        curve.points,
        vec![[10.0, 20.0], [110.0, 20.0], [110.0, 70.0]]
    );
}

#[test]
fn hwpx_arrows_and_connect_line() {
    let section = parse_section(
        r#"<hp:p><hp:run><hp:connectLine>
<hp:sz width="400" height="300"/><hp:pos horzOffset="25" vertOffset="50"/>
<hp:lineShape style="DASH" headStyle="EMPTY_DIAMOND" tailStyle="SPEAR" headSz="SMALL_LARGE" tailSz="LARGE_SMALL" headfill="1" tailfill="0"/>
<hp:startPt x="10" y="20"/><hp:endPt x="30" y="40"/>
</hp:connectLine></hp:run></hp:p>"#,
    );
    let shape = first_shape(&section);
    assert_eq!(shape.kind, "line");
    assert_eq!(shape.points, [[10.0, 20.0], [30.0, 40.0]]);
    assert_eq!(
        (
            shape.width,
            shape.height,
            shape.pos.horz_offset,
            shape.pos.vert_offset
        ),
        (400.0, 300.0, 25.0, 50.0)
    );
    assert_eq!(shape.line_type, "DASH");
    let start = shape.arrow_start.as_ref().unwrap();
    assert_eq!(
        (start.style.as_str(), start.size.as_str(), start.fill),
        ("EMPTY_DIAMOND", "SMALL_LARGE", true)
    );
    let end = shape.arrow_end.as_ref().unwrap();
    assert_eq!(
        (end.style.as_str(), end.size.as_str(), end.fill),
        ("SPEAR", "LARGE_SMALL", false)
    );
    let section = parse_section(
        r#"<hp:p><hp:run><hp:line><hp:lineShape headStyle="NORMAL" tailStyle="NORMAL" headfill="1" tailfill="1"/></hp:line></hp:run></hp:p>"#,
    );
    assert!(first_shape(&section).arrow_start.is_none());
    assert!(first_shape(&section).arrow_end.is_none());
}

#[test]
fn hwpx_gradient_image_fill() {
    let section = parse_section(
        r##"<hp:p><hp:run><hp:rect><hp:fillBrush>
<hc:winBrush faceColor="#123456"/>
<hc:gradation type="RADIAL" angle="45" centerX="30" centerY="70" step="12" stepCenter="60">
<hc:color value="#112233"/><hc:color value="#445566"/><hc:color value="#778899"/>
</hc:gradation>
<hc:imgBrush mode="CENTER"><hc:img binaryItemIDRef="image9" bright="-40" contrast="35" effect="GRAY_SCALE"/></hc:imgBrush>
</hp:fillBrush></hp:rect></hp:run></hp:p>"##,
    );
    let shape = first_shape(&section);
    let gradient = shape.gradient.as_ref().unwrap();
    assert_eq!(gradient.kind, "RADIAL");
    assert_eq!(
        (
            gradient.angle,
            gradient.center_x,
            gradient.center_y,
            gradient.step,
            gradient.step_center
        ),
        (45.0, 30.0, 70.0, 12, 60)
    );
    assert_eq!(gradient.colors, [0x112233, 0x445566, 0x778899]);
    assert!(gradient.positions.is_empty());
    let image = shape.image_fill.as_ref().unwrap();
    assert_eq!(
        (
            image.mode.as_str(),
            image.binary_id.as_str(),
            image.bright,
            image.contrast,
            image.effect.as_str()
        ),
        ("CENTER", "image9", -40, 35, "GRAY_SCALE")
    );
    assert_eq!((shape.fill_color, shape.fill_alpha), (Some(0x123456), 1.0));
}

#[test]
fn hwpx_curve_segment_types_and_arc_geometry() {
    let section = parse_section(
        r#"<hp:p><hp:run><hp:curve>
<hp:seg type="CURVE" x1="1" y1="2" x2="3" y2="4"/>
<hp:seg type="LINE" x1="3" y1="4" x2="5" y2="6"/>
</hp:curve></hp:run></hp:p>"#,
    );
    let shape = first_shape(&section);
    assert_eq!(shape.segment_types, ["CURVE", "LINE"]);
    assert_eq!(shape.points, [[1.0, 2.0], [3.0, 4.0], [5.0, 6.0]]);
    let section = parse_section(
        r#"<hp:p><hp:run><hp:arc type="CHORD">
<hp:renderingInfo><hp:transMatrix e1="2" e2="0" e3="10" e4="0" e5="3" e6="20"/></hp:renderingInfo>
<hp:center x="50" y="25"/><hp:ax1 x="100" y="25"/><hp:ax2 x="50" y="50"/>
</hp:arc></hp:run></hp:p>"#,
    );
    let arc = first_shape(&section).arc.as_ref().unwrap();
    assert_eq!(arc.kind, "CHORD");
    assert_eq!(
        (arc.center, arc.axis1, arc.axis2),
        ([110.0, 95.0], [210.0, 95.0], [110.0, 170.0])
    );
    let section = parse_section(
        r#"<hp:p><hp:run><hp:container><hp:arc type="PIE">
<hp:orgSz width="200" height="100"/>
<hp:renderingInfo><hp:transMatrix e1="2" e2="0" e3="10" e4="0" e5="3" e6="20"/></hp:renderingInfo>
<hp:center x="50" y="25"/><hp:ax1 x="100" y="25"/><hp:ax2 x="50" y="50"/>
</hp:arc></hp:container></hp:run></hp:p>"#,
    );
    let container = first_shape(&section);
    let arc = child_shape(&container.children[0]).arc.as_ref().unwrap();
    assert_eq!(
        (arc.center, arc.axis1, arc.axis2),
        ([100.0, 75.0], [200.0, 75.0], [100.0, 150.0])
    );
}

#[test]
fn hwpx_ole_placeholder_shape() {
    for name in ["ole", "chart"] {
        let section = parse_section(&format!(
            r#"<hp:p><hp:run><hp:{name} zOrder="7" textWrap="TOP_AND_BOTTOM">
<hp:sz width="400" height="300"/><hp:pos horzOffset="25" vertOffset="50"/>
</hp:{name}></hp:run></hp:p>"#
        ));
        let shape = first_shape(&section);
        assert_eq!(shape.kind, "ole");
        assert_eq!(
            (
                shape.width,
                shape.height,
                shape.pos.horz_offset,
                shape.pos.vert_offset,
                shape.z_order
            ),
            (400.0, 300.0, 25.0, 50.0, 7)
        );
        assert_eq!(shape.pos.text_wrap, "TOP_AND_BOTTOM");
        for size in ["sz", "curSz"] {
            let section = parse_section(&format!(
                r#"<hp:p><hp:run><hp:container><hp:{name}>
<hp:offset x="10" y="20"/><hp:{size} width="400" height="300"/>
</hp:{name}><hp:connectLine><hp:{size} width="50" height="60"/>
<hp:startPt x="1" y="2"/><hp:endPt x="3" y="4"/></hp:connectLine>
</hp:container></hp:run></hp:p>"#
            ));
            let children = &first_shape(&section).children;
            assert_eq!(children.len(), 2);
            assert_eq!(children[0].frame, [[10.0, 20.0], [400.0, 300.0]]);
            assert_eq!(child_shape(&children[0]).kind, "ole");
            assert_eq!(child_shape(&children[1]).kind, "line");
            assert_eq!(children[1].frame[1], [50.0, 60.0]);
        }
    }
}

#[test]
fn dump_shape_fill_distribution() {
    let bytes = build_hwpx(
        r##"<hs:sec xmlns:hs="urn:hs" xmlns:hp="urn:hp" xmlns:hc="urn:hc"><hp:p><hp:run>
<hp:rect><hp:fillBrush><hc:gradation type="LINEAR"><hc:color value="#123456"/></hc:gradation></hp:fillBrush></hp:rect>
<hp:line/></hp:run></hp:p></hs:sec>"##,
    );
    let parsed = parse(&bytes, &ParseOptions::default()).unwrap();
    assert_eq!(
        serde_json::to_value(hwp_core::probe::shape_fill_distribution(&parsed.document)).unwrap(),
        serde_json::json!({ "shapes": 2, "solid": 0, "gradient": 1, "image": 0, "unfilled": 1 })
    );
}

#[test]
fn unsigned_written_negative_values_are_signed() {
    // 한/글은 -203을 4294967093으로 적는다.
    let section = parse_section(
        r#"<hp:p><hp:run><hp:container><hp:sz width="1000" height="1000"/><hp:pos treatAsChar="0" vertOffset="4294967093" horzOffset="4294967295"/>
<hp:rect><hp:offset x="4294967286" y="20"/><hp:curSz width="300" height="400"/></hp:rect>
</hp:container></hp:run></hp:p>"#,
    );
    let container = first_shape(&section);
    assert_eq!(container.pos.vert_offset, -203.0);
    assert_eq!(container.pos.horz_offset, -1.0);
    assert_eq!(container.children[0].frame[0], [-10.0, 20.0]);
}

#[test]
fn current_size_written_negative_falls_back_to_specified_size() {
    let section = parse_section(
        r#"<hp:p><hp:run><hp:rect><hp:curSz width="300" height="4294965239"/><hp:sz width="300" height="400"/></hp:rect></hp:run></hp:p>"#,
    );
    assert_eq!(first_shape(&section).height, 400.0);
}
