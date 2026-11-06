//! 글꼴 색인. 받은 폴더들의 TrueType/OpenType 글꼴을 이름으로 찾고, 글자마다 글리프 번호와 폭을 낸다.
//! 글꼴 파일은 색인할 때 이름표만 읽고, 그릴 때 처음 쓰는 얼굴만 메모리에 올린다.

use crate::scene::PathOp;
use hwp_core::{BlobKind, Document};
use skrifa::instance::{LocationRef, Size};
use skrifa::outline::{DrawSettings, OutlinePen};
use skrifa::raw::{FileRef, TableProvider};
use skrifa::string::StringId;
use skrifa::{FontRef, GlyphId, MetadataProvider};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use unicode_normalization::UnicodeNormalization;

pub type FaceId = usize;

/// 문서에 내장된 글꼴을 색인할 때 쓰는 가상 경로의 첫 칸. 실제 파일 이름과 겹치지 않게
/// 파일 이름에 쓸 수 없는 제어 문자를 쓴다. 이 경로는 디스크를 읽지 않고 메모리 색인에만 쓴다.
const EMBEDDED_ROOT: &str = "\u{1}embedded";

/// 글꼴 이름을 맞출 때 쓰는 열쇠: NFC, 소문자, 공백 없음.
pub fn key(name: &str) -> String {
    name.nfc()
        .flat_map(char::to_lowercase)
        .filter(|character| !character.is_whitespace())
        .collect()
}

/// 글꼴 파일의 OS/2 `fsType`을 PDF에 넣어도 되는지로 풀어 놓은 것.
///
/// 이 값은 PDF에 글꼴 프로그램을 넣을 수 있는지 정한다. 글꼴 자료를 넣을 수 없는 얼굴은
/// 배치에도 쓰지 않고, 글꼴이 없을 때와 같은 대체 사슬로 넘어간다. 윤곽선을 그리는 것도
/// 글꼴 자료를 넣는 일이므로 포함 금지 얼굴에서는 하지 않는다.
///
/// 부분 집합을 금지하거나(0x100) 비트맵만 허가하면(0x200) 이 PDF 작성 방식으로는 지킬 수
/// 없으므로 포함하지 않고 대체 사슬로 넘어간다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Embedding {
    /// fsType 0: 설치 가능. 부분 집합으로 포함한다.
    Installable,
    /// fsType 2만: 포함 금지. 글꼴 자료를 넣지 않고 대체 사슬로 넘어간다.
    Restricted,
    /// fsType 4: 보기·인쇄용 포함 허가. 부분 집합으로 포함한다.
    PreviewPrint,
    /// fsType 8: 편집용 포함 허가. 부분 집합으로 포함한다.
    Editable,
    /// fsType 0x100: 부분 집합 포함 금지. 이 PDF 작성 방식은 늘 부분 집합을 넣으므로
    /// 포함하지 않고 대체 사슬로 넘어간다.
    NoSubsetting,
    /// fsType 0x200: 비트맵만 포함 허가. 쓸 수 있는 내장 비트맵이 없으므로 포함하지 않고
    /// 대체 사슬로 넘어간다.
    BitmapOnly,
    /// 허가 값을 알 수 없음(알 수 없는 비트 조합, OS/2 없음). 포함하지 않고 대체 사슬로 넘어간다.
    Unknown,
}

impl Embedding {
    /// OS/2 버전 2 이상의 `fsType`을 푼다.
    pub fn from_fs_type(fs_type: u16) -> Self {
        Self::from_os2(2, fs_type)
    }

    /// OS/2 `version`과 `fsType`을 함께 푼다.
    ///
    /// 버전 0·1에서는 비트 4-15가 예약이라 무시한다. 버전 2 이상에서도 예약 비트가 서 있으면
    /// 허가 값을 신뢰하지 않고 [`Embedding::Unknown`]으로 둔다. 허가 비트가 여럿이면 가장
    /// 너그러운 쪽을 따른다(OpenType 규격).
    pub fn from_os2(version: u16, fs_type: u16) -> Self {
        let fs_type = if version <= 1 {
            fs_type & 0x000F
        } else {
            fs_type
        };
        if fs_type & 0x0200 != 0 {
            Self::BitmapOnly
        } else if fs_type & 0x0100 != 0 {
            Self::NoSubsetting
        } else if fs_type & !0x000F != 0 {
            // 비트 4-7, 10-15는 예약이다. 값이 서 있으면 해석하지 않는다.
            Self::Unknown
        } else {
            let kind = fs_type & 0x000F;
            // 0x1은 폐기된 예약 비트다. 다른 허가 비트와 함께 서 있어도 유효한 허가 값
            // (0/2/4/8)이 아니므로 너그러운 쪽으로 읽지 않는다.
            if kind & 0x0001 != 0 {
                Self::Unknown
            } else if kind == 0 {
                Self::Installable
            } else if kind & 0x0008 != 0 {
                Self::Editable
            } else if kind & 0x0004 != 0 {
                Self::PreviewPrint
            } else if kind & 0x0002 != 0 {
                Self::Restricted
            } else {
                Self::Unknown
            }
        }
    }

    pub fn can_embed(self) -> bool {
        matches!(
            self,
            Self::Installable | Self::PreviewPrint | Self::Editable
        )
    }
}

struct FaceInfo {
    path: PathBuf,
    index: u32,
    weight: f32,
    embedding: Embedding,
    /// 글꼴 이름표의 패밀리 이름. 포함할 수 없는 얼굴을 글꼴이 없을 때와 같은 사슬로
    /// 대체할 때 쓴다.
    name: Option<String>,
    /// 지금 문서에 붙인 얼굴인지. 문서를 바꾸면 참인 얼굴만 지운다.
    embedded: bool,
}

/// 글자 하나를 그 얼굴로 그릴 때의 글리프와 진행 폭(em 단위).
#[derive(Clone, Copy)]
pub struct GlyphInfo {
    pub glyph: u32,
    pub advance: f32,
}

/// 얼굴 하나의 세로 치수(em 단위, 위가 양수).
#[derive(Clone, Copy)]
pub struct FaceMetrics {
    pub ascent: f32,
    pub descent: f32,
    pub x_height: f32,
}

/// 글자들의 잉크 상자(em 단위, 위가 +, 기준점에서 잰다). 그린 글자가 없으면 폭·높이가 0이다.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Ink {
    pub min_x: f32,
    pub max_x: f32,
    pub min_y: f32,
    pub max_y: f32,
}

impl Ink {
    pub fn width(&self) -> f32 {
        self.max_x - self.min_x
    }

    pub fn height(&self) -> f32 {
        self.max_y - self.min_y
    }

    pub fn mid_y(&self) -> f32 {
        (self.min_y + self.max_y) / 2.0
    }
}

/// 글꼴 파일 바이트. 앱이 매핑해 넘긴 범위도 복사하지 않고 담는다.
type Bytes = Arc<dyn AsRef<[u8]> + Send + Sync>;

struct Loaded {
    data: Bytes,
    pdf: Option<krilla::text::Font>,
    metrics: FaceMetrics,
    glyphs: HashMap<char, Option<GlyphInfo>>,
    inks: HashMap<String, Ink>,
}

#[derive(Default)]
pub struct FontBook {
    faces: BTreeMap<FaceId, FaceInfo>,
    names: HashMap<String, Vec<FaceId>>,
    files: HashMap<PathBuf, Bytes>,
    loaded: HashMap<FaceId, Loaded>,
    /// 다음에 붙일 얼굴 번호. 문서를 바꿔도 줄이지 않아 지난 `Page`의 `FaceId`가 새 얼굴을
    /// 가리키지 않는다.
    next_face: FaceId,
    /// 한/글이 설치한 HFT 영문 글꼴의 폭(`hftinfo.dat`이 있는 폴더를 받았을 때).
    hft: crate::hft::HftIndex,
}

const NAME_IDS: [StringId; 4] = [
    StringId::FAMILY_NAME,
    StringId::FULL_NAME,
    StringId::POSTSCRIPT_NAME,
    StringId::TYPOGRAPHIC_FAMILY_NAME,
];

impl FontBook {
    /// 폴더(하위 폴더까지)와 글꼴 파일들의 이름표를 색인한다. 읽지 못한 파일은 건너뛴다.
    /// 같은 이름의 글꼴이 여럿이면 앞에 받은 경로의 것을 쓴다.
    pub fn scan(paths: &[PathBuf]) -> Self {
        let mut book = Self::default();
        let mut files = Vec::new();
        for path in paths {
            collect_font_files(path, &mut files);
        }
        for info in files.iter().filter(|path| is_hft_info(path)) {
            if let Ok(bytes) = std::fs::read(info) {
                book.hft.add_info(info, &bytes);
            }
        }
        files.retain(|path| !is_hft_info(path));
        let mut seen = std::collections::HashSet::new();
        files.retain(|path| seen.insert(path.clone()));
        for path in files {
            let Ok(bytes) = std::fs::read(&path) else {
                continue;
            };
            book.index_file(&path, &bytes, false);
        }
        book
    }

    /// 메모리에 있는 글꼴 파일 하나를 더한다(시험용 글꼴 등).
    ///
    /// 문서를 붙인 뒤에 부르면 먼저 그 문서의 내장 글꼴을 지운다. 얼굴 번호는 다시 쓰지
    /// 않으므로, 이전 문서의 `Page`가 가리키는 [`FaceId`]는 새 얼굴을 가리키지 않는다.
    /// 그런 쪽은 먼저 [`crate::pdf::write`]로 다 써야 하며, 나중에 쓰면 오류가 난다.
    pub fn add_bytes(&mut self, path: PathBuf, bytes: impl AsRef<[u8]> + Send + Sync + 'static) {
        self.detach_document();
        let bytes: Bytes = Arc::new(bytes);
        self.index_file(&path, (*bytes).as_ref(), false);
        self.files.insert(path, bytes);
    }

    /// 문서에 내장된 글꼴을 이 색인에 붙이고 색인한 얼굴 수를 돌려준다.
    ///
    /// 먼저 앞 문서의 내장 글꼴을 지운다. 글꼴 자료는 문서 모델의 메모리에서만 읽고
    /// 디스크에 쓰지 않는다. HWPX는 `isEmbedded`인 얼굴의 `binaryItemIDRef`가 가리키는
    /// 항목만 붙이고, 그 얼굴 이름도 함께 붙여 내부 이름표와 문서가 적은 이름이 달라도
    /// 찾을 수 있게 한다. HWP5처럼 `binaryItemIDRef`가 없는 문서는 글꼴 `BinData`를 모두 쓴다.
    pub fn attach_document(&mut self, document: &Document) -> usize {
        self.detach_document();
        let mut aliases: HashMap<&str, Vec<&str>> = HashMap::new();
        let mut embedded_refs = HashSet::new();
        let mut declared_refs = HashSet::new();
        for face in document.font_faces.values().flatten() {
            let Some(reference) = face.binary_ref.as_deref().filter(|value| !value.is_empty())
            else {
                continue;
            };
            declared_refs.insert(reference);
            if !face.embedded {
                continue;
            }
            embedded_refs.insert(reference);
            if !face.name.is_empty() {
                aliases.entry(reference).or_default().push(&face.name);
            }
        }
        let mut added = 0;
        for (order, blob) in document
            .bins
            .iter()
            .enumerate()
            .filter(|(_, blob)| blob.kind == BlobKind::Font)
        {
            // HWPX는 isEmbedded인 얼굴이 가리키는 항목만 내장 글꼴로 쓴다. HWP5는
            // binaryItemIDRef가 없으므로 모든 글꼴 BinData를 쓴다.
            if !declared_refs.is_empty() && !embedded_refs.contains(blob.id.as_str()) {
                continue;
            }
            // 경로에 문서가 정한 id를 넣지 않는다. id에 `C:\...`나 `..\`가 들어와도
            // 가상 루트 밖으로 나가지 않게 순번만 쓴다.
            let path = PathBuf::from(EMBEDDED_ROOT).join(order.to_string());
            let ids = self.index_file(&path, &blob.data, true);
            if ids.is_empty() {
                continue;
            }
            if let Some(names) = aliases.get(blob.id.as_str()) {
                for id in &ids {
                    for name in names {
                        let keys = self.names.entry(key(name)).or_default();
                        if !keys.contains(id) {
                            keys.push(*id);
                        }
                    }
                }
            }
            self.files.insert(path, blob.data.clone());
            added += ids.len();
        }
        added
    }

    /// 지금 문서에 내장된 글꼴 얼굴 수.
    pub fn document_face_count(&self) -> usize {
        self.faces.values().filter(|face| face.embedded).count()
    }

    fn detach_document(&mut self) {
        let embedded: HashSet<FaceId> = self
            .faces
            .iter()
            .filter(|(_, face)| face.embedded)
            .map(|(id, _)| *id)
            .collect();
        if embedded.is_empty() {
            return;
        }
        self.faces.retain(|id, _| !embedded.contains(id));
        for ids in self.names.values_mut() {
            ids.retain(|id| !embedded.contains(id));
        }
        self.names.retain(|_, ids| !ids.is_empty());
        self.loaded.retain(|id, _| !embedded.contains(id));
        self.files
            .retain(|path, _| !path.starts_with(Path::new(EMBEDDED_ROOT)));
    }

    fn index_file(&mut self, path: &Path, bytes: &[u8], embedded: bool) -> Vec<FaceId> {
        let mut added = Vec::new();
        let Ok(file) = FileRef::new(bytes) else {
            return added;
        };
        for (index, font) in file.fonts().enumerate() {
            let Ok(font) = font else {
                continue;
            };
            if font.charmap().mappings().next().is_none() {
                continue;
            }
            let embedding = font.os2().map_or(Embedding::Unknown, |os2| {
                Embedding::from_os2(os2.version(), os2.fs_type())
            });
            let name = font
                .localized_strings(StringId::FAMILY_NAME)
                .next()
                .map(|name| name.to_string());
            let id = self.next_face;
            self.next_face += 1;
            self.faces.insert(
                id,
                FaceInfo {
                    path: path.to_path_buf(),
                    index: index as u32,
                    weight: font.attributes().weight.value(),
                    embedding,
                    name,
                    embedded,
                },
            );
            let mut keys: Vec<String> = NAME_IDS
                .iter()
                .flat_map(|name_id| font.localized_strings(*name_id))
                .map(|name| key(&name.to_string()))
                .filter(|name| !name.is_empty())
                .collect();
            keys.sort();
            keys.dedup();
            for name in keys {
                self.names.entry(name).or_default().push(id);
            }
            added.push(id);
        }
        added
    }

    /// 영문 HFT 글꼴의 폭 표 번호([`Self::hft_advance`]에 쓴다).
    pub(crate) fn hft_latin(&mut self, name: &str, bold: bool, italic: bool) -> Option<usize> {
        self.hft.find(name, bold, italic)
    }

    /// HFT 폭 표의 글자 폭(em).
    pub(crate) fn hft_advance(&self, id: usize, ch: char) -> Option<f32> {
        self.hft.advance(id, ch)
    }

    pub fn face_count(&self) -> usize {
        self.faces.len()
    }

    /// PDF에 넣을 수 있는 얼굴을 색인 순서대로 돌려준다.
    pub fn embeddable_faces(&self) -> impl Iterator<Item = FaceId> + '_ {
        self.faces
            .iter()
            .filter(|(_, face)| face.embedding.can_embed())
            .map(|(id, _)| *id)
    }

    /// PDF에 글꼴 프로그램을 넣을 수 있는 첫 얼굴. 이름으로 찾지 못할 때 쓴다.
    pub fn any_face(&self) -> Option<FaceId> {
        self.embeddable_faces().next()
    }

    /// 이 얼굴의 포함 허가. 색인에 없으면 `None`.
    pub fn embedding(&self, id: FaceId) -> Option<Embedding> {
        self.faces.get(&id).map(|face| face.embedding)
    }

    /// 글꼴 이름표의 패밀리 이름(진단·대체용).
    pub(crate) fn face_name(&self, id: FaceId) -> Option<&str> {
        self.faces.get(&id)?.name.as_deref()
    }

    /// PDF에 넣을 수 없는 얼굴을 글꼴이 없을 때와 같은 사슬로 대체한다.
    pub(crate) fn fallback(&self, id: FaceId, bold: bool) -> Option<(FaceId, bool)> {
        let name = self.face_name(id)?.to_string();
        crate::subst::Rules::builtin().resolve(self, &hwp_core::FontFace::new(name), "hangul", bold)
    }

    /// 얼굴을 읽어 온 글꼴 파일 이름(진단용).
    pub fn file_name(&self, id: FaceId) -> Option<String> {
        let info = self.faces.get(&id)?;
        let name = info.path.file_name()?.to_string_lossy();
        Some(format!("{name}#{}", info.index))
    }

    pub fn has(&self, name: &str) -> bool {
        self.names.contains_key(&key(name))
    }

    /// 이름에 맞는 PDF에 넣을 수 있는 얼굴. 굵게면 그 이름의 얼굴 중 가장 굵은 쪽(600 이상)을,
    /// 아니면 400에 가까운 것을 고른다. 문서에 내장된 얼굴을 먼저 고른다. `fsType`이 포함을
    /// 막는 얼굴은 건너뛰어 글꼴이 없을 때와 같은 사슬로 대체하게 한다.
    /// 돌려주는 bool은 고른 얼굴이 정말 굵은지다(아니면 그릴 때 획을 덧그린다).
    pub fn find(&self, name: &str, bold: bool) -> Option<(FaceId, bool)> {
        self.find_embedded(name, bold)
            .or_else(|| self.find_installed(name, bold))
    }

    /// 이 이름의 문서 내장 얼굴만 고른다.
    pub(crate) fn find_embedded(&self, name: &str, bold: bool) -> Option<(FaceId, bool)> {
        self.pick(name, bold, true)
    }

    /// 이 이름의 설치된 얼굴만 고른다.
    pub(crate) fn find_installed(&self, name: &str, bold: bool) -> Option<(FaceId, bool)> {
        self.pick(name, bold, false)
    }

    fn pick(&self, name: &str, bold: bool, embedded: bool) -> Option<(FaceId, bool)> {
        let ids = self.names.get(&key(name))?;
        let target = if bold { 700.0 } else { 400.0 };
        let id = *ids
            .iter()
            .filter(|id| {
                self.faces
                    .get(*id)
                    .is_some_and(|face| face.embedded == embedded && face.embedding.can_embed())
            })
            .min_by(|a, b| {
                let da = (self.faces[*a].weight - target).abs();
                let db = (self.faces[*b].weight - target).abs();
                da.total_cmp(&db).then(a.cmp(b))
            })?;
        Some((id, self.faces[&id].weight >= 600.0))
    }

    fn load(&mut self, id: FaceId) -> Option<&mut Loaded> {
        if !self.loaded.contains_key(&id) {
            let info = self.faces.get(&id)?;
            let data = match self.files.get(&info.path) {
                Some(data) => data.clone(),
                None => {
                    let data: Bytes = Arc::new(std::fs::read(&info.path).ok()?);
                    self.files.insert(info.path.clone(), data.clone());
                    data
                }
            };
            let font = FontRef::from_index((*data).as_ref(), info.index).ok()?;
            let metrics = font.metrics(Size::unscaled(), LocationRef::default());
            let upem = f32::from(metrics.units_per_em.max(1));
            let metrics = FaceMetrics {
                ascent: metrics.ascent / upem,
                descent: -metrics.descent / upem,
                x_height: metrics.x_height.map_or(0.5, |height| height / upem),
            };
            let pdf = krilla::text::Font::new(data.clone().into(), info.index);
            self.loaded.insert(
                id,
                Loaded {
                    data,
                    pdf,
                    metrics,
                    glyphs: HashMap::new(),
                    inks: HashMap::new(),
                },
            );
        }
        self.loaded.get_mut(&id)
    }

    /// 이 얼굴에 그 글자가 있으면 글리프와 폭.
    pub fn glyph(&mut self, id: FaceId, character: char) -> Option<GlyphInfo> {
        let index = self.faces.get(&id)?.index;
        let loaded = self.load(id)?;
        if let Some(hit) = loaded.glyphs.get(&character) {
            return *hit;
        }
        let font = FontRef::from_index((*loaded.data).as_ref(), index).ok()?;
        let hit = font.charmap().map(character).and_then(|glyph| {
            let metrics = font.metrics(Size::unscaled(), LocationRef::default());
            let upem = f32::from(metrics.units_per_em.max(1));
            let advance = font
                .glyph_metrics(Size::unscaled(), LocationRef::default())
                .advance_width(glyph)?;
            Some(GlyphInfo {
                glyph: glyph.to_u32(),
                advance: advance / upem,
            })
        });
        loaded.glyphs.insert(character, hit);
        hit
    }

    pub fn metrics(&mut self, id: FaceId) -> FaceMetrics {
        self.load(id).map_or(
            FaceMetrics {
                ascent: 0.85,
                descent: 0.15,
                x_height: 0.5,
            },
            |loaded| loaded.metrics,
        )
    }

    /// 이 얼굴로 그 글자들을 이어 놓았을 때의 잉크 상자(em 단위). 글자가 없으면 빈 상자다.
    pub fn ink(&mut self, id: FaceId, text: &str) -> Ink {
        let Some(index) = self.faces.get(&id).map(|face| face.index) else {
            return Ink::default();
        };
        let Some(loaded) = self.load(id) else {
            return Ink::default();
        };
        if let Some(hit) = loaded.inks.get(text) {
            return *hit;
        }
        let ink = ink_of((*loaded.data).as_ref(), index, text);
        loaded.inks.insert(text.to_string(), ink);
        ink
    }

    pub fn pdf_font(&mut self, id: FaceId) -> Option<krilla::text::Font> {
        if !self.faces.get(&id)?.embedding.can_embed() {
            return None;
        }
        self.load(id)?.pdf.clone()
    }

    /// 글리프 하나의 윤곽선을 em 단위(위가 +)로 돌려준다. 윤곽선도 글꼴 자료를 넣는 일이므로
    /// PDF에 넣을 수 없는 얼굴이면 `None`이다.
    pub fn glyph_outline(&mut self, id: FaceId, glyph: u32) -> Option<Vec<PathOp>> {
        if !self.faces.get(&id)?.embedding.can_embed() {
            return None;
        }
        let index = self.faces.get(&id)?.index;
        let loaded = self.load(id)?;
        let font = FontRef::from_index((*loaded.data).as_ref(), index).ok()?;
        let metrics = font.metrics(Size::unscaled(), LocationRef::default());
        let upem = f32::from(metrics.units_per_em.max(1));
        let outline = font.outline_glyphs().get(GlyphId::new(glyph))?;
        let mut pen = PathPen::default();
        outline
            .draw(
                DrawSettings::unhinted(Size::unscaled(), LocationRef::default()),
                &mut pen,
            )
            .ok()?;
        if pen.ops.is_empty() {
            return None;
        }
        Some(
            pen.ops
                .into_iter()
                .map(|op| match op {
                    PathOp::Move(x, y) => PathOp::Move(x / upem, -y / upem),
                    PathOp::Line(x, y) => PathOp::Line(x / upem, -y / upem),
                    PathOp::Cubic(x1, y1, x2, y2, x, y) => PathOp::Cubic(
                        x1 / upem,
                        -y1 / upem,
                        x2 / upem,
                        -y2 / upem,
                        x / upem,
                        -y / upem,
                    ),
                    PathOp::Close => PathOp::Close,
                })
                .collect(),
        )
    }
}

/// 글꼴 윤곽선을 [`PathOp`]로 옮기는 펜. 2차 베지에는 3차로 바꾼다.
#[derive(Default)]
struct PathPen {
    ops: Vec<PathOp>,
    current: (f32, f32),
    start: (f32, f32),
}

impl OutlinePen for PathPen {
    fn move_to(&mut self, x: f32, y: f32) {
        self.ops.push(PathOp::Move(x, y));
        self.current = (x, y);
        self.start = (x, y);
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.ops.push(PathOp::Line(x, y));
        self.current = (x, y);
    }

    fn quad_to(&mut self, cx: f32, cy: f32, x: f32, y: f32) {
        let (x0, y0) = self.current;
        let c1 = (x0 + 2.0 / 3.0 * (cx - x0), y0 + 2.0 / 3.0 * (cy - y0));
        let c2 = (x + 2.0 / 3.0 * (cx - x), y + 2.0 / 3.0 * (cy - y));
        self.ops.push(PathOp::Cubic(c1.0, c1.1, c2.0, c2.1, x, y));
        self.current = (x, y);
    }

    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        self.ops.push(PathOp::Cubic(cx0, cy0, cx1, cy1, x, y));
        self.current = (x, y);
    }

    fn close(&mut self) {
        self.ops.push(PathOp::Close);
        self.current = self.start;
    }
}

/// 글자들의 잉크 상자(em 단위). 윤곽선 좌표는 글꼴 단위이고 위가 +다.
fn ink_of(data: &[u8], index: u32, text: &str) -> Ink {
    let Ok(font) = FontRef::from_index(data, index) else {
        return Ink::default();
    };
    let metrics = font.metrics(Size::unscaled(), LocationRef::default());
    let upem = f32::from(metrics.units_per_em.max(1));
    let charmap = font.charmap();
    let advances = font.glyph_metrics(Size::unscaled(), LocationRef::default());
    let outlines = font.outline_glyphs();
    let mut pen = InkPen::default();
    let (mut left, mut right) = (f32::MAX, f32::MIN);
    let (mut top, mut bottom) = (f32::MAX, f32::MIN);
    let mut drawn = false;
    let mut x = 0.0f32;
    for character in text.chars() {
        let Some(glyph) = charmap.map(character) else {
            continue;
        };
        if let Some(outline) = outlines.get(glyph) {
            pen.reset();
            let _ = outline.draw(
                DrawSettings::unhinted(Size::unscaled(), LocationRef::default()),
                &mut pen,
            );
            if let Some(bounds) = pen.bounds() {
                left = left.min(bounds.min_x + x);
                right = right.max(bounds.max_x + x);
                top = top.max(bounds.max_y);
                bottom = bottom.min(bounds.min_y);
                drawn = true;
            }
        }
        x += advances.advance_width(glyph).unwrap_or(0.0);
    }
    if !drawn {
        return Ink::default();
    }
    Ink {
        min_x: left / upem,
        max_x: right / upem,
        min_y: bottom / upem,
        max_y: top / upem,
    }
}

/// 윤곽선을 훑어 잉크 상자를 모으는 펜. 곡선은 양 끝이 아니라 실제 극값에서 잰다.
#[derive(Default)]
struct InkPen {
    left: f32,
    right: f32,
    top: f32,
    bottom: f32,
    touched: bool,
    current: (f32, f32),
}

impl InkPen {
    fn reset(&mut self) {
        *self = Self::default();
    }

    fn point(&mut self, x: f32, y: f32) {
        self.current = (x, y);
        if !self.touched {
            (self.left, self.right) = (x, x);
            (self.bottom, self.top) = (y, y);
            self.touched = true;
            return;
        }
        self.left = self.left.min(x);
        self.right = self.right.max(x);
        self.bottom = self.bottom.min(y);
        self.top = self.top.max(y);
    }

    fn bounds(&self) -> Option<Ink> {
        self.touched.then_some(Ink {
            min_x: self.left,
            max_x: self.right,
            min_y: self.bottom,
            max_y: self.top,
        })
    }
}

impl OutlinePen for InkPen {
    fn move_to(&mut self, x: f32, y: f32) {
        self.point(x, y);
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.point(x, y);
    }

    fn quad_to(&mut self, cx: f32, cy: f32, x: f32, y: f32) {
        let (x0, y0) = self.current;
        self.point(x, y);
        let mut ts = [1.0f32; 2];
        let mut count = 0;
        quad_extrema(x0, cx, x, &mut ts, &mut count);
        quad_extrema(y0, cy, y, &mut ts, &mut count);
        for t in ts.into_iter().take(count) {
            self.point(quad_at(x0, cx, x, t), quad_at(y0, cy, y, t));
        }
    }

    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        let (x0, y0) = self.current;
        self.point(x, y);
        let mut ts = [1.0f32; 4];
        let mut count = 0;
        cubic_extrema(x0, cx0, cx1, x, &mut ts, &mut count);
        cubic_extrema(y0, cy0, cy1, y, &mut ts, &mut count);
        for t in ts.into_iter().take(count) {
            self.point(cubic_at(x0, cx0, cx1, x, t), cubic_at(y0, cy0, cy1, y, t));
        }
    }

    fn close(&mut self) {}
}

fn quad_at(p0: f32, c: f32, p1: f32, t: f32) -> f32 {
    let u = 1.0 - t;
    u * u * p0 + 2.0 * u * t * c + t * t * p1
}

fn cubic_at(p0: f32, c0: f32, c1: f32, p1: f32, t: f32) -> f32 {
    let u = 1.0 - t;
    u * u * u * p0 + 3.0 * u * u * t * c0 + 3.0 * u * t * t * c1 + t * t * t * p1
}

/// 2차 베지에가 멈추는 곳(0<t<1).
fn quad_extrema(p0: f32, c: f32, p1: f32, out: &mut [f32], count: &mut usize) {
    let denominator = p0 - 2.0 * c + p1;
    if denominator.abs() <= f32::EPSILON {
        return;
    }
    let t = (p0 - c) / denominator;
    if t > 0.0 && t < 1.0 && *count < out.len() {
        out[*count] = t;
        *count += 1;
    }
}

/// 3차 베지에가 멈추는 곳(0<t<1).
fn cubic_extrema(p0: f32, c0: f32, c1: f32, p1: f32, out: &mut [f32], count: &mut usize) {
    let a = -p0 + 3.0 * c0 - 3.0 * c1 + p1;
    let b = 2.0 * (p0 - 2.0 * c0 + c1);
    let c = c0 - p0;
    let mut push = |t: f32, count: &mut usize| {
        if t > 0.0 && t < 1.0 && *count < out.len() {
            out[*count] = t;
            *count += 1;
        }
    };
    if a.abs() <= f32::EPSILON {
        if b.abs() > f32::EPSILON {
            push(-c / b, count);
        }
        return;
    }
    let discriminant = b * b - 4.0 * a * c;
    if discriminant < 0.0 {
        return;
    }
    let root = discriminant.sqrt();
    push((-b + root) / (2.0 * a), count);
    push((-b - root) / (2.0 * a), count);
}

/// 폴더면 하위 폴더까지 훑고, 파일이면 그 파일 하나를 받는다.
fn collect_font_files(path: &Path, out: &mut Vec<PathBuf>) {
    if path.is_dir() {
        let Ok(entries) = std::fs::read_dir(path) else {
            return;
        };
        let mut children: Vec<PathBuf> = entries.flatten().map(|entry| entry.path()).collect();
        children.sort();
        for child in children {
            collect_font_files(&child, out);
        }
        return;
    }
    let extension = path
        .extension()
        .map(|extension| extension.to_string_lossy().to_ascii_lowercase());
    if matches!(extension.as_deref(), Some("ttf" | "otf" | "ttc" | "otc")) || is_hft_info(path) {
        out.push(path.to_path_buf());
    }
}

fn is_hft_info(path: &Path) -> bool {
    path.file_name()
        .is_some_and(|name| name.eq_ignore_ascii_case("hftinfo.dat"))
}

#[cfg(test)]
mod tests {
    use super::{EMBEDDED_ROOT, Embedding, FaceInfo, FontBook, key};
    use crate::subst::Rules;
    use hwp_core::{Blob, BlobKind, Document, FontFace};
    use std::path::{Path, PathBuf};

    fn face(path: &str, weight: f32, embedded: bool) -> FaceInfo {
        FaceInfo {
            path: PathBuf::from(path),
            index: 0,
            weight,
            embedding: Embedding::Installable,
            name: Some(path.to_string()),
            embedded,
        }
    }

    #[test]
    fn key_ignores_case_space_and_composition() {
        assert_eq!(key("HCR Batang"), "hcrbatang");
        assert_eq!(key("맑은 고딕"), key("맑은고딕"));
        assert_eq!(key("\u{1100}\u{1161}"), key("가"));
    }

    #[test]
    fn embedded_faces_win_across_all_candidate_names() {
        let mut book = FontBook::default();
        book.faces.insert(0, face("installed.ttf", 400.0, false));
        book.names.insert(key("Installed Face"), vec![0]);
        book.faces.insert(1, face("embedded.ttf", 400.0, true));
        book.names.insert(key("Embedded Face"), vec![1]);
        book.next_face = 2;

        let mut face = FontFace::new("Installed Face");
        face.default_name = Some("Embedded Face".to_string());
        let resolved = Rules::builtin()
            .resolve(&book, &face, "hangul", false)
            .expect("resolve");
        assert_eq!(resolved.0, 1);
    }

    #[test]
    fn installed_candidate_beats_an_embedded_fallback_name() {
        let mut book = FontBook::default();
        book.faces.insert(0, face("installed.ttf", 400.0, false));
        book.names.insert(key("함초롬바탕"), vec![0]);
        book.faces.insert(1, face("embedded.ttf", 400.0, true));
        book.names.insert(key("바탕"), vec![1]);
        book.next_face = 2;

        let resolved = Rules::builtin()
            .resolve(&book, &FontFace::new("함초롬바탕"), "hangul", false)
            .expect("resolve");
        assert_eq!(
            resolved.0, 0,
            "the exact installed candidate must beat an embedded fallback"
        );
    }

    #[test]
    fn embedded_path_keys_ignore_document_controlled_ids() {
        let bytes = std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../assets/fonts/LiberationSans-Regular.ttf"
        ))
        .expect("test font");
        let id = r"C:\Windows\evil.ttf";
        let mut document = Document::default();
        let mut face = FontFace::new("Liberation Sans");
        face.embedded = true;
        face.binary_ref = Some(id.to_string());
        document.font_faces.insert("hangul".to_string(), vec![face]);
        document.bins.push(Blob {
            id: id.to_string(),
            kind: BlobKind::Font,
            data: bytes.into(),
        });

        let mut book = FontBook::default();
        assert_eq!(book.attach_document(&document), 1);
        assert!(
            book.files
                .keys()
                .all(|path| path.starts_with(Path::new(EMBEDDED_ROOT)))
        );
        book.detach_document();
        assert!(book.files.is_empty());
    }
}
