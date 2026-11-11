//! 수식 스크립트 읽기: 낱말, 글리프 사상, 구문 나무. Swift `HWPEquationScript.swift`를 옮겼다.

/// 기호의 간격 갈래.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Cls {
    Ord,
    Bin,
    Rel,
    Open,
    Close,
    Punct,
}

/// 그릴 글꼴 갈래: 수식 글꼴(또는 Times 로만), 수식 글꼴 기울임(또는 Times 기울임), 글꼴(한글).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum FaceKind {
    Math,
    Italic,
    Text,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) struct Face {
    pub(super) kind: FaceKind,
    pub(super) bold: bool,
}

impl Face {
    pub(super) const fn math() -> Self {
        Self {
            kind: FaceKind::Math,
            bold: false,
        }
    }

    pub(super) const fn italic() -> Self {
        Self {
            kind: FaceKind::Italic,
            bold: false,
        }
    }

    pub(super) const fn text() -> Self {
        Self {
            kind: FaceKind::Text,
            bold: false,
        }
    }

    pub(super) fn with_bold(self, bold: bool) -> Self {
        Self { bold, ..self }
    }
}

#[derive(Clone, PartialEq, Debug)]
pub(super) enum Token {
    Word(String),
    Number(String),
    Sym(char),
    Text(String),
    Quoted(String),
    Open,
    Close,
    Sup,
    Sub,
    Amp,
    Row,
    Space(f32),
}

#[derive(Clone, PartialEq, Debug)]
enum Key {
    Style(String),
    Structural(String),
    Symbol(String, Cls),
    Function(String),
    BigOp(String),
    Greek(usize, bool),
}

const STRUCTURAL: &[&str] = &[
    "over",
    "atop",
    "sqrt",
    "root",
    "of",
    "left",
    "right",
    "cases",
    "pile",
    "lpile",
    "rpile",
    "eqalign",
    "matrix",
    "pmatrix",
    "bmatrix",
    "dmatrix",
    "sup",
    "sub",
    "from",
    "to",
    "bar",
    "vec",
    "hat",
    "dot",
    "ddot",
    "tilde",
    "under",
    "acute",
    "grave",
    "check",
    "overline",
    "underline",
];

const FUNCTIONS: &[&str] = &[
    "sin", "cos", "tan", "cot", "sec", "csc", "arcsin", "arccos", "arctan", "sinh", "cosh", "tanh",
    "coth", "log", "ln", "lg", "exp", "det", "max", "min", "gcd", "lcm", "mod", "dim", "ker",
    "hom", "arg", "sgn",
];

const BIG_OPS: &[&str] = &[
    "sum", "prod", "coprod", "int", "dint", "tint", "oint", "union", "inter", "bigcup", "bigcap",
    "lim",
];

const GREEK_NAMES: &[&str] = &[
    "alpha", "beta", "gamma", "delta", "epsilon", "zeta", "eta", "theta", "iota", "kappa",
    "lambda", "mu", "nu", "xi", "omicron", "pi", "rho", "sigma", "tau", "upsilon", "phi", "chi",
    "psi", "omega",
];

const GREEK_LOWER: &[char] = &[
    'α', 'β', 'γ', 'δ', 'ε', 'ζ', 'η', 'θ', 'ι', 'κ', 'λ', 'μ', 'ν', 'ξ', 'ο', 'π', 'ρ', 'σ', 'τ',
    'υ', 'φ', 'χ', 'ψ', 'ω',
];

const GREEK_UPPER: &[char] = &[
    'Α', 'Β', 'Γ', 'Δ', 'Ε', 'Ζ', 'Η', 'Θ', 'Ι', 'Κ', 'Λ', 'Μ', 'Ν', 'Ξ', 'Ο', 'Π', 'Ρ', 'Σ', 'Τ',
    'Υ', 'Φ', 'Χ', 'Ψ', 'Ω',
];

const GREEK_VARIANT_NAMES: &[&str] = &["vartheta", "varpi", "varsigma", "varphi", "varepsilon"];
const GREEK_VARIANT_CHARS: &[char] = &['ϑ', 'ϖ', 'ς', 'φ', 'ε'];
const GREEK_VARIANT_PUA: &[u32] = &[0xE0BE, 0xE0BF, 0xE0C0, 0xE0C2, 0xE10E];

fn symbol_word(name: &str) -> Option<(&'static str, Cls)> {
    let hit = match name {
        "times" => ("×", Cls::Bin),
        "div" => ("÷", Cls::Bin),
        "pm" => ("±", Cls::Bin),
        "mp" => ("∓", Cls::Bin),
        "cdot" => ("⋅", Cls::Bin),
        "circ" => ("∘", Cls::Bin),
        "bullet" => ("∙", Cls::Bin),
        "cap" => ("∩", Cls::Bin),
        "cup" => ("∪", Cls::Bin),
        "wedge" => ("∧", Cls::Bin),
        "vee" => ("∨", Cls::Bin),
        "oplus" => ("⊕", Cls::Bin),
        "otimes" => ("⊗", Cls::Bin),
        "ominus" => ("⊖", Cls::Bin),
        "odot" => ("⊙", Cls::Bin),
        "uplus" => ("⊎", Cls::Bin),
        "leq" | "le" => ("≤", Cls::Rel),
        "geq" | "ge" => ("≥", Cls::Rel),
        "neq" | "ne" => ("≠", Cls::Rel),
        "approx" => ("≈", Cls::Rel),
        "sim" => ("∼", Cls::Rel),
        "simeq" => ("≃", Cls::Rel),
        "equiv" => ("≡", Cls::Rel),
        "cong" => ("≅", Cls::Rel),
        "doteq" => ("≐", Cls::Rel),
        "ll" => ("≪", Cls::Rel),
        "gg" => ("≫", Cls::Rel),
        "prec" => ("≺", Cls::Rel),
        "succ" => ("≻", Cls::Rel),
        "in" => ("∈", Cls::Rel),
        "notin" => ("∉", Cls::Rel),
        "ni" | "owns" => ("∋", Cls::Rel),
        "subset" => ("⊂", Cls::Rel),
        "supset" => ("⊃", Cls::Rel),
        "subseteq" => ("⊆", Cls::Rel),
        "supseteq" => ("⊇", Cls::Rel),
        "propto" => ("∝", Cls::Rel),
        "perp" => ("⊥", Cls::Rel),
        "parallel" => ("∥", Cls::Rel),
        "mapsto" => ("↦", Cls::Rel),
        "inf" | "infty" => ("∞", Cls::Ord),
        "cdots" => ("⋯", Cls::Ord),
        "ldots" => ("…", Cls::Ord),
        "vdots" => ("⋮", Cls::Ord),
        "ddots" => ("⋱", Cls::Ord),
        "prime" => ("′", Cls::Ord),
        "dprime" => ("″", Cls::Ord),
        "angle" => ("∠", Cls::Ord),
        "triangle" => ("△", Cls::Ord),
        "forall" => ("∀", Cls::Ord),
        "exist" | "exists" => ("∃", Cls::Ord),
        "partial" => ("∂", Cls::Ord),
        "nabla" => ("∇", Cls::Ord),
        "emptyset" => ("∅", Cls::Ord),
        "therefore" => ("∴", Cls::Ord),
        "because" => ("∵", Cls::Ord),
        "deg" => ("°", Cls::Ord),
        "centigrade" => ("℃", Cls::Ord),
        "fahrenheit" => ("℉", Cls::Ord),
        "hbar" => ("ℏ", Cls::Ord),
        "ell" => ("ℓ", Cls::Ord),
        "aleph" => ("ℵ", Cls::Ord),
        "dagger" => ("†", Cls::Ord),
        "ddagger" => ("‡", Cls::Ord),
        "box" => ("□", Cls::Ord),
        "diamond" => ("◇", Cls::Ord),
        "star" => ("⋆", Cls::Ord),
        "neg" | "lnot" => ("¬", Cls::Ord),
        "langle" => ("⟨", Cls::Open),
        "rangle" => ("⟩", Cls::Close),
        _ => return None,
    };
    Some(hit)
}

fn arrow(name: &str) -> Option<(&'static str, &'static str)> {
    let hit = match name {
        "rarrow" | "rightarrow" => ("→", "⇒"),
        "larrow" | "leftarrow" => ("←", "⇐"),
        "lrarrow" => ("↔", "⇔"),
        "uarrow" => ("↑", "⇑"),
        "darrow" => ("↓", "⇓"),
        "udarrow" => ("↕", "⇕"),
        _ => return None,
    };
    Some(hit)
}

/// 낱말의 뜻. 두 글자 이하 이름은 소문자로 쓴 것만 받는다(그리스 이름은 대문자도 받는다).
fn keyword(word: &str) -> Option<Key> {
    let first = word.chars().next()?;
    let lower = word.to_lowercase();
    let upper = first.is_uppercase();
    if let Some(index) = GREEK_NAMES.iter().position(|name| *name == lower) {
        return Some(Key::Greek(index, upper));
    }
    if word.chars().count() <= 2 && word != lower {
        return None;
    }
    if matches!(lower.as_str(), "rm" | "it" | "bold") {
        return Some(Key::Style(lower));
    }
    if STRUCTURAL.contains(&lower.as_str()) {
        return Some(Key::Structural(lower));
    }
    if let Some(index) = GREEK_VARIANT_NAMES.iter().position(|name| *name == lower) {
        return Some(Key::Greek(100 + index, false));
    }
    if let Some((plain, double)) = arrow(&lower) {
        return Some(Key::Symbol(
            if upper { double } else { plain }.to_string(),
            Cls::Rel,
        ));
    }
    if let Some((glyph, class)) = symbol_word(&lower) {
        return Some(Key::Symbol(glyph.to_string(), class));
    }
    if FUNCTIONS.contains(&lower.as_str()) {
        return Some(Key::Function(lower));
    }
    if BIG_OPS.contains(&lower.as_str()) {
        return Some(Key::BigOp(lower));
    }
    None
}

fn is_style(key: &Key) -> bool {
    matches!(key, Key::Style(_))
}

/// 영문자 이어짐을 낱말로 나눈다. 통째로 이름이면 그대로다.
fn split_word(word: &str) -> Vec<String> {
    if word.is_empty() {
        return Vec::new();
    }
    if keyword(word).is_some() {
        return vec![word.to_string()];
    }
    for prefix in ["bold", "rm", "it"] {
        if let Some(rest) = word.strip_prefix(prefix)
            && !rest.is_empty()
        {
            let mut parts = vec![prefix.to_string()];
            parts.extend(split_word(rest));
            return parts;
        }
    }
    let chars: Vec<char> = word.chars().collect();
    if chars.len() > 3 {
        for length in (4..chars.len()).rev() {
            let head: String = chars[..length].iter().collect();
            if let Some(key) = keyword(&head)
                && !is_style(&key)
            {
                let tail: String = chars[length..].iter().collect();
                let mut parts = vec![head];
                parts.extend(split_word(&tail));
                return parts;
            }
        }
    }
    vec![word.to_string()]
}

fn is_math_symbol(character: char) -> bool {
    let value = character as u32;
    (0x0370..=0x03FF).contains(&value)
        || (0x2190..=0x22FF).contains(&value)
        || (0x2032..=0x2034).contains(&value)
        || (0x27E8..=0x27E9).contains(&value)
        || [
            0x00D7, 0x00F7, 0x00B1, 0x00B7, 0x00B0, 0x2103, 0x2109, 0x2026,
        ]
        .contains(&value)
}

/// 스크립트 → 낱말.
pub(super) fn tokenize(script: &str) -> Vec<Token> {
    let chars: Vec<char> = script.chars().collect();
    let mut out = Vec::new();
    let mut at = 0;
    while at < chars.len() {
        let character = chars[at];
        if character.is_whitespace() {
            at += 1;
            continue;
        }
        match character {
            '~' => {
                out.push(Token::Space(0.5));
                at += 1;
                continue;
            }
            '`' => {
                out.push(Token::Space(0.125));
                at += 1;
                continue;
            }
            '{' => {
                out.push(Token::Open);
                at += 1;
                continue;
            }
            '}' => {
                out.push(Token::Close);
                at += 1;
                continue;
            }
            '^' => {
                out.push(Token::Sup);
                at += 1;
                continue;
            }
            '_' => {
                out.push(Token::Sub);
                at += 1;
                continue;
            }
            '&' => {
                out.push(Token::Amp);
                at += 1;
                continue;
            }
            '#' => {
                out.push(Token::Row);
                at += 1;
                continue;
            }
            '"' => {
                let mut index = at + 1;
                let mut text = String::new();
                while index < chars.len() && chars[index] != '"' {
                    text.push(chars[index]);
                    index += 1;
                }
                out.push(Token::Quoted(text));
                at = (index + 1).min(chars.len());
                continue;
            }
            _ => {}
        }
        if character.is_ascii_alphabetic() {
            let mut index = at;
            let mut word = String::new();
            while index < chars.len() && chars[index].is_ascii_alphabetic() {
                word.push(chars[index]);
                index += 1;
            }
            for part in split_word(&word) {
                out.push(Token::Word(part));
            }
            at = index;
            continue;
        }
        if character.is_ascii_digit() {
            let mut index = at;
            let mut number = String::new();
            while index < chars.len()
                && (chars[index].is_ascii_digit()
                    || (chars[index] == '.'
                        && index + 1 < chars.len()
                        && chars[index + 1].is_ascii_digit()))
            {
                number.push(chars[index]);
                index += 1;
            }
            out.push(Token::Number(number));
            at = index;
            continue;
        }
        if character.is_ascii() || is_math_symbol(character) {
            out.push(Token::Sym(character));
            at += 1;
            continue;
        }
        let mut index = at;
        let mut text = String::new();
        while index < chars.len()
            && !chars[index].is_ascii()
            && !is_math_symbol(chars[index])
            && !chars[index].is_whitespace()
        {
            text.push(chars[index]);
            index += 1;
        }
        out.push(Token::Text(text));
        at = index;
    }
    out
}

/// 스크립트 글자 → 그릴 글자. `pua`면 수식 글꼴의 사용자 영역 글리프를 쓴다.
#[derive(Clone, Copy)]
pub(super) struct Glyphs {
    pua: bool,
}

impl Glyphs {
    pub(super) fn new(pua: bool) -> Self {
        Self { pua }
    }
}

fn scalar(value: u32) -> String {
    char::from_u32(value).map(String::from).unwrap_or_default()
}

fn ascii_pua(character: char) -> Option<u32> {
    let value = match character {
        '!' => 0xE03E,
        '@' => 0xE03F,
        '#' => 0xE040,
        '$' => 0xE041,
        '%' => 0xE042,
        '*' => 0xE043,
        '(' => 0xE044,
        ')' => 0xE045,
        '-' | '−' => 0xE046,
        '=' => 0xE047,
        '+' => 0xE048,
        '[' => 0xE049,
        ']' => 0xE04A,
        '{' => 0xE04B,
        '}' => 0xE04C,
        '|' => 0xE04D,
        ';' => 0xE04E,
        ':' => 0xE04F,
        '′' => 0xE050,
        '″' => 0xE051,
        ',' => 0xE052,
        '.' => 0xE053,
        '/' => 0xE054,
        '<' => 0xE055,
        '>' => 0xE056,
        '?' => 0xE057,
        _ => return None,
    };
    Some(value)
}

fn accent_pua(kind: &str) -> Option<(u32, &'static str)> {
    let hit = match kind {
        "hat" => (0xE063, "ˆ"),
        "tilde" => (0xE066, "˜"),
        "dot" => (0xE05F, "˙"),
        "ddot" => (0xE060, "¨"),
        "acute" => (0xE05D, "´"),
        "grave" => (0xE05E, "`"),
        "check" => (0xE064, "ˇ"),
        _ => return None,
    };
    Some(hit)
}

impl Glyphs {
    /// 영문 낱말. roman이면 ASCII 그대로, 아니면 기울임(수식 글꼴이면 사용자 영역 글리프).
    pub(super) fn letters(&self, word: &str, roman: bool) -> (String, Face) {
        if roman {
            return (word.to_string(), Face::math());
        }
        if !self.pua {
            return (word.to_string(), Face::italic());
        }
        let mut out = String::new();
        for character in word.chars() {
            let value = character as u32;
            if (97..=122).contains(&value) {
                out.push_str(&scalar(0xE0E5 + value - 97));
            } else if (65..=90).contains(&value) {
                out.push_str(&scalar(0xE000 + value - 65));
            } else {
                out.push(character);
            }
        }
        (out, Face::math())
    }

    pub(super) fn digits(&self, number: &str) -> String {
        if !self.pua {
            return number.to_string();
        }
        let mut out = String::new();
        for character in number.chars() {
            match character {
                '0' => out.push_str(&scalar(0xE03D)),
                '1'..='9' => out.push_str(&scalar(0xE034 + character as u32 - 49)),
                '.' => out.push_str(&scalar(0xE053)),
                other => out.push(other),
            }
        }
        out
    }

    /// 기호 글자 하나. '-'는 빼기, '\''는 프라임이다.
    pub(super) fn symbol(&self, character: char) -> (String, Face) {
        let character = match character {
            '-' => '−',
            '\'' => '′',
            other => other,
        };
        if self.pua
            && let Some(value) = ascii_pua(character)
        {
            return (scalar(value), Face::math());
        }
        if !self.pua && character.is_alphabetic() && (0x03B1..=0x03C9).contains(&(character as u32))
        {
            return (character.to_string(), Face::italic());
        }
        (character.to_string(), Face::math())
    }

    pub(super) fn greek(&self, index: usize, upper: bool) -> (String, Face) {
        if index >= 100 {
            let variant = index - 100;
            if variant >= GREEK_VARIANT_NAMES.len() {
                return (String::new(), Face::math());
            }
            return if self.pua {
                (scalar(GREEK_VARIANT_PUA[variant]), Face::math())
            } else {
                (GREEK_VARIANT_CHARS[variant].to_string(), Face::italic())
            };
        }
        if index >= 24 {
            return (String::new(), Face::math());
        }
        if self.pua {
            let base = if upper { 0xE085 } else { 0xE09D };
            return (scalar(base + index as u32), Face::math());
        }
        if upper {
            (GREEK_UPPER[index].to_string(), Face::math())
        } else {
            (GREEK_LOWER[index].to_string(), Face::italic())
        }
    }

    pub(super) fn delimiter(&self, delimiter: &str) -> (String, Face) {
        if self.pua && delimiter == "|" {
            return (scalar(0xE101), Face::math());
        }
        let mut chars = delimiter.chars();
        if let (Some(character), None) = (chars.next(), chars.next()) {
            return self.symbol(character);
        }
        (delimiter.to_string(), Face::math())
    }

    pub(super) fn radical(&self) -> String {
        if self.pua {
            scalar(0xE05C)
        } else {
            "√".to_string()
        }
    }

    /// 큰 연산자: (글자, 크기 배율, 극한을 위·아래에 두는가).
    pub(super) fn bigop(&self, name: &str) -> (String, f32, bool) {
        match name {
            "sum" => (
                if self.pua {
                    scalar(0xE067)
                } else {
                    "∑".into()
                },
                1.8,
                true,
            ),
            "prod" => (
                if self.pua {
                    scalar(0xE068)
                } else {
                    "∏".into()
                },
                1.8,
                true,
            ),
            "coprod" => (
                if self.pua {
                    scalar(0xE069)
                } else {
                    "∐".into()
                },
                1.8,
                true,
            ),
            "union" | "bigcup" => ("⋃".into(), 1.8, true),
            "inter" | "bigcap" => ("⋂".into(), 1.8, true),
            "dint" => ("∬".into(), 2.0, false),
            "tint" => ("∭".into(), 2.0, false),
            "oint" => ("∮".into(), 2.0, false),
            _ => (
                if self.pua {
                    scalar(0xE05B)
                } else {
                    "∫".into()
                },
                2.0,
                false,
            ),
        }
    }

    pub(super) fn accent(&self, kind: &str) -> String {
        match accent_pua(kind) {
            None => String::new(),
            Some((value, plain)) => {
                if self.pua {
                    scalar(value)
                } else {
                    plain.to_string()
                }
            }
        }
    }
}

/// 구문 나무.
#[derive(Clone)]
pub(super) enum Node {
    Atom(String, Face, Cls),
    Space(f32),
    Group(Vec<Node>),
    Script(Box<Node>, Option<Box<Node>>, Option<Box<Node>>),
    Frac(Box<Node>, Box<Node>, bool),
    Root(Box<Node>, Option<Box<Node>>),
    Fence(Option<String>, Box<Node>, Option<String>),
    Grid(String, Vec<Vec<Node>>),
    BigOp(String, Option<Box<Node>>, Option<Box<Node>>),
    Accent(String, Box<Node>),
    Amp,
    RowSep,
}

/// 글자 모양: rm·it는 로만체와 기울임을 오가고, bold는 따로 켠다.
#[derive(Clone, Copy, Default)]
struct Style {
    roman: bool,
    bold: bool,
}

impl Style {
    fn apply(&mut self, command: &str) {
        if command == "bold" {
            self.bold = true;
        } else if command == "rm" || command == "it" {
            self.roman = command == "rm";
        }
    }

    pub(super) fn letters(&self, face: Face) -> Face {
        face.with_bold(self.bold)
    }
}

/// 구문 나무가 이보다 깊으면 수식을 그리지 않는다.
///
/// 파싱·배치·버리기가 모두 나무를 따라 재귀하고, 배치는 호출한 스레드의 스택을 쓴다
/// (iOS·macOS 보조 스레드는 512KB). 한/글 문서의 수식은 이 깊이에 한참 못 미친다.
pub(super) const MAX_DEPTH: usize = 64;

/// 구문 나무의 깊이. 스택을 쓰지 않고 센다.
pub(super) fn depth(nodes: &[Node]) -> usize {
    let mut deepest = 0;
    let mut pending: Vec<(&Node, usize)> = nodes.iter().map(|node| (node, 1)).collect();
    while let Some((node, level)) = pending.pop() {
        deepest = deepest.max(level);
        let next = level + 1;
        match node {
            Node::Atom(..) | Node::Space(_) | Node::Amp | Node::RowSep => {}
            Node::Group(list) => pending.extend(list.iter().map(|child| (child, next))),
            Node::Script(base, sub, sup) => {
                pending.push((base, next));
                pending.extend(sub.iter().chain(sup).map(|child| (&**child, next)));
            }
            Node::BigOp(_, sub, sup) => {
                pending.extend(sub.iter().chain(sup).map(|child| (&**child, next)));
            }
            Node::Frac(numerator, denominator, _) => {
                pending.push((numerator, next));
                pending.push((denominator, next));
            }
            Node::Root(content, index) => {
                pending.push((content, next));
                pending.extend(index.iter().map(|child| (&**child, next)));
            }
            Node::Fence(_, inner, _) | Node::Accent(_, inner) => pending.push((inner, next)),
            Node::Grid(_, rows) => pending.extend(rows.iter().flatten().map(|cell| (cell, next))),
        }
    }
    deepest
}

/// 낱말 → 구문 나무. 글자 모양은 묶음 안에서 뒤로 이어지고 묶음을 벗어나면 되돌아간다.
pub(super) struct Parser {
    tokens: Vec<Token>,
    at: usize,
    glyphs: Glyphs,
    /// 지금 들어와 있는 `primary` 수.
    nesting: usize,
    too_deep: bool,
}

impl Parser {
    pub(super) fn new(tokens: Vec<Token>, glyphs: Glyphs) -> Self {
        Self {
            tokens,
            at: 0,
            glyphs,
            nesting: 0,
            too_deep: false,
        }
    }

    fn peek(&self) -> Option<Token> {
        self.tokens.get(self.at).cloned()
    }

    /// 너무 깊다. 남은 낱말을 모두 건너뛰어 모든 반복을 끝낸다.
    fn give_up(&mut self) {
        self.too_deep = true;
        self.at = self.tokens.len();
    }

    /// 스크립트 전체. 짝 없는 `}`와 `right`는 건너뛴다.
    /// 나무가 [`MAX_DEPTH`]보다 깊어지면 `None`이다.
    pub(super) fn parse_all(&mut self) -> Option<Vec<Node>> {
        let mut out = Vec::new();
        let style = Style::default();
        while self.at < self.tokens.len() {
            out.extend(self.list(style));
            if self.peek().is_none() {
                break;
            }
            self.at += 1;
            _ = self.delimiter();
        }
        (!self.too_deep && depth(&out) <= MAX_DEPTH).then_some(out)
    }

    /// 닫는 `}`나 right 앞까지. 칸·행 표지는 목록에 남긴다.
    fn list(&mut self, style: Style) -> Vec<Node> {
        let mut current = style;
        let mut out = Vec::new();
        while let Some(token) = self.peek() {
            match &token {
                Token::Close => return out,
                Token::Amp => {
                    self.at += 1;
                    out.push(Node::Amp);
                    continue;
                }
                Token::Row => {
                    self.at += 1;
                    out.push(Node::RowSep);
                    continue;
                }
                Token::Word(word) => match keyword(word) {
                    Some(Key::Structural(name)) if name == "right" => return out,
                    Some(Key::Structural(name)) if name == "over" || name == "atop" => {
                        self.at += 1;
                        let numerator = out.pop().unwrap_or(Node::Group(Vec::new()));
                        // `a over b over c …`는 재귀 없이 분수가 겹쳐 쌓인다.
                        if depth(std::slice::from_ref(&numerator)) >= MAX_DEPTH {
                            self.give_up();
                            return out;
                        }
                        let denominator =
                            self.item(&mut current).unwrap_or(Node::Group(Vec::new()));
                        out.push(Node::Frac(
                            Box::new(numerator),
                            Box::new(denominator),
                            name == "over",
                        ));
                        continue;
                    }
                    Some(Key::Style(command)) => {
                        self.at += 1;
                        current.apply(&command);
                        continue;
                    }
                    _ => {}
                },
                _ => {}
            }
            if let Some(node) = self.item(&mut current) {
                out.push(node);
            }
        }
        out
    }

    /// 항목 하나와 그 뒤의 첨자(`^ _ sup sub from to`).
    fn item(&mut self, style: &mut Style) -> Option<Node> {
        let mut base = match self.peek() {
            Some(Token::Sup) | Some(Token::Sub) => Node::Group(Vec::new()),
            _ => self.primary(style)?,
        };
        let mut sub: Option<Node> = None;
        let mut sup: Option<Node> = None;
        while let Some(token) = self.peek() {
            match &token {
                Token::Sup => {
                    self.at += 1;
                    sup = Some(self.argument(*style));
                }
                Token::Sub => {
                    self.at += 1;
                    sub = Some(self.argument(*style));
                }
                Token::Word(word) => match keyword(word) {
                    Some(Key::Structural(name))
                        if matches!(name.as_str(), "sup" | "sub" | "from" | "to") =>
                    {
                        self.at += 1;
                        if name == "sup" || name == "to" {
                            sup = Some(self.argument(*style));
                        } else {
                            sub = Some(self.argument(*style));
                        }
                    }
                    _ => break,
                },
                _ => break,
            }
        }
        if sub.is_none() && sup.is_none() {
            return Some(base);
        }
        if let Node::BigOp(name, old_sub, old_sup) = base {
            return Some(Node::BigOp(
                name,
                sub.or(old_sub.map(|boxed| *boxed)).map(Box::new),
                sup.or(old_sup.map(|boxed| *boxed)).map(Box::new),
            ));
        }
        base = Node::Script(Box::new(base), sub.map(Box::new), sup.map(Box::new));
        Some(base)
    }

    /// 첨자·명령의 인자 하나(뒤 첨자는 붙이지 않는다). 없으면 빈 묶음.
    fn argument(&mut self, style: Style) -> Node {
        let mut current = style;
        while let Some(Token::Word(word)) = self.peek() {
            match keyword(&word) {
                Some(Key::Style(command)) => {
                    self.at += 1;
                    current.apply(&command);
                }
                _ => break,
            }
        }
        self.primary(&mut current)
            .unwrap_or(Node::Group(Vec::new()))
    }

    /// 항목 하나(첨자 없이). 묶음·명령 인자를 따라 재귀하므로 여기서 깊이를 센다.
    fn primary(&mut self, style: &mut Style) -> Option<Node> {
        if self.nesting >= MAX_DEPTH {
            self.give_up();
            return None;
        }
        self.nesting += 1;
        let node = self.primary_here(style);
        self.nesting -= 1;
        node
    }

    fn primary_here(&mut self, style: &mut Style) -> Option<Node> {
        let token = self.peek()?;
        self.at += 1;
        match token {
            Token::Open => {
                let inner = self.list(*style);
                if matches!(self.peek(), Some(Token::Close)) {
                    self.at += 1;
                }
                Some(Node::Group(inner))
            }
            Token::Close | Token::Amp | Token::Row | Token::Sup | Token::Sub => {
                self.at -= 1;
                None
            }
            Token::Space(amount) => Some(Node::Space(amount)),
            Token::Number(number) => Some(Node::Atom(
                self.glyphs.digits(&number),
                style.letters(Face::math()),
                Cls::Ord,
            )),
            Token::Sym(character) => {
                let (text, face) = self.glyphs.symbol(character);
                Some(Node::Atom(text, face, char_class(character)))
            }
            Token::Text(text) => Some(Node::Atom(text, style.letters(Face::text()), Cls::Ord)),
            Token::Quoted(text) => Some(self.quoted(&text, *style)),
            Token::Word(word) => self.word(&word, style),
        }
    }

    fn word(&mut self, word: &str, style: &mut Style) -> Option<Node> {
        match keyword(word) {
            Some(Key::Style(command)) => {
                style.apply(&command);
                Some(Node::Group(Vec::new()))
            }
            Some(Key::Structural(name)) => match name.as_str() {
                "sqrt" | "root" => {
                    let index = self.argument(*style);
                    if let Some(Token::Word(next)) = self.peek()
                        && next.to_lowercase() == "of"
                    {
                        self.at += 1;
                        return Some(Node::Root(
                            Box::new(self.argument(*style)),
                            Some(Box::new(index)),
                        ));
                    }
                    Some(Node::Root(Box::new(index), None))
                }
                "left" => {
                    let left = self.delimiter();
                    let inner = self.list(*style);
                    let mut right = None;
                    if let Some(Token::Word(next)) = self.peek()
                        && next.to_lowercase() == "right"
                    {
                        self.at += 1;
                        right = self.delimiter();
                    }
                    Some(Node::Fence(left, Box::new(Node::Group(inner)), right))
                }
                "right" => {
                    _ = self.delimiter();
                    Some(Node::Group(Vec::new()))
                }
                "cases" | "pile" | "lpile" | "rpile" | "eqalign" | "matrix" | "pmatrix"
                | "bmatrix" | "dmatrix" => {
                    let argument = self.argument(*style);
                    Some(Node::Grid(name, rows(argument)))
                }
                "bar" | "vec" | "hat" | "dot" | "ddot" | "tilde" | "under" | "acute" | "grave"
                | "check" | "overline" | "underline" => {
                    Some(Node::Accent(name, Box::new(self.argument(*style))))
                }
                _ => Some(Node::Group(Vec::new())),
            },
            Some(Key::Symbol(text, class)) => {
                let mut chars = text.chars();
                if let (Some(character), None) = (chars.next(), chars.next()) {
                    let (text, face) = self.glyphs.symbol(character);
                    return Some(Node::Atom(text, face, class));
                }
                Some(Node::Atom(text, Face::math(), class))
            }
            Some(Key::Greek(index, upper)) => {
                let (text, face) = self.glyphs.greek(index, upper);
                let face = if style.roman && !self.glyphs.pua {
                    Face::math()
                } else {
                    face
                };
                Some(Node::Atom(text, style.letters(face), Cls::Ord))
            }
            Some(Key::Function(name)) => {
                Some(Node::Atom(name, style.letters(Face::math()), Cls::Ord))
            }
            Some(Key::BigOp(name)) => Some(Node::BigOp(name, None, None)),
            None => {
                let (text, face) = self.glyphs.letters(word, style.roman);
                Some(Node::Atom(text, style.letters(face), Cls::Ord))
            }
        }
    }

    /// left·right 뒤의 구분 기호. `.`은 없음이다.
    pub(super) fn delimiter(&mut self) -> Option<String> {
        let token = self.peek()?;
        self.at += 1;
        match token {
            Token::Open => Some("{".to_string()),
            Token::Close => Some("}".to_string()),
            Token::Sym(character) => (character != '.').then(|| character.to_string()),
            Token::Word(word) => match word.to_lowercase().as_str() {
                "langle" => Some("⟨".to_string()),
                "rangle" => Some("⟩".to_string()),
                "vert" => Some("|".to_string()),
                "dvert" => Some("‖".to_string()),
                "lceil" => Some("⌈".to_string()),
                "rceil" => Some("⌉".to_string()),
                "lfloor" => Some("⌊".to_string()),
                "rfloor" => Some("⌋".to_string()),
                _ => {
                    self.at -= 1;
                    None
                }
            },
            _ => {
                self.at -= 1;
                None
            }
        }
    }

    /// `"…"` 안의 글: ASCII는 로만체, 그 밖의 글자는 한글 글꼴.
    fn quoted(&self, text: &str, style: Style) -> Node {
        let mut parts = Vec::new();
        let mut current = String::new();
        let mut ascii = true;
        let flush = |current: &mut String, ascii: bool, parts: &mut Vec<Node>| {
            if !current.is_empty() {
                parts.push(Node::Atom(
                    std::mem::take(current),
                    style.letters(if ascii { Face::math() } else { Face::text() }),
                    Cls::Ord,
                ));
            }
        };
        for character in text.chars() {
            if character.is_ascii() != ascii {
                flush(&mut current, ascii, &mut parts);
                ascii = character.is_ascii();
            }
            current.push(character);
        }
        flush(&mut current, ascii, &mut parts);
        Node::Group(parts)
    }
}

/// 기호 글자의 간격 갈래.
fn char_class(character: char) -> Cls {
    match character {
        '+' | '-' | '−' | '×' | '÷' | '±' | '∓' | '⋅' | '·' | '∘' | '∙' | '∩' | '∪' | '∧' | '∨'
        | '⊕' | '⊗' | '*' => Cls::Bin,
        '=' | '<' | '>' | '≤' | '≥' | '≠' | '≈' | '∼' | '≃' | '≡' | '≅' | '≪' | '≫' | '→' | '←'
        | '↔' | '⇒' | '⇐' | '⇔' | '∈' | '∉' | '∋' | '⊂' | '⊃' | '⊆' | '⊇' | '∝' | '⊥' | '∥'
        | '↑' | '↓' | '≒' => Cls::Rel,
        '(' | '[' | '⟨' => Cls::Open,
        ')' | ']' | '⟩' => Cls::Close,
        ',' | ';' => Cls::Punct,
        _ => Cls::Ord,
    }
}

fn rows(node: Node) -> Vec<Vec<Node>> {
    match node {
        Node::Group(list) => rows_of(&list),
        other => vec![vec![other]],
    }
}

pub(super) fn rows_of(list: &[Node]) -> Vec<Vec<Node>> {
    let mut out = Vec::new();
    let mut row = Vec::new();
    let mut cell = Vec::new();
    for node in list {
        match node {
            Node::Amp => {
                row.push(Node::Group(std::mem::take(&mut cell)));
            }
            Node::RowSep => {
                row.push(Node::Group(std::mem::take(&mut cell)));
                out.push(std::mem::take(&mut row));
            }
            other => cell.push(other.clone()),
        }
    }
    row.push(Node::Group(std::mem::take(&mut cell)));
    out.push(row);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keywords_split_like_the_script() {
        assert_eq!(keyword("alpha"), Some(Key::Greek(0, false)));
        assert_eq!(keyword("Alpha"), Some(Key::Greek(0, true)));
        assert_eq!(keyword("rm"), Some(Key::Style("rm".to_string())));
        assert_eq!(keyword("sqrt"), Some(Key::Structural("sqrt".to_string())));
        assert_eq!(keyword("sum"), Some(Key::BigOp("sum".to_string())));
        assert_eq!(
            keyword("Rarrow"),
            Some(Key::Symbol("⇒".to_string(), Cls::Rel))
        );
    }

    #[test]
    fn words_split_before_keywords() {
        assert_eq!(split_word("sqrtx"), vec!["sqrt", "x"]);
        assert_eq!(split_word("bold"), vec!["bold"]);
        assert_eq!(split_word("rmalpha"), vec!["rm", "alpha"]);
        assert_eq!(split_word("abcdef"), vec!["abcdef"]);
    }

    #[test]
    fn tokenize_keeps_structure() {
        let tokens = tokenize("a^2 + {b over c}");
        assert!(matches!(tokens[0], Token::Word(ref w) if w == "a"));
        assert!(matches!(tokens[1], Token::Sup));
        assert!(matches!(tokens[2], Token::Number(ref n) if n == "2"));
    }

    fn parse(script: &str) -> Option<Vec<Node>> {
        Parser::new(tokenize(script), Glyphs::new(false)).parse_all()
    }

    #[test]
    fn nesting_stops_at_the_depth_limit() {
        // 묶음 63겹 + 글자 = 깊이 64.
        let inside = "{".repeat(MAX_DEPTH - 1) + "x";
        assert_eq!(depth(&parse(&inside).unwrap()), MAX_DEPTH);
        assert!(parse(&("{".repeat(MAX_DEPTH) + "x")).is_none());
        // 분수 사슬은 재귀 없이 쌓이지만 같은 상한을 받는다.
        assert!(parse(&("x".to_string() + &" over y".repeat(MAX_DEPTH - 1))).is_some());
        assert!(parse(&("x".to_string() + &" over y".repeat(MAX_DEPTH))).is_none());
        assert!(parse(&"sqrt ".repeat(100_000)).is_none());
    }

    #[test]
    fn rows_split_on_cells_and_lines() {
        let list = vec![
            Node::Atom("a".into(), Face::math(), Cls::Ord),
            Node::Amp,
            Node::Atom("b".into(), Face::math(), Cls::Ord),
            Node::RowSep,
            Node::Atom("c".into(), Face::math(), Cls::Ord),
        ];
        let grid = rows_of(&list);
        assert_eq!(grid.len(), 2);
        assert_eq!(grid[0].len(), 2);
        assert_eq!(grid[1].len(), 1);
    }
}
