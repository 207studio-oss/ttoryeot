use std::collections::BTreeMap;

use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;

const MAX_XML_DEPTH: usize = 256;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Node {
    pub name: String,
    pub attrs: BTreeMap<String, String>,
    pub children: Vec<Node>,
    pub text: String,
    pub(crate) depth_limited: bool,
    /// 문단(`p`) 요소면 이 XML에서 문단 요소가 나오는 차례(0부터, 중첩된 문단도 센다). 깊이 제한으로 건너뛴 곳
    /// 뒤에서는 셀 수 없어 `None`이다. 글 고치기([`super::edit`])가 같은 차례로 문단을 찾는다.
    pub(crate) ordinal: Option<u32>,
}

impl Node {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            attrs: BTreeMap::new(),
            children: Vec::new(),
            text: String::new(),
            depth_limited: false,
            ordinal: None,
        }
    }

    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attrs.get(name).map(String::as_str)
    }

    pub fn int(&self, name: &str) -> Option<i64> {
        self.attr(name)?.trim().parse::<i64>().ok()
    }

    /// 한/글은 부호 있는 32비트 값(음수 오프셋·여백 등)을 부호 없이 적는다. 2^31 이상 2^32 미만 정수는 음수로 되돌린다.
    pub fn num(&self, name: &str) -> Option<f64> {
        let value = self.attr(name)?.trim();
        match value.parse::<u32>() {
            Ok(unsigned) => Some(f64::from(unsigned as i32)),
            Err(_) => value.parse::<f64>().ok(),
        }
    }

    pub fn bool(&self, name: &str) -> bool {
        matches!(
            self.attr(name).map(str::to_ascii_lowercase).as_deref(),
            Some("1") | Some("true")
        )
    }

    pub fn first(&self, name: &str) -> Option<&Node> {
        self.children.iter().find(|child| child.name == name)
    }

    pub fn all<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Node> + 'a {
        self.children.iter().filter(move |child| child.name == name)
    }

    pub fn find(&self, name: &str) -> Option<&Node> {
        for child in &self.children {
            if child.name == name {
                return Some(child);
            }
            if let Some(found) = child.find(name) {
                return Some(found);
            }
        }
        None
    }

    pub fn find_all<'a>(&'a self, name: &str, out: &mut Vec<&'a Node>) {
        for child in &self.children {
            if child.name == name {
                out.push(child);
            }
            child.find_all(name, out);
        }
    }

    pub fn all_text(&self) -> String {
        if self.name == "#text" {
            return self.text.clone();
        }
        let mut text = String::new();
        for child in &self.children {
            text.push_str(&child.all_text());
        }
        text
    }

    fn append_text(&mut self, value: &str) {
        if value.is_empty() {
            return;
        }
        if let Some(last) = self.children.last_mut() {
            if last.name == "#text" {
                last.text.push_str(value);
                return;
            }
        }
        let mut node = Node::new("#text");
        node.text.push_str(value);
        self.children.push(node);
    }
}

pub fn parse(data: &[u8]) -> Option<Node> {
    read(data, None)
}

/// 뿌리의 자식을 다 읽는 대로 하나씩 넘기고 버려, 문서 전체 트리를 메모리에 두지 않는다.
/// 돌려주는 값은 깊이 제한에 걸렸는지이다. XML이 잘못됐으면 `None`이고, 이미 넘긴 자식은 무를 수 없다.
pub fn for_each_root_child(data: &[u8], on_child: &mut dyn FnMut(&Node)) -> Option<bool> {
    read(data, Some(on_child)).map(|root| root.depth_limited)
}

fn read(data: &[u8], mut on_child: Option<&mut dyn FnMut(&Node)>) -> Option<Node> {
    let mut reader = Reader::from_reader(data);
    reader.config_mut().trim_text(false);
    let mut stack: Vec<Node> = Vec::new();
    let mut root: Option<Node> = None;
    let mut buffer = Vec::new();
    let mut skipped = Vec::new();
    let mut depth_limited = false;
    // 문단 요소의 차례. 깊이 제한으로 건너뛰면 그 안의 문단을 셀 수 없어 그 뒤로는 적지 않는다.
    let mut paragraphs: u32 = 0;
    let mut number = |node: &mut Node, depth_limited: bool| {
        if node.name == "p" {
            node.ordinal = (!depth_limited).then_some(paragraphs);
            paragraphs = paragraphs.saturating_add(1);
        }
    };

    loop {
        match reader.read_event_into(&mut buffer) {
            Ok(Event::Start(element)) => {
                if stack.len() >= MAX_XML_DEPTH {
                    reader.read_to_end_into(element.name(), &mut skipped).ok()?;
                    depth_limited = true;
                } else {
                    let mut node = element_node(&element)?;
                    number(&mut node, depth_limited);
                    stack.push(node);
                }
            }
            Ok(Event::Empty(element)) => {
                if stack.len() >= MAX_XML_DEPTH {
                    depth_limited = true;
                } else {
                    let mut node = element_node(&element)?;
                    number(&mut node, depth_limited);
                    attach(node, &mut stack, &mut root, &mut on_child)?;
                }
            }
            Ok(Event::End(_)) => {
                let node = stack.pop()?;
                attach(node, &mut stack, &mut root, &mut on_child)?;
            }
            Ok(Event::Text(text)) => {
                let content = text.xml10_content();
                let decoded = quick_xml::escape::unescape(&content).ok()?;
                if let Some(parent) = stack.last_mut() {
                    parent.append_text(&decoded);
                }
            }
            Ok(Event::CData(text)) => {
                if let Some(parent) = stack.last_mut() {
                    parent.append_text(&text.xml10_content());
                }
            }
            Ok(Event::GeneralRef(reference)) => {
                let name = reference.into_inner();
                let value = resolve_reference(&name)?;
                if let Some(parent) = stack.last_mut() {
                    parent.append_text(&value);
                }
            }
            Ok(Event::Eof) => break,
            Ok(Event::Decl(_))
            | Ok(Event::Comment(_))
            | Ok(Event::PI(_))
            | Ok(Event::DocType(_)) => {}
            Err(_) => return None,
        }
        buffer.clear();
    }
    if stack.is_empty() {
        let mut root = root?;
        root.depth_limited = depth_limited;
        Some(root)
    } else {
        None
    }
}

fn element_node(element: &BytesStart<'_>) -> Option<Node> {
    let name = std::str::from_utf8(element.name().local_name().into_inner().as_bytes())
        .ok()?
        .to_string();
    let mut node = Node::new(name);
    for attribute in element.attributes() {
        let attribute = attribute.ok()?;
        let key = std::str::from_utf8(attribute.key.local_name().into_inner().as_bytes())
            .ok()?
            .to_string();
        let value = attribute
            .normalized_value(quick_xml::XmlVersion::Implicit1_0)
            .ok()?
            .into_owned();
        node.attrs.insert(key, value);
    }
    Some(node)
}

fn attach(
    node: Node,
    stack: &mut [Node],
    root: &mut Option<Node>,
    on_child: &mut Option<&mut dyn FnMut(&Node)>,
) -> Option<()> {
    if let ([_], Some(on_child)) = (&*stack, on_child) {
        on_child(&node);
    } else if let Some(parent) = stack.last_mut() {
        parent.children.push(node);
    } else {
        if root.is_some() {
            return None;
        }
        *root = Some(node);
    }
    Some(())
}

pub(super) fn resolve_reference(name: &str) -> Option<String> {
    if let Some(digits) = name.strip_prefix("#x") {
        let value = u32::from_str_radix(digits, 16).ok()?;
        return char::from_u32(value).map(|character| character.to_string());
    }
    if let Some(digits) = name.strip_prefix('#') {
        let value = digits.parse::<u32>().ok()?;
        return char::from_u32(value).map(|character| character.to_string());
    }
    let value = match name {
        "amp" => "&",
        "lt" => "<",
        "gt" => ">",
        "apos" => "'",
        "quot" => "\"",
        _ => return None,
    };
    Some(value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_children_arrive_one_by_one_like_the_full_tree() {
        let data = b"<r><a x=\"1\"><b/></a> <c/></r>";
        let mut seen = Vec::new();
        let depth_limited = for_each_root_child(data, &mut |node| seen.push(node.clone()));
        assert_eq!(depth_limited, Some(false));
        let full = parse(data).expect("xml");
        let elements = full.children.iter().filter(|node| node.name != "#text");
        assert_eq!(seen, elements.cloned().collect::<Vec<_>>());
    }

    #[test]
    fn local_names_and_mixed_text() {
        let node = parse(b"<a:root><b:t>x<c:tab/> y</b:t></a:root>").expect("xml");
        assert_eq!(node.name, "root");
        let text = node.find("t").expect("t");
        assert_eq!(text.children.len(), 3);
        assert_eq!(text.children[0].text, "x");
        assert_eq!(text.children[1].name, "tab");
        assert_eq!(text.children[2].text, " y");
    }

    #[test]
    fn first_descendant_wins() {
        let node = parse(b"<r><switch><case><margin>x</margin></case><default><margin>y</margin></default></switch></r>")
            .expect("xml");
        assert_eq!(node.find("margin").expect("margin").all_text(), "x");
    }

    #[test]
    fn depth_limit_preserves_parent_text_and_following_siblings() {
        let mut xml = "<root>".to_string();
        xml.push_str(&"<n>".repeat(MAX_XML_DEPTH - 1));
        xml.push_str("before<skip><n>omitted</n></skip><empty/>after");
        xml.push_str(&"</n>".repeat(MAX_XML_DEPTH - 1));
        xml.push_str("<sibling>last</sibling></root>");
        let root = parse(xml.as_bytes()).expect("partial XML tree");
        assert_eq!(root.all_text(), "beforeafterlast");
        assert!(root.depth_limited);
        assert!(root.find("skip").is_none());
        assert!(root.find("empty").is_none());
        assert_eq!(root.first("sibling").expect("sibling").all_text(), "last");
    }

    #[test]
    fn content_at_depth_limit_is_preserved() {
        let xml = format!(
            "{}text{}",
            "<n>".repeat(MAX_XML_DEPTH),
            "</n>".repeat(MAX_XML_DEPTH)
        );
        let root = parse(xml.as_bytes()).expect("complete XML tree");
        assert_eq!(root.all_text(), "text");
        assert!(!root.depth_limited);
    }

    #[test]
    fn unterminated_skipped_subtree_is_invalid_xml() {
        let xml = format!("{}<skip>", "<n>".repeat(MAX_XML_DEPTH));
        assert!(parse(xml.as_bytes()).is_none());
    }
}
