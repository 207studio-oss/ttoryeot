use hwp_core::probe::{shape_fill_distribution, ShapeFillDistribution};
use hwp_core::*;

fn paragraphs(item: RunItem) -> Vec<Paragraph> {
    vec![Paragraph {
        runs: vec![Run {
            items: vec![item],
            ..Run::default()
        }],
        ..Paragraph::default()
    }]
}

#[test]
fn fill_distribution_visits_every_shape_location_and_combined_fills() {
    let filled = RunItem::Shape(Shape {
        fill_color: Some(0x112233),
        gradient: Some(Gradient {
            kind: "LINEAR".into(),
            angle: 0.0,
            center_x: 50.0,
            center_y: 50.0,
            step: 0,
            step_center: 50,
            colors: vec![0, 0xFFFFFF],
            positions: vec![],
        }),
        image_fill: Some(ImageFill {
            mode: "TOTAL".into(),
            binary_id: "bin1".into(),
            bright: 0,
            contrast: 0,
            effect: "REAL_PIC".into(),
        }),
        ..Shape::default()
    });
    let nested = paragraphs(filled.clone());
    let caption = Some(Caption {
        paragraphs: nested.clone(),
        ..Caption::default()
    });
    let items = vec![
        RunItem::Shape(Shape {
            kind: "container".into(),
            paragraphs: nested.clone(),
            caption: caption.clone(),
            children: vec![ShapeChild {
                item: Box::new(filled.clone()),
                frame: [[0.0; 2]; 2],
                transform: None,
            }],
            ..Shape::default()
        }),
        RunItem::Table(Table {
            cells: vec![Cell {
                paragraphs: nested.clone(),
                ..Cell::default()
            }],
            caption: caption.clone(),
            ..Table::default()
        }),
        RunItem::Picture(Picture {
            caption,
            ..Picture::default()
        }),
        RunItem::Note(Note {
            paragraphs: nested.clone(),
            ..Note::default()
        }),
        RunItem::Header {
            paragraphs: nested.clone(),
            applies: "BOTH".into(),
        },
        RunItem::Footer {
            paragraphs: nested.clone(),
            applies: "BOTH".into(),
        },
        RunItem::Text("synthetic".into()),
    ];
    let document = Document {
        sections: vec![Section {
            paragraphs: vec![Paragraph {
                runs: vec![Run {
                    items,
                    ..Run::default()
                }],
                memos: vec![Memo {
                    paragraphs: nested.clone(),
                    ..Memo::default()
                }],
                ..Paragraph::default()
            }],
            header: nested.clone(),
            footer: nested,
            ..Section::default()
        }],
        ..Document::default()
    };
    assert_eq!(
        shape_fill_distribution(&document),
        ShapeFillDistribution {
            shapes: 13,
            solid: 12,
            gradient: 12,
            image: 12,
            unfilled: 1,
        }
    );
}
