//! 글꼴 대체 규칙(`data/font-subst.tsv`). 문서가 적은 글꼴 이름에서 찾아볼 이름 차례를 만든다.

use crate::fonts::{FaceId, FontBook, key};
use hwp_core::FontFace;
use std::collections::HashMap;
use std::sync::OnceLock;

const TABLE: &str = include_str!("../data/font-subst.tsv");

#[derive(Default)]
pub struct Rules {
    legacy_latin: HashMap<String, String>,
    hft: HashMap<String, (String, bool)>,
    ttf: HashMap<String, String>,
    alias: HashMap<String, String>,
    missing: HashMap<String, String>,
    serif_words: Vec<String>,
    gungseo_words: Vec<String>,
    pub fallback_serif: Vec<String>,
    pub fallback_gungseo: Vec<String>,
    pub fallback_gothic: Vec<String>,
    pub fallback_glyph: Vec<String>,
}

/// 글꼴 계열. 찾지 못한 글꼴을 이 계열의 글꼴로 대신한다.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Family {
    Serif,
    Gungseo,
    Gothic,
}

impl Rules {
    pub fn builtin() -> &'static Rules {
        static RULES: OnceLock<Rules> = OnceLock::new();
        RULES.get_or_init(|| Rules::parse(TABLE))
    }

    fn parse(table: &str) -> Rules {
        let mut rules = Rules::default();
        for line in table.lines() {
            if line.starts_with('#') || line.trim().is_empty() {
                continue;
            }
            let mut fields = line.split('\t');
            let (Some(kind), Some(name), Some(value)) =
                (fields.next(), fields.next(), fields.next())
            else {
                continue;
            };
            let value = value.to_string();
            match kind {
                "legacy-latin" => {
                    rules.legacy_latin.entry(key(name)).or_insert(value);
                }
                "hft" => {
                    rules.hft.entry(key(name)).or_insert((value, false));
                }
                "hft-latin" => {
                    rules.hft.entry(key(name)).or_insert((value, true));
                }
                "ttf" => {
                    rules.ttf.entry(key(name)).or_insert(value);
                }
                "missing" => {
                    rules.missing.entry(key(name)).or_insert(value);
                }
                "alias" => {
                    rules.alias.entry(key(name)).or_insert(value);
                }
                "serif-word" => rules.serif_words.push(name.to_lowercase()),
                "gungseo-word" => rules.gungseo_words.push(name.to_lowercase()),
                "fallback-serif" => rules.fallback_serif.push(name.to_string()),
                "fallback-gungseo" => rules.fallback_gungseo.push(name.to_string()),
                "fallback-gothic" => rules.fallback_gothic.push(name.to_string()),
                "fallback-glyph" => rules.fallback_glyph.push(name.to_string()),
                _ => {}
            }
        }
        rules
    }

    /// 이름·글꼴 종류(0 모름, 1 TTF, 2 HFT)·언어 칸에 맞는 대신할 이름.
    pub fn target(&self, name: &str, kind: i32, lang: &str) -> Option<&str> {
        let name = key(name);
        if lang == "latin"
            && let Some(target) = self.legacy_latin.get(&name)
        {
            return Some(target);
        }
        if kind == 2
            && let Some((target, latin_only)) = self.hft.get(&name)
            && (!latin_only || lang == "latin")
        {
            return Some(target);
        }
        self.ttf.get(&name).map(String::as_str)
    }

    /// 찾을 이름 차례: 원래 이름·영문 기본 이름·파일 이름 별칭·없을 때 한/글이 쓴 글꼴 → 한컴 치환 → 대체 이름·그 치환.
    /// 같은 이름은 한 번만.
    pub fn candidates(&self, face: &FontFace, lang: &str) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        let mut add = |name: Option<&str>| {
            if let Some(name) = name
                && !name.is_empty()
                && !out.iter().any(|seen| key(seen) == key(name))
            {
                out.push(name.to_string());
            }
        };
        add(Some(&face.name));
        add(face.default_name.as_deref());
        add(self.alias.get(&key(&face.name)).map(String::as_str));
        add(self.missing.get(&key(&face.name)).map(String::as_str));
        add(self.target(&face.name, face.face_type, lang));
        add(face.alt_name.as_deref());
        if let Some(alt) = &face.alt_name {
            add(self.target(alt, face.alt_type, lang));
        }
        out
    }

    pub fn family(&self, name: &str) -> Family {
        let name = name.to_lowercase();
        if self.gungseo_words.iter().any(|word| name.contains(word)) {
            Family::Gungseo
        } else if self.serif_words.iter().any(|word| name.contains(word)) {
            Family::Serif
        } else {
            Family::Gothic
        }
    }

    pub fn fallbacks(&self, family: Family) -> &[String] {
        match family {
            Family::Serif => &self.fallback_serif,
            Family::Gungseo => &self.fallback_gungseo,
            Family::Gothic => &self.fallback_gothic,
        }
    }

    /// 문서 글꼴을 그릴 얼굴로 정한다.
    ///
    /// 후보 이름(문서가 적은 이름과 그 치환)을 먼저 훑어 내장 얼굴, 설치 얼굴 차례로 고른다.
    /// 후보 이름에서 모두 실패하면 계열 대체·글자 대체 이름을 같은 차례로 훑는다. 대체 이름의
    /// 내장 얼굴이 설치된 정확한 후보를 이기지 않게 하기 위해서다.
    pub fn resolve(
        &self,
        fonts: &FontBook,
        face: &FontFace,
        lang: &str,
        bold: bool,
    ) -> Option<(FaceId, bool)> {
        let candidates = self.candidates(face, lang);
        if let Some(hit) = candidates
            .iter()
            .find_map(|name| fonts.find_embedded(name, bold))
        {
            return Some(hit);
        }
        if let Some(hit) = candidates
            .iter()
            .find_map(|name| fonts.find_installed(name, bold))
        {
            return Some(hit);
        }
        let mut fallbacks: Vec<&String> = self.fallbacks(self.family(&face.name)).iter().collect();
        fallbacks.extend(&self.fallback_glyph);
        fallbacks
            .iter()
            .find_map(|name| fonts.find_embedded(name, bold))
            .or_else(|| {
                fallbacks
                    .iter()
                    .find_map(|name| fonts.find_installed(name, bold))
            })
            .or_else(|| fonts.any_face().map(|id| (id, false)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_loads_every_rule() {
        let rules = Rules::builtin();
        assert_eq!(rules.legacy_latin.len(), 35);
        assert_eq!(rules.hft.len(), 117);
        assert_eq!(rules.ttf.len(), 19);
    }

    /// 한/글 12.30.0은 고른 글꼴에 없는 글자(➊, ｢, ‧ 등)를 명조 계열에서도 함초롬돋움으로 그린다.
    #[test]
    fn missing_glyphs_fall_back_to_hamchorom_dotum_first() {
        assert_eq!(
            Rules::builtin().fallback_glyph.first().map(String::as_str),
            Some("함초롬돋움")
        );
    }

    #[test]
    fn target_follows_lookup_order() {
        let rules = Rules::builtin();
        assert_eq!(
            rules.target("HCI Poppy", 0, "latin"),
            Some("Palatino Linotype")
        );
        assert_eq!(
            rules.target("HCI Poppy", 2, "hangul"),
            Some("Palatino Linotype")
        );
        assert_eq!(rules.target("HCI Tulip", 2, "hangul"), None);
        assert_eq!(rules.target("한양그래픽", 1, "hangul"), None);
        assert_eq!(rules.target("한양그래픽", 2, "hangul"), Some("굴림"));
        assert_eq!(rules.target("Gulim", 0, "hangul"), Some("굴림"));
    }

    #[test]
    fn missing_font_falls_back_to_what_hancom_drew() {
        let rules = Rules::builtin();
        let names = rules.candidates(&FontFace::new("바탕".to_string()), "hangul");
        assert_eq!(names[..2], ["바탕".to_string(), "한컴바탕".to_string()]);
        let names = rules.candidates(&FontFace::new("돋움".to_string()), "hangul");
        assert_eq!(names[..2], ["돋움".to_string(), "한컴돋움".to_string()]);
        let names = rules.candidates(&FontFace::new("한양신명조".to_string()), "symbol");
        assert_eq!(
            names[..2],
            ["한양신명조".to_string(), "한컴바탕".to_string()]
        );
    }

    #[test]
    fn family_reads_name_words() {
        let rules = Rules::builtin();
        assert_eq!(rules.family("휴먼명조"), Family::Serif);
        assert_eq!(rules.family("한양궁서"), Family::Gungseo);
        assert_eq!(rules.family("맑은 고딕"), Family::Gothic);
    }
}
