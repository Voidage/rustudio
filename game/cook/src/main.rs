//! rustudio game cook (FORK-PLAN §8.2).
//!
//! Inputs: a place, the content roots, and an output bundle directory.
//!
//! 1. Static scan: every content id in a `Content` / `ContentId` property
//!    (through the reflection database, with the names
//!    `crates/rustudio-player/src/asset_report.rs` follows as fallback) and
//!    every id a script's `Source` names.
//! 2. Runtime capture: plays the place with a clean `RUSTUDIO_CACHE_DIR`
//!    (the bundle's `cache/`) and no local store, so everything Play asks
//!    for lands in the staging cache — including ids only built at run time.
//! 3. Fetch missing: resolves scanned ids the playthrough did not touch
//!    through the engine's asset path (public delivery anonymously, or the
//!    saved session with `--signed-in`).
//! 4. Report: `<out>/assets.json` with id, status, size, sha256, where it
//!    was named, and the pieces that are not in the cache.
//!
//! Local files (`file://` or paths relative to the place) are copied into
//! `<out>/place/` beside the place, with the `.rustudio-local-content`
//! marker the engine requires during Play. `rbxasset://` ids belong to the
//! content pack and are only checked, never copied. `rbxthumb://` and web
//! URLs are reported as not cookable. An absolute local path cannot be
//! bundled (the id would still point at the old file): the cook fails.
//!
//! The cook is a developer tool and may depend on upstream crates; the
//! launcher deliberately does not (FORK-PLAN §3).

mod cache;
mod scan;

use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::Serialize;

use rustudio_core::assets::{self, AssetRef, AssetStore};
use rustudio_core::doc::Document;

use scan::Referrer;

const USAGE: &str = "\
usage: rustudio-cook --place FILE --out DIR [options]
       rustudio-cook export --place FILE --out DIR [options]

  --place FILE       place to cook (.rbxl / .rbxlx)
  --out DIR          bundle directory to fill (cache/, place/, assets.json)
  --report FILE      report path (default: <out>/assets.json)
  --content DIR      engine content root for rbxasset:// ids
  --local-assets DIR offline asset library to import missing ids from
                     (<id>.<ext>, as RUSTUDIO_LOCAL_ASSETS uses)
  --player FILE      rustudio-player for the runtime capture
                     (default: $RUSTUDIO_PLAYER, else target/{release,debug}
                     above the current directory)
  --seconds N        seconds to play for the capture (default 8)
  --no-capture       static scan + fetch only, no playthrough
  --signed-in        use the saved Roblox session for private assets
  --allow-missing    exit 0 even when some assets could not be cooked
  -h, --help         this text

export: 'Save assets for offline use'. Copies every asset the capture ledger
records for the place out of the asset cache into <out>/cache, writes
<out>/offline-assets.json with hashes, and reports ids that failed or whose
cache file is gone. No network, no playthrough.

  --place FILE       the place those runs opened
  --out DIR          bundle directory to fill (cache/, offline-assets.json)
  --ledger FILE      capture ledger (default: $RUSTUDIO_CAPTURE_LEDGER, else
                     <cache>/capture-ledger.jsonl)
  --all              export every place's captures in the ledger
  --allow-missing    exit 0 even when some assets could not be exported
  -h, --help         this text";

#[derive(Debug)]
struct Options {
    place: PathBuf,
    out: PathBuf,
    report: Option<PathBuf>,
    content: Option<PathBuf>,
    local_assets: Option<PathBuf>,
    player: Option<PathBuf>,
    seconds: f64,
    capture: bool,
    signed_in: bool,
    allow_missing: bool,
}

fn main() {
    let mut args: Vec<String> = env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("export") {
        args.remove(0);
        let options = match parse_export_args(args) {
            Ok(Some(options)) => options,
            Ok(None) => return,
            Err(error) => {
                eprintln!("cook export: {error}\n\n{USAGE}");
                std::process::exit(2);
            }
        };
        match export(&options) {
            Ok(code) => std::process::exit(code),
            Err(error) => {
                eprintln!("cook export: {error}");
                std::process::exit(2);
            }
        }
    }
    let options = match parse_args(args) {
        Ok(Some(options)) => options,
        Ok(None) => return,
        Err(error) => {
            eprintln!("cook: {error}\n\n{USAGE}");
            std::process::exit(2);
        }
    };
    match cook(&options) {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            eprintln!("cook: {error}");
            std::process::exit(2);
        }
    }
}

/// `rustudio-cook export` (FORK-PLAN §8.4 item 3): "Save assets for offline
/// use" — the capture ledger's entries for a place, copied into a bundle.
#[derive(Debug)]
struct ExportOptions {
    place: PathBuf,
    out: PathBuf,
    ledger: Option<PathBuf>,
    all: bool,
    allow_missing: bool,
}

fn parse_export_args(args: Vec<String>) -> Result<Option<ExportOptions>, String> {
    let mut place = None;
    let mut out = None;
    let mut ledger = None;
    let mut all = false;
    let mut allow_missing = false;
    let mut it = args.into_iter();
    while let Some(arg) = it.next() {
        let mut value = |flag: &str| it.next().ok_or_else(|| format!("{flag} needs a value"));
        match arg.as_str() {
            "-h" | "--help" => {
                println!("{USAGE}");
                return Ok(None);
            }
            "--place" => place = Some(PathBuf::from(value("--place")?)),
            "--out" => out = Some(PathBuf::from(value("--out")?)),
            "--ledger" => ledger = Some(PathBuf::from(value("--ledger")?)),
            "--all" => all = true,
            "--allow-missing" => allow_missing = true,
            other => return Err(format!("unknown option {other}")),
        }
    }
    Ok(Some(ExportOptions {
        place: place.ok_or("--place is required")?,
        out: out.ok_or("--out is required")?,
        ledger,
        all,
        allow_missing,
    }))
}

/// One line of `<cache>/capture-ledger.jsonl` (written by
/// `rustudio_core::assets::capture_record`).
#[derive(Debug, serde::Deserialize)]
struct LedgerLine {
    #[serde(default)]
    time: u64,
    #[serde(default)]
    place: String,
    id: u64,
    #[serde(default)]
    version: Option<u64>,
    status: String,
    #[serde(default)]
    path: String,
    #[serde(default)]
    size: u64,
}

#[derive(Debug, Serialize)]
struct OfflineReport {
    manifest_version: u32,
    kind: &'static str,
    place: String,
    ledger: String,
    summary: BTreeMap<String, u32>,
    assets: Vec<OfflineRow>,
}

#[derive(Debug, Serialize)]
struct OfflineRow {
    id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    version: Option<u64>,
    status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    size: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sha256: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    ledger_time: Option<u64>,
}

fn export(options: &ExportOptions) -> Result<i32, String> {
    let place = options
        .place
        .canonicalize()
        .map_err(|e| format!("{}: {e}", options.place.display()))?;
    let place_key = place.display().to_string();
    let ledger_path = options
        .ledger
        .clone()
        .unwrap_or_else(assets::capture_ledger_path);
    let text = fs::read_to_string(&ledger_path)
        .map_err(|e| format!("{}: {e} (play the place first)", ledger_path.display()))?;
    let cache = assets::cache_dir();
    let bundle_cache = options.out.join("cache");
    fs::create_dir_all(&options.out).map_err(|e| format!("{}: {e}", options.out.display()))?;

    // (id, version) -> the entry the export will ship or report.
    #[derive(Default)]
    struct Seen {
        cached: Option<LedgerLine>,
        failed_time: Option<u64>,
        failed_size: u64,
    }
    let mut seen: BTreeMap<(u64, Option<u64>), Seen> = BTreeMap::new();
    let mut lines = 0usize;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let Ok(entry) = serde_json::from_str::<LedgerLine>(line) else { continue };
        lines += 1;
        if !options.all && entry.place != place_key {
            continue;
        }
        let slot = seen.entry((entry.id, entry.version)).or_default();
        match entry.status.as_str() {
            "cached" => {
                // A later capture of the same id+version replaces it.
                let newer = slot
                    .cached
                    .as_ref()
                    .is_none_or(|old| entry.time >= old.time);
                if newer {
                    slot.cached = Some(entry);
                }
            }
            "failed" => {
                let newer = slot.failed_time.is_none_or(|old| entry.time >= old);
                if newer {
                    slot.failed_time = Some(entry.time);
                    slot.failed_size = entry.size;
                }
            }
            _ => {}
        }
    }
    if lines == 0 {
        return Err(format!("{}: no ledger entries", ledger_path.display()));
    }

    let mut rows = Vec::new();
    let mut summary: BTreeMap<String, u32> = BTreeMap::new();
    for ((id, version), slot) in &seen {
        let asset = AssetRef { id: *id, version: *version };
        let id_str = match version {
            Some(v) => format!("rbxassetid://{id}?version={v}"),
            None => format!("rbxassetid://{id}"),
        };
        let mut row = OfflineRow {
            id: id_str,
            version: *version,
            status: "missing",
            file: None,
            size: None,
            sha256: None,
            ledger_time: None,
        };
        if let Some(line) = &slot.cached {
            row.ledger_time = Some(line.time);
            // The recorded path first (it is what the play wrote); rebuilt
            // from id+version when the ledger names a path this cache does
            // not have (a different cache layout across versions).
            let recorded = cache.join(&line.path);
            let source = if recorded.is_file() {
                recorded
            } else {
                cache::expected(&cache, asset)
            };
            if source.is_file() {
                let dest = cache::expected(&bundle_cache, asset);
                if let Some(dir) = dest.parent() {
                    fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
                }
                fs::copy(&source, &dest)
                    .map_err(|e| format!("{} -> {}: {e}", source.display(), dest.display()))?;
                let size = fs::metadata(&dest).map(|m| m.len()).unwrap_or(line.size);
                let sha256 = cache::sha256_file(&dest)?;
                let file = dest
                    .strip_prefix(&options.out)
                    .unwrap_or(&dest)
                    .display()
                    .to_string();
                row.status = "exported";
                row.file = Some(file);
                row.size = Some(size);
                row.sha256 = Some(sha256);
            }
        } else if slot.failed_time.is_some() {
            row.status = "failed";
            row.ledger_time = slot.failed_time;
            row.size = (slot.failed_size > 0).then_some(slot.failed_size);
        }
        *summary.entry(row.status.to_string()).or_default() += 1;
        rows.push(row);
    }

    let report = OfflineReport {
        manifest_version: 1,
        kind: "offline-assets",
        place: place_key.clone(),
        ledger: ledger_path.display().to_string(),
        summary,
        assets: rows,
    };
    let report_path = options.out.join("offline-assets.json");
    let json = serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?;
    fs::write(&report_path, json).map_err(|e| format!("{}: {e}", report_path.display()))?;

    let missing: u32 = ["missing", "failed"]
        .iter()
        .filter_map(|k| report.summary.get(*k))
        .sum();
    println!(
        "export: {} asset(s) for {}: {}",
        report.assets.len(),
        place.display(),
        report
            .summary
            .iter()
            .map(|(k, v)| format!("{v} {k}"))
            .collect::<Vec<_>>()
            .join(", ")
    );
    println!("export: report {}", report_path.display());
    if missing > 0 {
        for row in report.assets.iter().filter(|r| r.status != "exported").take(20) {
            println!("export: {} {}", row.status, row.id);
        }
        if !options.allow_missing {
            return Ok(1);
        }
    }
    Ok(0)
}

fn parse_args(args: Vec<String>) -> Result<Option<Options>, String> {
    let mut place = None;
    let mut out = None;
    let mut report = None;
    let mut content = None;
    let mut local_assets = None;
    let mut player = None;
    let mut seconds: f64 = 8.0;
    let mut capture = true;
    let mut signed_in = false;
    let mut allow_missing = false;
    let mut it = args.into_iter();
    while let Some(arg) = it.next() {
        let mut value = |flag: &str| it.next().ok_or_else(|| format!("{flag} needs a value"));
        match arg.as_str() {
            "-h" | "--help" => {
                println!("{USAGE}");
                return Ok(None);
            }
            "--place" => place = Some(PathBuf::from(value("--place")?)),
            "--out" => out = Some(PathBuf::from(value("--out")?)),
            "--report" => report = Some(PathBuf::from(value("--report")?)),
            "--content" => content = Some(PathBuf::from(value("--content")?)),
            "--local-assets" => local_assets = Some(PathBuf::from(value("--local-assets")?)),
            "--player" => player = Some(PathBuf::from(value("--player")?)),
            "--seconds" => {
                seconds = value("--seconds")?.parse().map_err(|_| "--seconds: a number expected".to_string())?;
                if seconds <= 0.0 {
                    return Err("--seconds must be positive".into());
                }
            }
            "--no-capture" => capture = false,
            "--signed-in" => signed_in = true,
            "--allow-missing" => allow_missing = true,
            other => return Err(format!("unknown option {other}")),
        }
    }
    Ok(Some(Options {
        place: place.ok_or("--place is required")?,
        out: out.ok_or("--out is required")?,
        report,
        content,
        local_assets,
        player,
        seconds,
        capture,
        signed_in,
        allow_missing,
    }))
}

/// What an id names, once it is classified.
#[derive(Debug, Clone)]
enum Class {
    /// A Roblox cloud asset: `<id>_v<version|latest>.bin` in the cache.
    Cloud { asset: AssetRef, legacy: bool },
    /// A file inside the engine content pack (not copied here).
    ContentPack(PathBuf),
    /// `rbxasset://` with no file in the pack and no legacy id.
    MissingContent,
    /// A local file (copied into `place/` when it is relative).
    Local { id: String, path: Option<PathBuf> },
    /// `rbxtemp://`: generated at run time, not a bundle asset.
    Temporary,
    /// A live web thumbnail (never available offline).
    Thumbnail,
    /// Any other web URL.
    Web,
    /// A scheme the cook does not know.
    Unsupported,
}

fn cook(options: &Options) -> Result<i32, String> {
    let place = options
        .place
        .canonicalize()
        .map_err(|e| format!("{}: {e}", options.place.display()))?;
    if !place.is_file() {
        return Err(format!("{} is not a file", place.display()));
    }
    let place_dir = place.parent().map(Path::to_path_buf).unwrap_or_default();
    fs::create_dir_all(&options.out).map_err(|e| format!("{}: {e}", options.out.display()))?;
    if let Some(content) = &options.content {
        env::set_var("RUSTUDIO_CONTENT_DIR", content);
    }
    // A clean cache is the source of truth (§8.2.3): the playthrough must
    // not inherit bytes from an earlier cook or the editor's cache.
    let cache = options.out.join("cache");
    if cache.exists() {
        fs::remove_dir_all(&cache).map_err(|e| format!("{}: {e}", cache.display()))?;
    }
    fs::create_dir_all(cache.join("assets")).map_err(|e| format!("{}: {e}", cache.display()))?;
    env::set_var("RUSTUDIO_CACHE_DIR", &cache);
    env::remove_var("RUSTUDIO_LOCAL_ASSETS");

    println!("cook: opening {}", place.display());
    let doc = Document::open(&place).map_err(|e| format!("{}: {e}", place.display()))?;
    let scan = scan::scan(&doc);
    println!("cook: static scan found {} content id(s)", scan.refs.len());

    let place_rel = copy_place(&place, &options.out)?;

    let mut problems = Vec::new();
    let mut capture = None;
    let mut captured = BTreeMap::new();
    if options.capture {
        let player = find_player(options.player.clone())
            .ok_or("no rustudio-player found: pass --player, set RUSTUDIO_PLAYER, or build target/release/rustudio-player (--no-capture skips the playthrough)")?;
        let info = run_capture(&player, &place, options)?;
        if info.exit != 0 {
            problems.push(format!(
                "the capture playthrough exited with {} — see {}",
                info.exit,
                info.log.display()
            ));
        }
        println!("cook: played {}s for the capture", options.seconds);
        captured = cache::entries(&cache);
        capture = Some(info);
    }

    let session = if options.signed_in { open_session() } else { None };

    // Classify everything the scan found, then fetch the cloud ids the
    // playthrough did not provide.
    let mut rows: BTreeMap<String, Row> = BTreeMap::new();
    let mut missing: Vec<AssetRef> = Vec::new();
    for (id, refs) in &scan.refs {
        let class = classify(id, &place_dir);
        let mut row = Row::new(id.clone(), class_name(&class), refs);
        // Cloud ids are keyed by their normalised form so `42` and
        // `rbxassetid://42` share a row, and the fetch pass finds it.
        let key = match &class {
            Class::Cloud { asset, .. } => cloud_key(*asset),
            _ => id.clone(),
        };
        match class {
            Class::Cloud { asset, legacy } => {
                row.asset_id = Some(asset.id);
                row.version = asset.version;
                if captured.contains_key(&(asset.id, asset.version)) {
                    row.status = "captured".into();
                    row.fill_cache(&options.out, &captured[&(asset.id, asset.version)]);
                } else if let Some(found) = cache::entry(&cache, asset) {
                    row.status = "cached".into();
                    row.fill_cache(&options.out, &found);
                } else {
                    row.why = legacy.then(|| "rbxasset:// id with no content-pack file; comes from the legacy table".into());
                    missing.push(asset);
                }
            }
            Class::ContentPack(path) => {
                row.status = "content-pack".into();
                row.why = Some(format!("shipped by the content pack ({})", path.display()));
            }
            Class::MissingContent => {
                row.status = "missing-content".into();
                row.why = Some("rbxasset:// id with no file in the content pack and no legacy asset id".into());
                problems.push(format!("{id}: missing content-pack file"));
            }
            Class::Local { id: local, path } => {
                match path {
                    Some(source) if source.is_file() => match scan::local_relative(&local) {
                        Some(rel) => {
                            let dest = options.out.join("place").join(rel);
                            if let Some(dir) = dest.parent() {
                                fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
                            }
                            fs::copy(&source, &dest)
                                .map_err(|e| format!("{}: {e}", source.display()))?;
                            row.status = "local".into();
                            row.why = Some(format!("copied to place/{}", rel.display()));
                            row.size = dest.metadata().ok().map(|m| m.len());
                            row.sha256 = cache::sha256_file(&dest).ok();
                        }
                        None => {
                            row.status = "unsupported".into();
                            row.why = Some(
                                "absolute local path: an id that still names the old file can't be bundled — use a path relative to the place or a cloud id"
                                    .into(),
                            );
                            problems.push(format!("{id}: absolute local path can't be bundled"));
                        }
                    },
                    Some(source) => {
                        row.status = "failed".into();
                        row.why = Some("local file not found".into());
                        problems.push(format!("{id}: {} not found", source.display()));
                    }
                    None => {
                        row.status = "failed".into();
                        row.why = Some("local path does not resolve".into());
                        problems.push(format!("{id}: local path does not resolve"));
                    }
                }
            }
            Class::Temporary => {
                row.status = "runtime".into();
                row.why = Some("generated at run time (rbxtemp://); not a bundled asset".into());
            }
            Class::Thumbnail => {
                row.status = "live-thumbnail".into();
                row.why = Some("rbxthumb:// is a live web image; it does not work offline".into());
                problems.push(format!("{id}: live web thumbnail, not cookable"));
            }
            Class::Web => {
                row.status = "unsupported".into();
                row.why = Some("http(s) content is not cookable by default".into());
                problems.push(format!("{id}: web URL, not cookable"));
            }
            Class::Unsupported => {
                row.status = "unsupported".into();
                row.why = Some("unsupported content scheme".into());
                problems.push(format!("{id}: unsupported content scheme"));
            }
        }
        if let Some(existing) = rows.get_mut(&key) {
            // The same asset named twice: keep one row, every reference.
            existing.refs.extend(row.refs);
        } else {
            rows.insert(key, row);
        }
    }

    if !missing.is_empty() {
        println!("cook: fetching {} scanned asset(s) the playthrough did not request", missing.len());
        let results = fetch_missing(missing, session, options.local_assets.as_deref());
        let now = cache::entries(&cache);
        for (asset, result) in results {
            let id = cloud_key(asset);
            if let Some(row) = rows.get_mut(&id) {
                match result {
                    Ok(source) => {
                        row.status = match source {
                            FetchSource::Downloaded => "downloaded",
                            FetchSource::Imported => "imported",
                        }
                        .into();
                        if source == FetchSource::Imported {
                            row.why = Some("imported from the local asset store".into());
                        }
                        if let Some(entry) = now.get(&(asset.id, asset.version)) {
                            row.fill_cache(&options.out, entry);
                        }
                    }
                    Err(why) => {
                        row.status = "failed".into();
                        row.why = Some(why.clone());
                        problems.push(format!("{id}: {why}"));
                    }
                }
            }
        }
    }

    // Runtime-captured ids the static scan never named (built in a script).
    for entry in captured.values() {
        let key = cloud_key(entry.asset);
        if rows.contains_key(&key) {
            continue;
        }
        let mut row = Row::new(key.clone(), "runtime", &[]);
        row.asset_id = Some(entry.asset.id);
        row.version = entry.asset.version;
        row.status = "captured".into();
        row.why = Some("requested during the playthrough; no static reference".into());
        row.fill_cache(&options.out, entry);
        rows.insert(key, row);
    }

    // A zero-byte cache file is as broken as a missing one.
    for row in rows.values_mut() {
        let cached = matches!(row.status.as_str(), "captured" | "cached" | "downloaded" | "imported" | "local");
        if row.size == Some(0) && cached {
            row.status = "failed".into();
            row.why = Some("the cached file is empty".into());
            problems.push(format!("{}: the cached file is empty", row.id));
        }
    }

    let report_path = options.report.clone().unwrap_or_else(|| options.out.join("assets.json"));
    let report = Report {
        manifest_version: 1,
        place: place_rel,
        source_place: place.display().to_string(),
        capture,
        summary: summary(&rows),
        assets: rows.into_values().collect(),
        problems: problems.clone(),
    };
    let json = serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?;
    if let Some(dir) = report_path.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    fs::write(&report_path, json).map_err(|e| format!("{}: {e}", report_path.display()))?;

    println!("cook: {}", report.summary_line());
    println!("cook: report {}", report_path.display());
    if !problems.is_empty() {
        println!("cook: {} problem(s):", problems.len());
        for p in &problems {
            println!("  - {p}");
        }
        if !options.allow_missing {
            return Ok(1);
        }
    }
    Ok(0)
}

fn classify(id: &str, place_dir: &Path) -> Class {
    if id.starts_with("rbxtemp://") {
        return Class::Temporary;
    }
    if id.starts_with("rbxthumb://") {
        return Class::Thumbnail;
    }
    if id.starts_with("http://") || id.starts_with("https://") {
        return Class::Web;
    }
    if id.starts_with("rbxasset://") {
        if let Some(path) = assets::resolve(id, Some(place_dir)) {
            if path.is_file() {
                return Class::ContentPack(path);
            }
        }
        return match assets::asset_ref(id) {
            Some(asset) => Class::Cloud { asset, legacy: true },
            None => Class::MissingContent,
        };
    }
    if assets::is_local_path(id) {
        return Class::Local {
            id: id.to_string(),
            path: assets::resolve(id, Some(place_dir)),
        };
    }
    match assets::asset_ref(id) {
        Some(asset) => Class::Cloud { asset, legacy: false },
        None => Class::Unsupported,
    }
}

fn class_name(class: &Class) -> &'static str {
    match class {
        Class::Cloud { .. } => "cloud",
        Class::ContentPack(_) => "content-pack",
        Class::MissingContent => "content-pack",
        Class::Local { .. } => "local",
        Class::Temporary => "runtime",
        Class::Thumbnail => "thumbnail",
        Class::Web => "web",
        Class::Unsupported => "unsupported",
    }
}

fn cloud_key(asset: AssetRef) -> String {
    match asset.version {
        Some(v) => format!("rbxassetid://{}?version={v}", asset.id),
        None => format!("rbxassetid://{}", asset.id),
    }
}

/// Where a missing asset's bytes came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FetchSource {
    Downloaded,
    Imported,
}

/// Fetches through the engine's asset path: the local store first when the
/// cook was given one (FORK-PLAN §8.4's library mirror), then public
/// delivery, or the developer's session when given.
fn fetch_missing(
    assets_to_get: Vec<AssetRef>,
    session: Option<assets::SharedClient>,
    local_assets: Option<&Path>,
) -> Vec<(AssetRef, Result<FetchSource, String>)> {
    let cache_dir = assets::cache_dir();
    let mut results = Vec::new();
    let mut remote = Vec::new();
    for asset in assets_to_get {
        match local_assets.and_then(|dir| local_store_file(dir, asset.id)) {
            Some(path) => {
                let dest = cache::expected(&cache_dir, asset);
                match fs::copy(&path, &dest) {
                    Ok(_) => results.push((asset, Ok(FetchSource::Imported))),
                    Err(error) => results.push((asset, Err(format!("{}: {error}", path.display())))),
                }
            }
            None => remote.push(asset),
        }
    }
    if !remote.is_empty() {
        let mut store = AssetStore::default();
        store.set_client(session);
        for asset in &remote {
            let _ = store.fetch(*asset);
        }
        if store.pending_count() > 0 {
            eprintln!("cook: waiting for {} download(s)", store.pending_count());
            store.wait_pending(Duration::from_secs(300));
        }
        store.poll();
        for asset in remote {
            if cache::expected(&cache_dir, asset).is_file() {
                results.push((asset, Ok(FetchSource::Downloaded)));
            } else {
                let why = store
                    .why_not(&cloud_key(asset))
                    .unwrap_or_else(|| "the download did not produce a cache file".into());
                results.push((asset, Err(why)));
            }
        }
    }
    results
}

/// The local store's file for an id (`<id>` or `<id>.<ext>`, the names
/// `RUSTUDIO_LOCAL_ASSETS` uses).
fn local_store_file(dir: &Path, id: u64) -> Option<PathBuf> {
    const EXTS: &[&str] = &[
        "", ".mesh", ".png", ".jpg", ".dds", ".wav", ".ogg", ".mp3", ".webm", ".rbxm", ".rbxmx",
        ".anim.rbxm", ".decal.rbxmx", ".fbx",
    ];
    EXTS.iter().map(|ext| dir.join(format!("{id}{ext}"))).find(|path| path.is_file())
}

fn open_session() -> Option<assets::SharedClient> {
    let config = match rustudio_account::AccountConfig::load() {
        Ok(config) => config,
        Err(error) => {
            eprintln!("cook: --signed-in: can't load the account config: {error}");
            return None;
        }
    };
    match rustudio_account::Client::open(config) {
        Ok(client) => {
            let shared: assets::SharedClient = Arc::new(Mutex::new(Some(client)));
            assets::session_ready(Some(shared.clone()));
            Some(shared)
        }
        Err(error) => {
            eprintln!("cook: --signed-in: {}", error.describe());
            None
        }
    }
}

fn find_player(explicit: Option<PathBuf>) -> Option<PathBuf> {
    let with_exe = |base: &str| format!("{base}{}", env::consts::EXE_SUFFIX);
    let candidates = [
        with_exe("target/release/rustudio-player"),
        with_exe("target/debug/rustudio-player"),
    ];
    if let Some(path) = explicit.or_else(|| env::var_os("RUSTUDIO_PLAYER").map(PathBuf::from)) {
        return path.is_file().then_some(path);
    }
    let mut dir = env::current_dir().ok()?;
    loop {
        for candidate in &candidates {
            let path = dir.join(candidate);
            if path.is_file() {
                return Some(path);
            }
        }
        if !dir.pop() {
            return None;
        }
    }
}

#[derive(Debug, Serialize)]
struct CaptureInfo {
    seconds: f64,
    player: PathBuf,
    log: PathBuf,
    exit: i32,
}

fn run_capture(player: &Path, place: &Path, options: &Options) -> Result<CaptureInfo, String> {
    let log = options.out.join("logs").join("cook-play.log");
    if let Some(dir) = log.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let mut cmd = Command::new(player);
    cmd.arg(place)
        .arg("--headless")
        .arg("--seconds")
        .arg(format!("{}", options.seconds))
        .arg("--log")
        .arg(&log)
        .env("RUSTUDIO_CACHE_DIR", options.out.join("cache"))
        // The capture's ledger is evidence for the cook, not a bundle asset.
        .env("RUSTUDIO_CAPTURE_LEDGER", options.out.join("logs").join("cook-capture-ledger.jsonl"))
        .env("RUSTUDIO_NO_STUDIO_FETCH", "1")
        // Capture as a shipped game runs: no saved avatar/session, so the
        // game's own local rig (R15 or R6), head and Animate clips are what
        // gets cached. `--signed-in` still covers scanned private assets in
        // the fetch step.
        .env("RUSTUDIO_NO_SESSION", "1")
        .env("RUSTUDIO_CORE_SCRIPTS", "0")
        .env("RUSTUDIO_LIVE_DATA", "0")
        .env_remove("RUSTUDIO_LOCAL_ASSETS")
        .stdout(Stdio::null())
        .stderr(Stdio::inherit());
    if let Some(content) = &options.content {
        // `--content` names the pack root (content/ + ExtraContent/, as the
        // launcher's RUSTUDIO_ENGINE_CONTENT_DIR); a bare Studio content/
        // folder still works. Using the pack here makes the capture request
        // the same avatar and Animate assets the shipped game will read.
        if content.join("content").is_dir() || content.join("ExtraContent").is_dir() {
            cmd.env("RUSTUDIO_ENGINE_CONTENT_DIR", content);
        } else {
            cmd.env("RUSTUDIO_CONTENT_DIR", content);
        }
    }
    println!("cook: capture: {} --headless --seconds {}", player.display(), options.seconds);
    let mut child = cmd.spawn().map_err(|e| format!("{}: {e}", player.display()))?;
    let start = Instant::now();
    // The player ends itself after --seconds; this is only a hang guard.
    let limit = Duration::from_secs_f64(options.seconds + 120.0);
    let exit = loop {
        match child.try_wait().map_err(|e| e.to_string())? {
            Some(status) => break status.code().unwrap_or(1),
            None if start.elapsed() > limit => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("the player did not exit within {limit:?}"));
            }
            None => std::thread::sleep(Duration::from_millis(100)),
        }
    };
    Ok(CaptureInfo {
        seconds: options.seconds,
        player: player.to_path_buf(),
        log,
        exit,
    })
}

fn copy_place(place: &Path, out: &Path) -> Result<String, String> {
    let name = place
        .file_name()
        .ok_or_else(|| format!("{}: no file name", place.display()))?;
    let dir = out.join("place");
    fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let dest = dir.join(name);
    if dest != place {
        fs::copy(place, &dest).map_err(|e| format!("{}: {e}", place.display()))?;
    }
    // Any local file the scan copies into place/ needs the marker that lets
    // Play read files from the opened place's folder (assets.rs).
    let marker = dir.join(assets::LOCAL_CONTENT_MARKER);
    if !marker.exists() {
        let _ = fs::write(&marker, "Cooked local content is place content during Play.\n");
    }
    Ok(format!("place/{}", name.to_string_lossy()))
}

#[derive(Debug, Serialize)]
struct Report {
    manifest_version: u32,
    /// The place's path inside the bundle.
    place: String,
    /// The place that was cooked.
    source_place: String,
    capture: Option<CaptureInfo>,
    summary: BTreeMap<String, u32>,
    assets: Vec<Row>,
    problems: Vec<String>,
}

impl Report {
    fn summary_line(&self) -> String {
        let parts: Vec<String> = self.summary.iter().map(|(k, v)| format!("{v} {k}")).collect();
        format!("{} asset(s): {}", self.assets.len(), parts.join(", "))
    }
}

#[derive(Debug, Serialize)]
struct Row {
    id: String,
    kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    asset_id: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    version: Option<u64>,
    status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    why: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    size: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sha256: Option<String>,
    /// The cache file, relative to the bundle.
    #[serde(skip_serializing_if = "Option::is_none")]
    cache: Option<String>,
    refs: Vec<String>,
}

impl Row {
    fn new(id: String, kind: &'static str, refs: &[Referrer]) -> Self {
        Row {
            id,
            kind,
            asset_id: None,
            version: None,
            status: "pending".into(),
            why: None,
            size: None,
            sha256: None,
            cache: None,
            refs: refs.iter().map(Referrer::describe).collect(),
        }
    }

    fn fill_cache(&mut self, out: &Path, entry: &cache::CacheEntry) {
        self.size = Some(entry.size);
        self.sha256 = Some(entry.sha256.clone());
        self.cache = entry.path.strip_prefix(out).ok().map(|p| p.display().to_string());
    }
}

fn summary(rows: &BTreeMap<String, Row>) -> BTreeMap<String, u32> {
    let mut out = BTreeMap::new();
    for row in rows.values() {
        *out.entry(row.status.clone()).or_default() += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `cook` and `export` read the process-wide cache variables; tests that
    /// use either take this first.
    static ENV: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn classification_covers_the_id_forms() {
        let dir = Path::new("/nonexistent-place");
        assert!(matches!(
            classify("rbxassetid://42?version=7", dir),
            Class::Cloud { asset: AssetRef { id: 42, version: Some(7) }, legacy: false }
        ));
        assert!(matches!(
            classify("42", dir),
            Class::Cloud { asset: AssetRef { id: 42, version: None }, legacy: false }
        ));
        assert!(matches!(classify("rbxtemp://kfs/1", dir), Class::Temporary));
        assert!(matches!(classify("rbxthumb://type=Asset&id=1", dir), Class::Thumbnail));
        assert!(matches!(classify("https://example.com/a.png", dir), Class::Web));
        assert!(matches!(classify("textures/wood.png", dir), Class::Local { .. }));
        assert!(matches!(classify("file:///tmp/wood.png", dir), Class::Local { .. }));
        assert!(matches!(classify("not-a-real-scheme://x", dir), Class::Unsupported));
    }

    #[test]
    fn args_require_place_and_out() {
        assert!(parse_args(vec!["--place".into(), "a.rbxlx".into()]).is_err());
        let options = parse_args(vec![
            "--place".into(),
            "a.rbxlx".into(),
            "--out".into(),
            "out".into(),
            "--seconds".into(),
            "5".into(),
        ])
        .unwrap()
        .unwrap();
        assert_eq!(options.seconds, 5.0);
        assert!(options.capture);
        assert!(parse_args(vec!["--place".into(), "a".into(), "--out".into(), "o".into(), "--seconds".into(), "0".into()]).is_err());
    }

    #[test]
    fn report_rows_are_stable_json() {
        let row = Row::new("rbxassetid://1".into(), "cloud", &[]);
        let json = serde_json::to_value(&row).unwrap();
        assert_eq!(json["id"], "rbxassetid://1");
        assert!(json.get("asset_id").is_none());
        assert_eq!(json["refs"], serde_json::json!([]));
    }

    /// End to end without a player or the network: a place naming one cloud
    /// id, an offline library holding its bytes, and the bundle the cook
    /// writes from them.
    #[test]
    fn cook_imports_from_the_local_store_and_writes_a_report() {
        let _env = ENV.lock().unwrap_or_else(|e| e.into_inner());
        use rustudio_core::__rbx::types::{ContentId, Variant};
        use rustudio_core::__rbx::{ustr, InstanceBuilder};

        let root = std::env::temp_dir().join(format!(
            "rustudio-cook-e2e-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let store = root.join("library");
        fs::create_dir_all(&store).unwrap();
        fs::write(store.join("900000001.png"), b"fake png bytes").unwrap();

        let mut doc = Document::new_place();
        let workspace = doc.workspace().unwrap();
        let part = doc.dom.insert(workspace, InstanceBuilder::new("Part").with_name("Wall"));
        let decal = doc.dom.insert(part, InstanceBuilder::new("Decal"));
        doc.dom.get_by_ref_mut(decal).unwrap().properties.insert(
            ustr("Texture"),
            Variant::ContentId(ContentId::from("rbxassetid://900000001")),
        );
        let place = root.join("level.rbxlx");
        fs::write(&place, doc.encode_all(true).unwrap()).unwrap();

        let out = root.join("bundle");
        let options = Options {
            place,
            out: out.clone(),
            report: None,
            content: None,
            local_assets: Some(store),
            player: None,
            seconds: 1.0,
            capture: false,
            signed_in: false,
            allow_missing: false,
        };
        assert_eq!(cook(&options).unwrap(), 0, "{options:?}");
        assert!(out.join("cache/assets/900000001_vlatest.bin").is_file());
        assert!(out.join("place/level.rbxlx").is_file());
        assert!(out.join("place").join(assets::LOCAL_CONTENT_MARKER).is_file());
        let report: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(out.join("assets.json")).unwrap()).unwrap();
        assert_eq!(report["assets"][0]["id"], "rbxassetid://900000001");
        assert_eq!(report["assets"][0]["status"], "imported");
        assert_eq!(report["summary"]["imported"], 1);
        assert_eq!(report["problems"], serde_json::json!([]));
        fs::remove_dir_all(&root).unwrap();
    }

    /// F4 end to end without a player or the network: a capture ledger with
    /// a cached id, a failed id, a gone file and another place's asset.
    #[test]
    fn export_copies_a_places_captures_and_reports_the_rest() {
        let _env = ENV.lock().unwrap_or_else(|e| e.into_inner());
        let root = std::env::temp_dir().join(format!(
            "rustudio-cook-export-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let cache = root.join("cache");
        fs::create_dir_all(cache.join("assets")).unwrap();
        fs::write(cache.join("assets/7_vlatest.bin"), b"seven").unwrap();
        fs::write(cache.join("assets/8_v3.bin"), b"eight").unwrap();
        let place = root.join("level.rbxlx");
        fs::write(&place, b"<roblox/>").unwrap();
        let key = place.canonicalize().unwrap().display().to_string();
        let ledger = root.join("capture-ledger.jsonl");
        fs::write(
            &ledger,
            format!(
                concat!(
                    r#"{{"time":1,"place":"{key}","id":7,"version":null,"status":"cached","path":"assets/7_vlatest.bin","size":5}}"#, "\n",
                    r#"{{"time":0,"place":"{key}","id":7,"version":null,"status":"failed","size":0}}"#, "\n",
                    r#"{{"time":2,"place":"/elsewhere.rbxlx","id":8,"version":3,"status":"cached","path":"assets/8_v3.bin","size":5}}"#, "\n",
                    r#"{{"time":3,"place":"{key}","id":9,"version":null,"status":"failed","size":0}}"#, "\n",
                    r#"{{"time":4,"place":"{key}","id":10,"version":null,"status":"cached","path":"assets/10_vlatest.bin","size":5}}"#, "\n",
                    "not json\n",
                ),
                key = key
            ),
        )
        .unwrap();
        std::env::set_var("RUSTUDIO_CACHE_DIR", &cache);

        let out = root.join("bundle");
        let options = ExportOptions {
            place: place.clone(),
            out: out.clone(),
            ledger: Some(ledger.clone()),
            all: false,
            allow_missing: false,
        };
        assert_eq!(export(&options).unwrap(), 1, "failures and a gone file");
        assert_eq!(fs::read(out.join("cache/assets/7_vlatest.bin")).unwrap(), b"seven");
        assert!(!out.join("cache/assets/8_v3.bin").exists(), "another place's asset");
        let report: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(out.join("offline-assets.json")).unwrap()).unwrap();
        assert_eq!(report["kind"], "offline-assets");
        assert_eq!(report["summary"]["exported"], 1);
        assert_eq!(report["summary"]["failed"], 1);
        assert_eq!(report["summary"]["missing"], 1);
        assert_eq!(report["assets"].as_array().unwrap().len(), 3);
        let exported = report["assets"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["id"] == "rbxassetid://7")
            .unwrap()
            .clone();
        assert_eq!(exported["status"], "exported");
        assert_eq!(exported["file"], "cache/assets/7_vlatest.bin");
        assert_eq!(exported["size"], 5);
        assert_eq!(exported["sha256"].as_str().unwrap().len(), 64);

        // --all takes every place; --allow-missing still writes the report.
        let all = ExportOptions { all: true, allow_missing: true, ..options };
        assert_eq!(export(&all).unwrap(), 0);
        assert!(out.join("cache/assets/8_v3.bin").is_file());

        std::env::remove_var("RUSTUDIO_CACHE_DIR");
        fs::remove_dir_all(&root).unwrap();
    }
}
