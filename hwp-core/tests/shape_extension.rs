use hwp_core::hwp5::control::{parse_control, ParseContext, ParseState};
use hwp_core::hwp5::record::Record;
use hwp_core::{Arc, Arrow, Document, RunItem, Section, Shape};

fn record(tag: u16, data: Vec<u8>, children: Vec<Record>) -> Record {
    Record {
        tag,
        level: 0,
        data,
        children,
    }
}

fn component(tail: &[u8], geometry: Record, grouped: bool) -> Shape {
    let mut data = 7u32.to_le_bytes().to_vec();
    data.extend([0; 12]);
    for size in [200u32, 100, 200, 100] {
        data.extend(size.to_le_bytes());
    }
    data.extend([0; 16]);
    for value in [2.0f64, 0.0, 10.0, 0.0, 3.0, 20.0] {
        data.extend(value.to_le_bytes());
    }
    data.extend(tail);
    let inner = record(76, data, vec![geometry]);
    let drawing = if grouped {
        record(76, vec![], vec![inner])
    } else {
        inner
    };
    let mut common: Vec<u8> = b" osg".to_vec();
    common.extend([0; 36]);
    let control = record(71, common, vec![drawing]);
    let document = Document::default();
    let mut section = Section::default();
    let mut state = ParseState::default();
    let mut warnings = Vec::new();
    let mut context =
        ParseContext::new(&document, &mut section, &mut state, Some(0), &mut warnings);
    match parse_control(&control, &mut context) {
        Some(RunItem::Shape(shape)) => shape,
        other => panic!("expected shape: {other:?}"),
    }
}

fn style(attr: u32, fill_type: u32, fill: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend(0x00332211u32.to_le_bytes());
    bytes.extend(1440i32.to_le_bytes());
    bytes.extend(attr.to_le_bytes());
    bytes.push(0);
    bytes.extend(fill_type.to_le_bytes());
    bytes.extend(fill);
    bytes
}

fn shape(attr: u32, fill_type: u32, fill: &[u8]) -> Shape {
    component(
        &style(attr, fill_type, fill),
        record(79, vec![0], vec![]),
        false,
    )
}

fn gradient(kind: u8, count: i32) -> Vec<u8> {
    let mut bytes = vec![kind];
    for value in [45i32, 25, 75, 10, count] {
        bytes.extend(value.to_le_bytes());
    }
    if count > 2 && count < 10 {
        for position in [0i32, 40, 100].into_iter().take(count as usize) {
            bytes.extend(position.to_le_bytes());
        }
    }
    for color in [0x00332211u32, 0x00665544, 0x00998877]
        .into_iter()
        .take(count.clamp(0, 3) as usize)
    {
        bytes.extend(color.to_le_bytes());
    }
    bytes
}

#[test]
fn arrow_bits() {
    let attr = 2 | (1 << 6) | (2 << 10) | (1 << 16) | (4 << 22) | (4 << 26) | (1 << 31);
    let value = shape(attr, 0, &[]);
    assert_eq!(
        value.arrow_start,
        Some(Arrow {
            style: "SPEAR".into(),
            size: "MEDIUM_MEDIUM".into(),
            fill: false
        })
    );
    assert_eq!(
        value.arrow_end,
        Some(Arrow {
            style: "ARROW".into(),
            size: "MEDIUM_MEDIUM".into(),
            fill: true
        })
    );
    assert_eq!(value.line_type, shape(2, 0, &[]).line_type);
    assert_eq!(value.line_color, 0x112233);
    let value = serde_json::to_value(value).unwrap();
    assert_eq!(value["arrowStart"]["style"], "SPEAR");
    assert_eq!(value["arrowEnd"]["fill"], true);
    for (code, name) in [
        (1, "ARROW"),
        (2, "SPEAR"),
        (3, "CONCAVE_ARROW"),
        (4, "EMPTY_DIAMOND"),
        (5, "EMPTY_CIRCLE"),
        (6, "EMPTY_BOX"),
    ] {
        for (size, expected) in [
            "SMALL_SMALL",
            "SMALL_MEDIUM",
            "SMALL_LARGE",
            "MEDIUM_SMALL",
            "MEDIUM_MEDIUM",
            "MEDIUM_LARGE",
            "LARGE_SMALL",
            "LARGE_MEDIUM",
            "LARGE_LARGE",
            "MEDIUM_MEDIUM",
        ]
        .into_iter()
        .enumerate()
        {
            let arrow = shape((code << 10) | ((size as u32) << 22) | (1 << 30), 0, &[])
                .arrow_start
                .unwrap();
            assert_eq!(
                (arrow.style.as_str(), arrow.size.as_str(), arrow.fill),
                (name, expected, true)
            );
        }
    }
    for code in [0, 7, 63] {
        let value = shape((code << 10) | (code << 16) | (15 << 6), 0, &[]);
        assert!(value.arrow_start.is_none() && value.arrow_end.is_none());
    }
}

#[test]
fn gradient_fill_stops() {
    for (kind, expected) in [
        (1, "LINEAR"),
        (2, "RADIAL"),
        (3, "CONICAL"),
        (4, "SQUARE"),
        (255, "LINEAR"),
    ] {
        for count in [0, 1, 2, 3] {
            let mut fill = gradient(kind, count);
            fill.extend(2u32.to_le_bytes());
            fill.extend([67, 99]);
            let value = shape(0, 4, &fill).gradient.unwrap();
            assert_eq!(value.kind, expected);
            assert_eq!(
                (value.angle, value.center_x, value.center_y),
                (45.0, 25.0, 75.0)
            );
            assert_eq!((value.step, value.step_center), (10, 67));
            assert_eq!(
                value.colors,
                [0x112233, 0x445566, 0x778899][..count as usize]
            );
            assert_eq!(
                value.positions,
                if count == 3 {
                    vec![0.0, 40.0, 100.0]
                } else {
                    vec![]
                }
            );
        }
    }
    let mut fill = gradient(1, 2);
    fill.extend(0u32.to_le_bytes());
    let value = serde_json::to_value(shape(0, 4, &fill)).unwrap();
    assert_eq!(value["gradient"]["stepCenter"], 50);
    assert_eq!(value["gradient"]["positions"], serde_json::json!([]));
    for count in [-1, i32::MAX] {
        assert!(shape(0, 4, &gradient(1, count)).gradient.is_none());
    }
    let fill = gradient(1, 3);
    for length in 0..fill.len() {
        assert!(shape(0, 4, &fill[..length]).gradient.is_none());
    }
}

#[test]
fn image_fill_and_solid_alpha_are_preserved() {
    let modes = [
        "TILE",
        "TILE_HORZ_TOP",
        "TILE_HORZ_BOTTOM",
        "TILE_VERT_LEFT",
        "TILE_VERT_RIGHT",
        "TOTAL",
        "CENTER",
        "CENTER_TOP",
        "CENTER_BOTTOM",
        "LEFT_CENTER",
        "LEFT_TOP",
        "LEFT_BOTTOM",
        "RIGHT_CENTER",
        "RIGHT_TOP",
        "RIGHT_BOTTOM",
        "ZOOM",
        "TOTAL",
    ];
    for (mode, expected) in modes.into_iter().enumerate() {
        for (effect, name) in [
            "REAL_PIC",
            "GRAY_SCALE",
            "BLACK_WHITE",
            "PATTERN8x8",
            "REAL_PIC",
        ]
        .into_iter()
        .enumerate()
        {
            let fill = [
                mode as u8,
                (-35i8) as u8,
                42,
                effect as u8,
                7,
                0,
                0,
                0,
                0,
                0,
            ];
            let image = shape(0, 2, &fill).image_fill.unwrap();
            assert_eq!(
                (
                    image.mode.as_str(),
                    image.binary_id.as_str(),
                    image.bright,
                    image.contrast,
                    image.effect.as_str()
                ),
                (expected, "bin7", -35, 42, name)
            );
            assert_eq!(serde_json::to_value(image).unwrap()["binaryID"], "bin7");
        }
    }
    assert!(shape(0, 2, &[5, 0, 0, 0, 0, 0]).image_fill.is_none());
    for (background, pattern, expected) in [
        (0x00332211u32, 0i32, Some(0x112233)),
        (0x00332211, -1, Some(0x112233)),
        (0x00332211, 1, None),
        (0xFF332211, 0, None),
    ] {
        let mut fill = Vec::new();
        fill.extend(background.to_le_bytes());
        fill.extend(0u32.to_le_bytes());
        fill.extend(pattern.to_le_bytes());
        fill.extend(gradient(1, 2));
        fill.extend([5, 221, 42, 1, 7, 0]);
        fill.extend(1u32.to_le_bytes());
        fill.extend([65, 51]);
        let value = shape(0, 7, &fill);
        assert_eq!(value.fill_color, expected);
        assert_eq!(value.fill_alpha, if expected.is_some() { 0.8 } else { 1.0 });
        assert_eq!(value.gradient.unwrap().step_center, 65);
        assert_eq!(value.image_fill.unwrap().binary_id, "bin7");
    }
}

#[test]
fn curve_segment_types() {
    let mut data = 4i32.to_le_bytes().to_vec();
    for value in [0i32, 0, 10, 0, 10, 10, 20, 10] {
        data.extend(value.to_le_bytes());
    }
    data.extend([1, 0, 255, 9, 9, 9, 9]);
    let value = component(&[], record(83, data.clone(), vec![]), false);
    assert_eq!(value.kind, "curve");
    assert_eq!(
        value.points,
        [[10.0, 20.0], [30.0, 20.0], [30.0, 50.0], [50.0, 50.0]]
    );
    assert_eq!(value.segment_types, ["CURVE", "LINE", "LINE"]);
    assert!(component(&[], record(82, data, vec![]), false)
        .segment_types
        .is_empty());
    for count in [0i32, 1, -1, i32::MAX] {
        let value = component(&[], record(83, count.to_le_bytes().to_vec(), vec![]), false);
        assert!(value.points.is_empty() && value.segment_types.is_empty());
    }
}

#[test]
fn arc_geometry() {
    for (kind, expected) in [(0, "NORMAL"), (1, "PIE"), (2, "CHORD"), (255, "NORMAL")] {
        let mut data = vec![kind];
        for value in [50i32, 25, 100, 25, 50, 50] {
            data.extend(value.to_le_bytes());
        }
        let value = component(&[], record(81, data.clone(), vec![]), false);
        assert_eq!(value.kind, "arc");
        assert_eq!(
            value.arc,
            Some(Arc {
                kind: expected.into(),
                center: [110.0, 95.0],
                axis1: [210.0, 95.0],
                axis2: [110.0, 170.0]
            })
        );
        let grouped = component(&[], record(81, data.clone(), vec![]), true);
        let RunItem::Shape(child) = grouped.children[0].item.as_ref() else {
            panic!("arc child")
        };
        assert_eq!(child.arc.as_ref().unwrap().center, [100.0, 75.0]);
        for length in 0..25 {
            let value = component(&[], record(81, data[..length].to_vec(), vec![]), false);
            assert_eq!(value.kind, "arc");
            assert!(value.arc.is_none());
        }
    }
}
