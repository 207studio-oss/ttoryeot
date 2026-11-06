#[path = "support/mod.rs"]
mod support;

use hwp_core::hwpx::zip::MiniZip;
use hwp_core::model::{BlobKind, RunItem};
use hwp_core::{parse, Fatal, ParseOptions, WarningKind};

use support::zipw::{self, Entry};

fn build_hwpx(header: &str, section: &str, extra: &[(&str, &[u8])]) -> Vec<u8> {
    let content = br#"<?xml version="1.0" encoding="UTF-8"?><opf:package xmlns:opf="http://www.idpf.org/2007/opf/"><opf:manifest><opf:item id="section0" href="Contents/section0.xml"/></opf:manifest></opf:package>"#;
    build_hwpx_with_content(content, header, section, extra)
}

fn build_hwpx_with_content(
    content: &[u8],
    header: &str,
    section: &str,
    extra: &[(&str, &[u8])],
) -> Vec<u8> {
    let mut entries = vec![
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
    ];
    for (name, data) in extra {
        entries.push(Entry {
            name,
            data,
            deflate: true,
        });
    }
    zipw::build(&entries)
}

fn parse_document(bytes: &[u8]) -> hwp_core::model::Document {
    parse(bytes, &ParseOptions::default())
        .expect("parse hwpx")
        .document
}

#[test]
fn zip_stored_and_deflate() {
    let stored = b"stored bytes";
    let deflated = b"deflated bytes repeated repeated repeated";
    let bytes = zipw::build(&[
        Entry {
            name: "stored.txt",
            data: stored,
            deflate: false,
        },
        Entry {
            name: "deflated.txt",
            data: deflated,
            deflate: true,
        },
    ]);
    let zip = MiniZip::new(&bytes).expect("zip");
    assert_eq!(zip.entries().len(), 2);
    assert_eq!(zip.read("stored.txt").as_deref(), Some(stored.as_slice()));
    assert_eq!(
        zip.read("deflated.txt").as_deref(),
        Some(deflated.as_slice())
    );
    assert_eq!(zip.read("missing.txt"), None);
}

#[test]
fn zip_bad_central_dir_is_not_hwp() {
    let mut bytes = build_hwpx(
        r#"<hh:head xmlns:hh="urn:hh" secCnt="1"/>"#,
        r#"<hs:sec xmlns:hs="urn:hs"/>"#,
        &[],
    );
    let eocd = bytes.len() - 22;
    let directory =
        u32::from_le_bytes(bytes[eocd + 16..eocd + 20].try_into().expect("offset")) as usize;
    bytes[directory..directory + 4].copy_from_slice(&[0, 0, 0, 0]);
    assert_eq!(parse(&bytes, &ParseOptions::default()), Err(Fatal::NotHwp));
    assert_eq!(
        parse(b"PK\x03\x04not a zip", &ParseOptions::default()),
        Err(Fatal::NotHwp)
    );
}

#[test]
fn prefix_agnostic_names() {
    let header = r#"<?xml version="1.0" encoding="UTF-8"?>
<zz:head xmlns:zz="urn:zz" secCnt="1">
  <zz:fontfaces>
    <zz:fontface lang="HANGUL"><zz:font id="0" face="Body" type="TTF"/></zz:fontface>
  </zz:fontfaces>
  <zz:charProperties><zz:charPr id="3" height="1000"/></zz:charProperties>
</zz:head>"#;
    let section = r#"<?xml version="1.0" encoding="UTF-8"?>
<qq:sec xmlns:qq="urn:qq"><qq:p paraPrIDRef="7" styleIDRef="2"><qq:run charPrIDRef="3"><qq:t>  앞  뒤  </qq:t></qq:run></qq:p></qq:sec>"#;
    let document = parse_document(&build_hwpx(header, section, &[]));
    assert_eq!(document.sections.len(), 1);
    let paragraph = &document.sections[0].paragraphs[0];
    assert_eq!(paragraph.para_pr_id, 7);
    assert_eq!(paragraph.style_id, 2);
    assert_eq!(paragraph.runs[0].char_pr_id, 3);
    assert_eq!(
        paragraph.runs[0].items,
        vec![RunItem::Text("  앞  뒤  ".to_string())]
    );
    assert_eq!(document.font_faces["hangul"][0].name, "Body");
}

#[test]
fn encrypted_manifest_is_password_protected() {
    let manifest = br#"<?xml version="1.0" encoding="UTF-8"?>
<odf:manifest xmlns:odf="urn:odf"><odf:file-entry full-path="Contents/header.xml"><odf:encryption-data checksum-type="http://www.w3.org/2000/09/xmldsig#sha256-1k" checksum="AAAA"><odf:algorithm algorithm-name="http://www.w3.org/2001/04/xmlenc#aes256-cbc" initialisation-vector="AAAAAAAAAAAAAAAAAAAAAA=="/><odf:key-derivation key-derivation-name="http://www.w3.org/2001/04/xmlenc#pbkdf2" iteration-count="1024" salt="AAAAAAAAAAAAAAAAAAAAAA==" key-size="32"><odf:start-key-generation start-key-generation-name="http://www.w3.org/2000/09/xmldsig#sha256"/></odf:key-derivation></odf:encryption-data></odf:file-entry></odf:manifest>"#;
    let bytes = build_hwpx(
        r#"<hh:head xmlns:hh="urn:hh" secCnt="1"/>"#,
        r#"<hs:sec xmlns:hs="urn:hs"/>"#,
        &[("META-INF/manifest.xml", manifest)],
    );
    assert_eq!(
        parse(&bytes, &ParseOptions::default()),
        Err(Fatal::PasswordProtected)
    );
}

#[test]
fn embedded_font_bin_data_is_a_font_blob() {
    let header = r#"<?xml version="1.0" encoding="UTF-8"?>
<hh:head xmlns:hh="urn:hh" secCnt="1">
  <hh:fontfaces>
    <hh:fontface lang="HANGUL">
      <hh:font id="0" face="Embedded Face" type="TTF" isEmbedded="1" binaryItemIDRef="bin1"/>
    </hh:fontface>
  </hh:fontfaces>
  <hh:charProperties><hh:charPr id="0" height="1000"><hh:fontRef hangul="0"/></hh:charPr></hh:charProperties>
</hh:head>"#;
    let content = br#"<?xml version="1.0" encoding="UTF-8"?>
<opf:package xmlns:opf="http://www.idpf.org/2007/opf/"><opf:manifest>
  <opf:item id="section0" href="Contents/section0.xml"/>
  <opf:item id="bin1" href="BinData/font.ttf" media-type="application/x-font-ttf"/>
</opf:manifest></opf:package>"#;
    let bytes = build_hwpx_with_content(
        content,
        header,
        r#"<hs:sec xmlns:hs="urn:hs"/>"#,
        &[("BinData/font.ttf", b"font-bytes")],
    );
    let document = parse_document(&bytes);
    let font = document
        .bins
        .iter()
        .find(|blob| blob.id == "bin1")
        .expect("font blob");
    assert_eq!(font.kind, BlobKind::Font);
    assert_eq!(font.data.as_slice(), b"font-bytes");
    assert_eq!(
        document.bins.len(),
        1,
        "the manifest item and the ZIP entry must not both add the same BinData"
    );
}

#[test]
fn non_embedded_font_ref_is_not_a_font_blob() {
    let header = r#"<?xml version="1.0" encoding="UTF-8"?>
<hh:head xmlns:hh="urn:hh" secCnt="1">
  <hh:fontfaces>
    <hh:fontface lang="HANGUL">
      <hh:font id="0" face="Not Embedded" type="TTF" isEmbedded="0" binaryItemIDRef="bin1"/>
    </hh:fontface>
  </hh:fontfaces>
  <hh:charProperties><hh:charPr id="0" height="1000"><hh:fontRef hangul="0"/></hh:charPr></hh:charProperties>
</hh:head>"#;
    let content = br#"<?xml version="1.0" encoding="UTF-8"?>
<opf:package xmlns:opf="http://www.idpf.org/2007/opf/"><opf:manifest>
  <opf:item id="section0" href="Contents/section0.xml"/>
  <opf:item id="bin1" href="BinData/font.ttf" media-type="application/x-font-ttf"/>
</opf:manifest></opf:package>"#;
    let bytes = build_hwpx_with_content(
        content,
        header,
        r#"<hs:sec xmlns:hs="urn:hs"/>"#,
        &[("BinData/font.ttf", b"font-bytes")],
    );
    let document = parse_document(&bytes);
    let blob = document
        .bins
        .iter()
        .find(|blob| blob.id == "bin1")
        .expect("blob");
    assert_eq!(
        blob.kind,
        BlobKind::Picture,
        "isEmbedded=\"0\" must not make binaryItemIDRef a font"
    );
}

#[test]
fn non_embedded_zip_only_font_is_not_a_font_blob() {
    let header = r#"<?xml version="1.0" encoding="UTF-8"?>
<hh:head xmlns:hh="urn:hh" secCnt="1">
  <hh:fontfaces>
    <hh:fontface lang="HANGUL">
      <hh:font id="0" face="Not Embedded" type="TTF" isEmbedded="0" binaryItemIDRef="bin1"/>
    </hh:fontface>
  </hh:fontfaces>
  <hh:charProperties><hh:charPr id="0" height="1000"><hh:fontRef hangul="0"/></hh:charPr></hh:charProperties>
</hh:head>"#;
    // manifest에 bin1이 없고 ZIP에만 BinData/font.ttf가 있는 경우.
    let bytes = build_hwpx(
        header,
        r#"<hs:sec xmlns:hs="urn:hs"/>"#,
        &[("BinData/font.ttf", b"font-bytes")],
    );
    let document = parse_document(&bytes);
    assert!(
        document.bins.iter().all(|blob| blob.kind != BlobKind::Font),
        "isEmbedded=\"0\" must not turn an unreferenced ZIP .ttf into a font"
    );
}

#[test]
fn manifest_media_type_marks_an_unreferenced_bin_data_as_a_font() {
    let header = r#"<?xml version="1.0" encoding="UTF-8"?>
<hh:head xmlns:hh="urn:hh" secCnt="1"/>"#;
    let content = br#"<?xml version="1.0" encoding="UTF-8"?>
<opf:package xmlns:opf="http://www.idpf.org/2007/opf/"><opf:manifest>
  <opf:item id="section0" href="Contents/section0.xml"/>
  <opf:item id="font1" href="BinData/font.dat" media-type="application/x-font-ttf"/>
</opf:manifest></opf:package>"#;
    let bytes = build_hwpx_with_content(
        content,
        header,
        r#"<hs:sec xmlns:hs="urn:hs"/>"#,
        &[("BinData/font.dat", b"font-bytes")],
    );
    let document = parse_document(&bytes);
    let blob = document
        .bins
        .iter()
        .find(|blob| blob.id == "font1")
        .expect("font blob");
    assert_eq!(blob.kind, BlobKind::Font);
    assert_eq!(blob.data.as_slice(), b"font-bytes");
}

#[test]
fn header_tables_parity_fixture() {
    let header = r##"<?xml version="1.0" encoding="UTF-8"?>
<hh:head xmlns:hh="urn:hh" xmlns:hc="urn:hc" secCnt="1">
  <hh:fontfaces>
    <hh:fontface lang="HANGUL" fontCnt="1">
      <hh:font id="2" face="Test Face" type="TTF" isEmbedded="1" binaryItemIDRef="bin1">
        <hh:substFont face="Alt Face" type="HFT"/>
      </hh:font>
    </hh:fontface>
  </hh:fontfaces>
  <hh:borderFills>
    <hh:borderFill id="5">
      <hh:leftBorder type="SOLID" width="0.12 mm" color="#112233"/>
      <hh:rightBorder type="DASH" width="0.5 mm" color="#445566"/>
      <hh:topBorder type="NONE"/>
      <hh:bottomBorder type="DOT" width="0.15 mm" color="#778899"/>
      <hh:diagonal type="SOLID" width="0.2 mm" color="#AABBCC"/>
      <hh:slash type="SLASH"/><hh:backSlash type="BACK"/>
      <hc:winBrush faceColor="#010203"/>
      <hc:gradation><hc:color value="#102030"/><hc:color value="#405060"/></hc:gradation>
    </hh:borderFill>
  </hh:borderFills>
  <hh:charProperties>
    <hh:charPr id="1" height="1200" textColor="#AABBCC" shadeColor="none" useFontSpace="1" useKerning="true">
      <hh:fontRef hangul="2" latin="4"/>
      <hh:ratio hangul="90"/><hh:spacing hangul="-5"/><hh:relSz hangul="110"/><hh:offset hangul="3"/>
      <hh:bold/><hh:italic/><hh:underline type="BOTTOM" color="#223344"/>
      <hh:strikeout shape="3D" type="SOLID"/>
    </hh:charPr>
    <hh:charPr id="2" height="900" textColor="#000000">
      <hh:strikeout shape="SOLID" type="SOLID"/><hh:supscript/>
    </hh:charPr>
  </hh:charProperties>
  <hh:tabProperties>
    <hh:tabPr id="3" autoTabLeft="1" autoTabRight="true">
      <hh:tabItem pos="1000" type="RIGHT" leader="DOT"/>
      <hh:tabItem pos="-5" type="LEFT" leader="NONE"/>
      <hh:tabItem pos="2000" type="2" leader="1"/>
    </hh:tabPr>
  </hh:tabProperties>
  <hh:paraProperties>
    <hh:paraPr id="7" tabPrIDRef="3">
      <hh:align horizontal="CENTER" vertical="TOP"/>
      <hh:heading type="OUTLINE" idRef="4" level="2"/>
      <hh:breakSetting breakLatinWord="BREAK_WORD" breakNonLatinWord="KEEP_WORD" keepWithNext="1" pageBreakBefore="true"/>
      <hh:border borderFillIDRef="5"/>
      <hh:switch>
        <hh:case hp:required-namespace="http://www.hancom.co.kr/hwpml/2016/HwpUnitChar"><hh:margin><hc:intent value="10" unit="MM"/><hc:left value="20"/><hc:right value="30"/><hc:prev value="40"/><hc:next value="50"/></hh:margin></hh:case>
        <hh:default><hh:margin><hc:intent value="99"/></hh:margin></hh:default>
      </hh:switch>
      <hh:lineSpacing type="PERCENT" value="130"/>
    </hh:paraPr>
  </hh:paraProperties>
  <hh:numberings>
    <hh:numbering id="2" start="1">
      <hh:paraHead level="1" numFormat="HANGUL_SYLLABLE" align="CENTER" start="1" charPrIDRef="4294967295" autoIndent="1" widthAdjust="30" textOffsetType="HWPUNIT" textOffset="40">^1.</hh:paraHead>
      <hh:paraHead level="2" numFormat="DIGIT" start="2" charPrIDRef="5"/>
    </hh:numbering>
  </hh:numberings>
  <hh:bullets>
    <hh:bullet id="4" char="★"><hh:paraHead charPrIDRef="4294967295" autoIndent="true" useInstWidth="1" textOffsetPercent="HWPUNIT" textOffset="70"/></hh:bullet>
  </hh:bullets>
  <hh:styles><hh:style id="9" engName="Page Number" paraPrIDRef="7" charPrIDRef="1"/></hh:styles>
</hh:head>"##;
    let document = parse_document(&build_hwpx(header, r#"<hs:sec xmlns:hs="urn:hs"/>"#, &[]));

    let font = &document.font_faces["hangul"][0];
    assert_eq!(font.id, 2);
    assert_eq!(font.name, "Test Face");
    assert_eq!(font.face_type, 1);
    assert_eq!(font.alt_name.as_deref(), Some("Alt Face"));
    assert_eq!(font.alt_type, 2);
    assert!(font.embedded);
    assert_eq!(font.binary_ref.as_deref(), Some("bin1"));

    let fill = &document.border_fills[0];
    assert_eq!(fill.id, 5);
    assert_eq!(fill.left.border_type, "SOLID");
    assert_eq!(fill.left.width, 0.12);
    assert_eq!(fill.left.color, 0x112233);
    assert_eq!(fill.right.border_type, "DASH");
    assert_eq!(fill.top.border_type, "NONE");
    assert_eq!(fill.diagonal.width, 0.2);
    assert_eq!(fill.slash, "SLASH");
    assert_eq!(fill.back_slash, "BACK");
    assert_eq!(fill.fill_color, Some(0x010203));
    assert_eq!(fill.gradient, Some(vec![(0x102030, 0.0), (0x405060, 0.0)]));

    let character = &document.char_prs[0];
    assert_eq!(character.id, 1);
    assert_eq!(character.height, 1200.0);
    assert_eq!(character.text_color, 0xAABBCC);
    assert_eq!(character.shade_color, None);
    assert!(character.use_font_space);
    assert!(character.kerning);
    assert_eq!(character.font_ref["hangul"], 2);
    assert_eq!(character.font_ref["latin"], 4);
    assert_eq!(character.ratio["hangul"], 90.0);
    assert_eq!(character.spacing["hangul"], -5.0);
    assert_eq!(character.rel_sz["hangul"], 110.0);
    assert_eq!(character.offset["hangul"], 3.0);
    assert!(character.bold && character.italic);
    assert_eq!(character.underline, "BOTTOM");
    assert_eq!(character.underline_color, 0x223344);
    assert!(!character.strikeout);
    assert!(document.char_prs[1].strikeout);
    assert!(document.char_prs[1].supscript);

    let tabs = &document.tab_prs[0];
    assert_eq!(tabs.id, 3);
    assert!(tabs.auto_left && tabs.auto_right);
    assert_eq!(tabs.stops.len(), 2);
    assert_eq!(tabs.stops[0].position, 500.0);
    assert_eq!(tabs.stops[0].kind, "RIGHT");
    assert_eq!(tabs.stops[0].leader, "DOT");
    assert_eq!(tabs.stops[1].kind, "RIGHT");
    assert_eq!(tabs.stops[1].leader, "SOLID");

    let paragraph = &document.para_prs[0];
    assert_eq!(paragraph.id, 7);
    assert_eq!(paragraph.align, "CENTER");
    assert_eq!(paragraph.vert_align, "TOP");
    assert_eq!(paragraph.heading_type, "OUTLINE");
    assert_eq!(paragraph.heading_id_ref, 4);
    assert_eq!(paragraph.heading_level, 2);
    assert_eq!(paragraph.tab_pr_id_ref, 3);
    assert_eq!(paragraph.border_fill_id_ref, 5);
    assert_eq!(paragraph.break_latin_word, "BREAK_WORD");
    assert_eq!(paragraph.break_non_latin_word, "KEEP_WORD");
    assert!(paragraph.keep_with_next && paragraph.page_break_before);
    assert_eq!(paragraph.indent, 10.0 * 7200.0 / 25.4);
    assert_eq!(paragraph.margin_left, 20.0);
    assert_eq!(paragraph.margin_right, 30.0);
    assert_eq!(paragraph.prev, 40.0);
    assert_eq!(paragraph.next, 50.0);
    assert_eq!(paragraph.line_spacing_type, "PERCENT");
    assert_eq!(paragraph.line_spacing, 130.0);

    let numbering = &document.numberings[0];
    assert_eq!(numbering.id, 2);
    assert_eq!(numbering.heads[0].text, "^1.");
    assert_eq!(numbering.heads[0].format, "HANGUL_SYLLABLE");
    assert_eq!(numbering.heads[0].char_pr_id, None);
    assert_eq!(numbering.heads[1].char_pr_id, Some(5));
    assert_eq!(numbering.heads[0].para.width_adjust, 30.0);
    assert!(!numbering.heads[0].para.text_offset_percent);
    assert_eq!(numbering.heads[0].para.text_offset, 40.0);

    let bullet = &document.bullets[0];
    assert_eq!(bullet.id, 4);
    assert_eq!(bullet.char, "★");
    assert_eq!(bullet.char_pr_id, None);
    assert!(bullet.para.auto_indent && bullet.para.use_inst_width);
    assert_eq!(bullet.para.text_offset, 70.0);

    let style = &document.styles[0];
    assert_eq!(style.id, 9);
    assert_eq!(style.para_pr_id, 7);
    assert_eq!(style.char_pr_id, 1);
    assert_eq!(style.eng_name, "Page Number");
}

/// 한/글이 저장한 HWPX는 tabItem마다 switch를 두고 case에 HWPUNIT 값, default에 그 두 배를 적는다.
/// 개요 8~10 수준 heading도 case에만 OUTLINE으로 있다.
#[test]
fn switched_items_read_case_branch() {
    let header = r#"<?xml version="1.0" encoding="UTF-8"?>
<hh:head xmlns:hh="urn:hh" xmlns:hp="urn:hp" secCnt="1">
  <hh:tabProperties>
    <hh:tabPr id="1" autoTabLeft="0" autoTabRight="0">
      <hp:switch>
        <hp:case hp:required-namespace="http://www.hancom.co.kr/hwpml/2016/HwpUnitChar"><hh:tabItem pos="1500" type="LEFT" leader="NONE" unit="HWPUNIT"/></hp:case>
        <hp:default><hh:tabItem pos="3000" type="LEFT" leader="NONE"/></hp:default>
      </hp:switch>
      <hp:switch>
        <hp:case hp:required-namespace="http://www.hancom.co.kr/hwpml/2016/HwpUnitChar"><hh:tabItem pos="4000" type="RIGHT" leader="DOT" unit="HWPUNIT"/></hp:case>
        <hp:default><hh:tabItem pos="8000" type="RIGHT" leader="DOT"/></hp:default>
      </hp:switch>
    </hh:tabPr>
  </hh:tabProperties>
  <hh:paraProperties>
    <hh:paraPr id="2" tabPrIDRef="1">
      <hp:switch>
        <hp:case hp:required-namespace="http://www.hancom.co.kr/hwpml/2016/HwpUnitChar"><hh:heading type="OUTLINE" idRef="1" level="7"/></hp:case>
        <hp:default><hh:heading type="NONE" idRef="0" level="0"/></hp:default>
      </hp:switch>
    </hh:paraPr>
  </hh:paraProperties>
</hh:head>"#;
    let document = parse_document(&build_hwpx(header, r#"<hs:sec xmlns:hs="urn:hs"/>"#, &[]));
    let stops = &document.tab_prs[0].stops;
    assert_eq!(stops.len(), 2);
    assert_eq!(stops[0].position, 1500.0);
    assert_eq!(stops[1].position, 4000.0);
    assert_eq!(stops[1].kind, "RIGHT");
    let paragraph = &document.para_prs[0];
    assert_eq!(paragraph.heading_type, "OUTLINE");
    assert_eq!(paragraph.heading_level, 7);
}

#[test]
fn switches_use_supported_namespaces_and_fall_back_to_default() {
    let header = r#"<head><tabProperties><tabPr id="1">
      <tabItem pos="1000"/>
      <switch><case required-namespace="urn:future"><tabItem pos="2000"/></case><default><tabItem pos="3000"/></default></switch>
      <switch><case required-namespace="urn:future"><tabItem pos="5000"/></case><case required-namespace="http://www.hancom.co.kr/hwpml/2016/HwpUnitChar"><tabItem pos="4000"/></case></switch>
    </tabPr></tabProperties></head>"#;
    let document = parse_document(&build_hwpx(header, r#"<sec/>"#, &[]));
    assert_eq!(
        document.tab_prs[0]
            .stops
            .iter()
            .map(|stop| stop.position)
            .collect::<Vec<_>>(),
        [500.0, 1500.0, 4000.0]
    );
}

/// 한/글 2018 HWPX는 switch 없이 문단 여백을 HWP 5처럼 두 배로 적는다. HwpUnitChar case는 그대로다.
#[test]
fn bare_paragraph_margins_are_doubled_hwp_units() {
    let header = r#"<head><paraProperties>
      <paraPr id="0"><margin><intent value="-4074"/><left value="2000"/><right value="0"/><prev value="600"/><next value="0"/></margin></paraPr>
      <paraPr id="1"><switch><case required-namespace="http://www.hancom.co.kr/hwpml/2016/HwpUnitChar"><margin><intent value="-2744"/><left value="0"/></margin></case><default><margin><intent value="-5488"/><left value="0"/></margin></default></switch></paraPr>
    </paraProperties></head>"#;
    let document = parse_document(&build_hwpx(header, r#"<sec/>"#, &[]));
    let bare = &document.para_prs[0];
    assert_eq!(
        (bare.indent, bare.margin_left, bare.prev),
        (-2037.0, 1000.0, 300.0)
    );
    assert_eq!(document.para_prs[1].indent, -2744.0);
}

#[test]
fn common_fixture_header_tables() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/synthetic.hwpx");
    let bytes = std::fs::read(path).expect("read synthetic fixture");
    let parsed = parse(&bytes, &ParseOptions::default()).expect("parse synthetic fixture");
    assert!(parsed.warnings.is_empty());
    assert_eq!(parsed.document.sections.len(), 11);
    let fill = parsed
        .document
        .border_fills
        .iter()
        .find(|fill| fill.id == 1)
        .expect("border fill");
    assert_eq!(fill.fill_color, Some(0xE4F0E8));
    assert_eq!(fill.top.border_type, "SOLID");
    assert_eq!(fill.top.color, 0x1F5B4D);
    let width_pt = fill.top.width * 72.0 / 25.4;
    assert!((width_pt - 0.340_157_480_314_960_7).abs() <= 1e-9);
}

#[test]
fn unmatched_section_warns_and_keeps_prefix() {
    let bytes = build_hwpx(
        r#"<hh:head xmlns:hh="urn:hh" secCnt="1"/>"#,
        r#"<hs:sec xmlns:hs="urn:hs"><hs:p><hs:run><hs:t>x</hs:t></hs:run></hs:p></hs:sec>"#,
        &[],
    );
    let parsed = parse(&bytes, &ParseOptions::default()).expect("parse");
    assert!(parsed
        .warnings
        .iter()
        .all(|warning| warning.kind != WarningKind::InvalidXml));
    assert_eq!(
        parsed.document.sections[0].paragraphs[0].runs[0].items,
        vec![RunItem::Text("x".to_string())]
    );
}

#[test]
fn char_unit_margins_count_five_points_per_character() {
    let header = r#"<head><paraProperties><paraPr id="1">
      <switch><case required-namespace="http://www.hancom.co.kr/hwpml/2016/HwpUnitChar"><margin><intent value="300" unit="CHAR"/><left value="0" unit="HWPUNIT"/></margin></case>
      <default><margin><intent value="601" unit="HWPUNIT"/></margin></default></switch>
    </paraPr></paraProperties></head>"#;
    let document = parse_document(&build_hwpx(header, r#"<sec/>"#, &[]));
    assert_eq!(document.para_prs[0].indent, 1500.0);
}
