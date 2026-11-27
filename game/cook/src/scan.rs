//! Static scan (FORK-PLAN §8.2 steps 1–2): every content id a place names,
//! in a property or in a script's `Source`, and where it was named.
//!
//! Content properties are found three ways, cheapest first: the value already
//! is a `Content` / `ContentId` variant; the reflection database declares the
//! property `Content` / `ContentId` but the file stored a plain string; or
//! the property name is one of the engine's known content names (the list
//! `crates/rustudio-player/src/asset_report.rs` follows, plus sounds and
//! animations). Scripts are scanned with a deliberately over-collecting
//! reader: a missed id is a broken game, an extra one is a cached file.

use std::collections::BTreeMap;
use std::path::Path;

use rbx_reflection::DataType;
use rustudio_core::__rbx::types::{Variant, VariantType};
use rustudio_core::doc::Document;

/// One place a content id is named.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Referrer {
    /// `Workspace.Wall.Decal.Texture`.
    Property(String),
    /// A line of a Script / LocalScript / ModuleScript.
    Script { path: String, line: u32 },
}

impl Referrer {
    pub fn describe(&self) -> String {
        match self {
            Referrer::Property(p) => format!("property {p}"),
            Referrer::Script { path, line } => format!("script {path} (line {line})"),
        }
    }
}

/// Every id the place names, in id order; each keeps its referrers.
#[derive(Debug, Default)]
pub struct Scan {
    pub refs: BTreeMap<String, Vec<Referrer>>,
}

/// The property names the engine reads as content even when a file stored
/// them as plain strings (the reflection database is the first check; this
/// list is the fallback for names it does not type, and for old files).
const CONTENT_NAMES: &[&str] = &[
    "AnimationId",
    "ColorMap",
    "ColorMapContent",
    "Face",
    "Graphic",
    "HoverImage",
    "Image",
    "ImageContent",
    "MeshContent",
    "MeshId",
    "MeshID",
    "MetalnessMap",
    "MoonTextureId",
    "NormalMap",
    "PantsTemplate",
    "PressedImage",
    "RoughnessMap",
    "ShirtTemplate",
    "SkyboxBk",
    "SkyboxDn",
    "SkyboxFt",
    "SkyboxLf",
    "SkyboxRt",
    "SkyboxUp",
    "SoundId",
    "SunTextureId",
    "Texture",
    "TextureContent",
    "TextureID",
    "TextureId",
];

/// Scans the document's DataModel. `doc_dir` is the place's folder, used for
/// script names only; content ids are not resolved here.
pub fn scan(doc: &Document) -> Scan {
    let mut out = Scan::default();
    for inst in doc.dom.descendants() {
        let path = full_name(doc, inst.referent());
        for (name, value) in &inst.properties {
            if let Some(id) = content_string(&inst.class, name.as_str(), value) {
                push(&mut out, id, Referrer::Property(format!("{path}.{name}")));
            }
            // A script's Source is string data that *names* content; the
            // provider returns every id form it can see.
            if name.as_str() == "Source" {
                if let Variant::String(source) = value {
                    scan_source(&path, source, &mut out);
                }
            }
        }
    }
    out
}

impl Scan {
    /// Adds one reference; the pack scan merges into the place's table.
    pub fn add(&mut self, id: String, referrer: Referrer) {
        push(self, id, referrer);
    }
}

fn push(out: &mut Scan, id: String, referrer: Referrer) {
    let refs = out.refs.entry(id).or_default();
    if !refs.contains(&referrer) {
        refs.push(referrer);
    }
}

fn content_string(class: &str, prop: &str, value: &Variant) -> Option<String> {
    let s = match value {
        Variant::Content(c) => c.as_uri()?.to_string(),
        Variant::ContentId(c) => c.as_str().to_string(),
        Variant::String(s) if CONTENT_NAMES.contains(&prop) || content_typed(class, prop) => s.clone(),
        _ => return None,
    };
    let s = s.trim().to_string();
    if !s.is_empty() && s.len() > 8 {
        // A Roblox asset URL stored as a plain string (old Animate models:
        // http://www.roblox.com/asset/?id=N) names the same asset as the
        // rbxassetid://N form; normalize so the report has one row, not a
        // "web URL" problem beside a cached asset.
        let lower = s.to_ascii_lowercase();
        if (lower.starts_with("http://") || lower.starts_with("https://"))
            && lower.contains("roblox.com")
        {
            if let Some(id) = ids_in_line(&s)
                .into_iter()
                .find(|id| id.starts_with("rbxassetid://"))
            {
                return Some(id);
            }
        }
    }
    (!s.is_empty()).then_some(s)
}

/// Whether the reflection database declares `class.prop` as a content
/// property (`Content` or `ContentId`), even if the file holds a string.
fn content_typed(class: &str, prop: &str) -> bool {
    use std::cell::RefCell;
    thread_local! {
        static CACHE: RefCell<BTreeMap<(String, String), bool>> = const { RefCell::new(BTreeMap::new()) };
    }
    let key = (class.to_string(), prop.to_string());
    if let Some(v) = CACHE.with(|c| c.borrow().get(&key).copied()) {
        return v;
    }
    let db = rustudio_core::doc::db();
    let typed = db.classes.get(class).is_some_and(|c| {
        db.superclasses_iter(c).any(|c| {
            c.properties.get(prop).is_some_and(|p| {
                matches!(p.data_type, DataType::Value(VariantType::Content | VariantType::ContentId))
            })
        })
    });
    CACHE.with(|c| c.borrow_mut().insert(key, typed));
    typed
}

/// Workspace.Model.Part, as GetFullName would print it (the service root is
/// left off).
fn full_name(doc: &Document, r: rustudio_core::__rbx::types::Ref) -> String {
    let root = doc.dom.root_ref();
    let mut names = Vec::new();
    let mut at = r;
    while at != root {
        let Some(inst) = doc.dom.get_by_ref(at) else { break };
        names.push(inst.name.clone());
        at = inst.parent();
    }
    names.reverse();
    names.join(".")
}

fn scan_source(path: &str, source: &str, out: &mut Scan) {
    for (i, line) in source.lines().enumerate() {
        for id in ids_in_line(line) {
            push(
                out,
                id,
                Referrer::Script {
                    path: path.to_string(),
                    line: i as u32 + 1,
                },
            );
        }
    }
}

/// The content ids a line of Luau names. Over-collects on purpose.
pub fn ids_in_line(line: &str) -> Vec<String> {
    let lower = line.to_ascii_lowercase();
    let mut ids: Vec<String> = Vec::new();

    // rbxassetid://N, with an optional ?version=V anywhere after it.
    let mut from = 0;
    while let Some(at) = lower[from..].find("rbxassetid://") {
        let start = from + at + "rbxassetid://".len();
        let rest = &line[start..];
        let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
        if !digits.is_empty() {
            let mut id = format!("rbxassetid://{digits}");
            if let Some(v) = version_after(rest) {
                id.push_str(&format!("?version={v}"));
            }
            ids.push(id);
        }
        from = start;
        if from >= lower.len() {
            break;
        }
    }

    // http://www.roblox.com/asset/?id=N and ...&id=N
    for pat in ["?id=", "&id="] {
        let mut from = 0;
        while let Some(at) = lower[from..].find(pat) {
            let start = from + at + pat.len();
            let digits: String = line[start..].chars().take_while(|c| c.is_ascii_digit()).collect();
            if !digits.is_empty() {
                ids.push(format!("rbxassetid://{digits}"));
            }
            from = start;
        }
    }

    // `assetId = 123` (also SoundId / AnimationId / MeshId / TextureId: a
    // literal; a variable is unknowable statically and is covered by the
    // runtime capture).
    for field in ["assetid", "animationid", "soundid", "meshid", "textureid"] {
        let mut from = 0;
        while let Some(at) = lower[from..].find(field) {
            let after = &line[from + at + field.len()..];
            let trimmed = after.trim_start();
            if let Some(eq) = trimmed.strip_prefix('=') {
                let digits: String = eq.trim_start().chars().take_while(|c| c.is_ascii_digit()).collect();
                if !digits.is_empty() {
                    ids.push(format!("rbxassetid://{digits}"));
                }
            }
            from += at + field.len();
        }
    }

    // rbxasset:// paths (they belong to the content pack, not the cache).
    let mut from = 0;
    while let Some(at) = lower[from..].find("rbxasset://") {
        let start = from + at + "rbxasset://".len();
        let path: String = line[start..]
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | '/'))
            .collect();
        if !path.is_empty() {
            ids.push(format!("rbxasset://{path}"));
        }
        from = start;
    }

    ids.sort();
    ids.dedup();
    ids
}

fn version_after(rest: &str) -> Option<String> {
    let at = rest.find("version=")? + "version=".len();
    let digits: String = rest[at..].chars().take_while(|c| c.is_ascii_digit()).collect();
    (!digits.is_empty()).then_some(digits)
}

/// Resolves a relative local path's file, for the bundle copy.
pub fn local_relative(id: &str) -> Option<&Path> {
    if id.starts_with("file://") || Path::new(id).is_absolute() {
        return None;
    }
    let p = Path::new(id);
    let inside = p
        .components()
        .all(|c| matches!(c, std::path::Component::Normal(_) | std::path::Component::CurDir));
    inside.then_some(p)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustudio_core::__rbx::types::ContentId;
    use rustudio_core::__rbx::{ustr, InstanceBuilder};

    fn document_with_script(source: &str) -> Document {
        let mut doc = Document::new_place();
        let ws = doc.workspace().unwrap();
        let script = doc
            .dom
            .insert(ws, InstanceBuilder::new("Script").with_name("Main"));
        doc.dom
            .get_by_ref_mut(script)
            .unwrap()
            .properties
            .insert(ustr("Source"), Variant::String(source.to_string()));
        doc
    }

    #[test]
    fn property_content_is_found_by_variant_and_by_name() {
        let mut doc = Document::new_place();
        let ws = doc.workspace().unwrap();
        let part = doc.dom.insert(ws, InstanceBuilder::new("Part").with_name("Wall"));
        let decal = doc.dom.insert(part, InstanceBuilder::new("Decal"));
        doc.dom.get_by_ref_mut(decal).unwrap().properties.insert(
            ustr("Texture"),
            Variant::ContentId(ContentId::from("rbxassetid://900000001")),
        );
        // Stored as a plain string (an old file): the property list catches it.
        let sound = doc.dom.insert(ws, InstanceBuilder::new("Sound").with_name("Loop"));
        doc.dom.get_by_ref_mut(sound).unwrap().properties.insert(
            ustr("SoundId"),
            Variant::String("rbxassetid://900000002".to_string()),
        );
        let scan = scan(&doc);
        assert!(scan.refs.contains_key("rbxassetid://900000001"), "{:?}", scan.refs);
        assert!(scan.refs.contains_key("rbxassetid://900000002"), "{:?}", scan.refs);
        let where_ = scan.refs["rbxassetid://900000001"][0].describe();
        assert_eq!(where_, "property Workspace.Wall.Decal.Texture");
    }

    #[test]
    fn script_ids_keep_their_line() {
        let doc = document_with_script("local a = 'rbxassetid://123'\n-- see http://www.roblox.com/asset/?id=456\nlocal b = Instance.new('Animation'); b.AnimationId = 789");
        let scan = scan(&doc);
        assert!(scan.refs.contains_key("rbxassetid://123"), "{:?}", scan.refs);
        assert!(scan.refs.contains_key("rbxassetid://456"), "{:?}", scan.refs);
        assert!(scan.refs.contains_key("rbxassetid://789"), "{:?}", scan.refs);
        assert_eq!(
            scan.refs["rbxassetid://789"][0],
            Referrer::Script { path: "Workspace.Main".into(), line: 3 }
        );
    }

    #[test]
    fn ids_in_line_reads_the_forms_scripts_use() {
        assert_eq!(ids_in_line("x = 'rbxassetid://42?version=7'"), vec!["rbxassetid://42?version=7"]);
        assert_eq!(ids_in_line("assetId   =   99"), vec!["rbxassetid://99"]);
        assert_eq!(ids_in_line("s.SoundId = 55"), vec!["rbxassetid://55"]);
        assert_eq!(ids_in_line("a.AnimationId = 789"), vec!["rbxassetid://789"]);
        assert_eq!(ids_in_line("'rbxasset://textures/wood.png'"), vec!["rbxasset://textures/wood.png"]);
        assert_eq!(ids_in_line("local n = 5"), Vec::<String>::new());
        assert_eq!(
            ids_in_line("'rbxassetid://1' .. tostring(2)"),
            vec!["rbxassetid://1"]
        );
    }

    #[test]
    fn a_roblox_asset_url_property_normalizes_to_the_asset_id() {
        let url = Variant::String("http://www.roblox.com/asset/?id=180435571".into());
        assert_eq!(
            content_string("Animation", "AnimationId", &url),
            Some("rbxassetid://180435571".into())
        );
        let web = Variant::String("https://example.com/asset/?id=5".into());
        assert_eq!(
            content_string("Decal", "Texture", &web),
            Some("https://example.com/asset/?id=5".into())
        );
    }

    #[test]
    fn local_relative_refuses_absolutes_and_parent_paths() {
        assert_eq!(local_relative("textures/wood.png"), Some(Path::new("textures/wood.png")));
        assert_eq!(local_relative("./textures/wood.png"), Some(Path::new("./textures/wood.png")));
        assert_eq!(local_relative("../wood.png"), None);
        assert_eq!(local_relative("file:///tmp/wood.png"), None);
    }
}
