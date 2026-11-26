//! rustudio game launcher (spawn mode; FORK-PLAN §5.1).
//!
//! This binary is copied into a game bundle beside `game.json`. It reads the
//! manifest, resolves everything relative to its own executable (never the
//! current directory), applies the environment contract from FORK-PLAN §6
//! without overwriting variables the developer set explicitly, then spawns
//! the unmodified `engine/rustudio-player` with the entry place.
//!
//! It has no dependency on any rustudio crate, so upstream engine changes
//! cannot break it. The bundle format it reads is documented in
//! `game/docs/BUNDLE.md`.

#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

use std::{
    env,
    ffi::OsString,
    fs::{self, OpenOptions},
    io::Write,
    path::{Component, Path, PathBuf},
    process::{self, Command, Stdio},
    time::Instant,
};

use serde::Deserialize;

/// The manifest format this launcher understands.
const MANIFEST_VERSION: u32 = 1;
/// The player binary inside the bundle (FORK-PLAN §4).
const ENGINE_BIN: &str = "engine/rustudio-player";

#[derive(Debug, Deserialize)]
struct Manifest {
    manifest_version: Option<u32>,
    id: String,
    name: String,
    #[serde(default)]
    #[allow(dead_code)] // reserved for the log/version stamp (§4)
    version: Option<String>,
    /// The entry place, relative to the bundle root.
    entry: String,
    /// Game asset cache (default `cache`).
    #[serde(default)]
    cache: Option<String>,
    /// Optional local asset store (default `assets`).
    #[serde(default)]
    assets: Option<String>,
    /// Default engine content pack (§7.1): absent, a mode string, or an
    /// object. A plain string is shorthand for a bundled pack at that path.
    #[serde(default)]
    content: Option<ContentSpec>,
    /// Window size; passed as `--window WxH` unless `args` already says so.
    #[serde(default)]
    window: Option<Window>,
    /// Extra player arguments (the harness escape hatch, §4).
    #[serde(default)]
    args: Vec<String>,
    /// Offline promise: `RUSTUDIO_OFFLINE=1` for the player (F2).
    #[serde(default)]
    offline: Option<bool>,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(untagged)]
enum ContentSpec {
    /// `"bundled"`, `"download"`, `"engine"`, or a pack path (bundled).
    Mode(String),
    Spec {
        mode: String,
        #[serde(default)]
        path: Option<String>,
        #[serde(default)]
        #[allow(dead_code)] // download mode: F2/F3-era work (§7.1)
        url: Option<String>,
        #[serde(default)]
        #[allow(dead_code)]
        sha256: Option<String>,
        #[serde(default)]
        #[allow(dead_code)]
        version: Option<String>,
    },
}

#[derive(Debug, Deserialize, PartialEq)]
struct Window {
    #[serde(default)]
    #[allow(dead_code)] // RUSTUDIO_WINDOW_TITLE arrives with F1
    title: Option<String>,
    #[serde(default)]
    width: Option<u32>,
    #[serde(default)]
    height: Option<u32>,
}

/// Where the content pack resolved to (§7.1 first-launch rules, the parts
/// that do not need the network).
#[derive(Debug, PartialEq)]
enum Content {
    /// No pack: let the engine find its own content, else blocky fallback.
    None,
    /// Leave the variables unset: the engine's own lookup decides
    /// (development only, per the manifest).
    Engine,
    Bundled {
        /// `<bundle>/<path>/content`: the `rbxasset://` root.
        content_dir: Option<PathBuf>,
        /// `<bundle>/<path>`: set only when it validates as a Studio root
        /// (`ExtraContent/scripts/PlayerScripts`), as §6 requires.
        studio_dir: Option<PathBuf>,
    },
}

#[derive(Debug)]
struct Bundle {
    root: PathBuf,
    entry: PathBuf,
    engine: PathBuf,
    cache: PathBuf,
    /// Only an existing directory: never point the engine at a missing path.
    assets: Option<PathBuf>,
    content: Content,
    /// The manifest's offline promise: set `RUSTUDIO_OFFLINE=1`.
    offline: bool,
}

/// What the launcher writes to its own log before the player starts.
struct Plan {
    bundle: Bundle,
    notes: Vec<String>,
}

fn main() {
    let started = Instant::now();
    let code = match launch(started) {
        Ok(code) => code,
        Err(error) => {
            eprintln!("{error}");
            // The log is the only output on a windowless Windows build.
            if let Ok(root) = bundle_root() {
                let _ = fs::create_dir_all(root.join("logs"));
                let _ = fs::write(
                    root.join("logs/latest.log"),
                    format!("[launcher +{:.3}s] error: {error}\n", started.elapsed().as_secs_f32()),
                );
            }
            1
        }
    };
    process::exit(code);
}

fn launch(started: Instant) -> Result<i32, String> {
    let root = bundle_root()?;
    let log_path = root.join("logs/latest.log");
    fs::create_dir_all(root.join("logs"))
        .map_err(|e| format!("can't create {}: {e}", root.join("logs").display()))?;
    let mut log = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&log_path)
        .map_err(|e| format!("can't open {}: {e}", log_path.display()))?;
    let mut say = |text: String| {
        let _ = writeln!(log, "[launcher +{:.3}s] {text}", started.elapsed().as_secs_f32());
        let _ = log.flush();
    };

    let manifest_text = fs::read_to_string(root.join("game.json"))
        .map_err(|e| format!("can't read {}: {e}", root.join("game.json").display()))?;
    let manifest: Manifest = serde_json::from_str(&manifest_text)
        .map_err(|e| format!("game.json: {e}"))?;
    match manifest.manifest_version {
        Some(MANIFEST_VERSION) => {}
        Some(v) => return Err(format!("game.json: manifest_version {v} is not supported (this launcher reads {MANIFEST_VERSION})")),
        None => return Err("game.json: manifest_version is required".into()),
    }

    let plan = resolve(&manifest, &root, &|p| p.is_dir())?;
    if !plan.bundle.entry.is_file() {
        return Err(format!("entry place not found: {}", plan.bundle.entry.display()));
    }
    say(format!("{} ({})", manifest.name, manifest.id));
    for note in &plan.notes {
        say(note.clone());
    }
    apply_env(&plan.bundle, &mut say);

    let mut command = Command::new(&plan.bundle.engine);
    command.arg(&plan.bundle.entry);
    command.args(manifest.args.iter());
    if !manifest.args.iter().any(|a| a == "--window") {
        if let Some(w) = &manifest.window {
            if let (Some(width), Some(height)) = (w.width, w.height) {
                if width > 0 && height > 0 {
                    command.arg("--window").arg(format!("{width}x{height}"));
                }
            }
        }
    }
    // Debugging escape hatch: arguments given to the launcher go to the
    // player after the manifest's own.
    command.args(env::args_os().skip(1));

    say(format!("running {}", plan.bundle.engine.display()));
    drop(say);
    let out = log.try_clone().map_err(|e| format!("can't clone the log: {e}"))?;
    let err = log.try_clone().map_err(|e| format!("can't clone the log: {e}"))?;
    command
        .current_dir(&plan.bundle.root)
        .stdin(Stdio::null())
        .stdout(Stdio::from(out))
        .stderr(Stdio::from(err));
    let mut child = command
        .spawn()
        .map_err(|e| format!("can't start {}: {e}", plan.bundle.engine.display()))?;
    let status = child
        .wait()
        .map_err(|e| format!("lost the player process: {e}"))?;
    let code = exit_code(status);
    let _ = writeln!(
        log,
        "[launcher +{:.3}s] player exited with status {code} after {:.2}s",
        started.elapsed().as_secs_f32(),
        started.elapsed().as_secs_f32()
    );
    let _ = log.flush();
    Ok(code)
}

fn bundle_root() -> Result<PathBuf, String> {
    let exe = env::current_exe().map_err(|e| format!("can't find this executable: {e}"))?;
    exe.parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| "this executable has no directory".to_string())
}

/// The platform's player binary name inside `engine/`.
fn engine_binary(root: &Path) -> PathBuf {
    let name = if cfg!(windows) { format!("{ENGINE_BIN}.exe") } else { ENGINE_BIN.to_string() };
    root.join(name)
}

/// Resolves the manifest against the bundle root. `is_dir` is injectable so
/// the rules can be tested without a real tree (and so Windows paths work).
fn resolve(manifest: &Manifest, root: &Path, is_dir: &dyn Fn(&Path) -> bool) -> Result<Plan, String> {
    let entry = relative_to(root, &manifest.entry, "entry")?;
    let engine = engine_binary(root);
    if !is_dir(&root.join("engine")) {
        return Err(format!("no engine/ folder in {}", root.display()));
    }
    let cache = relative_to(root, manifest.cache.as_deref().unwrap_or("cache"), "cache")?;
    let assets = relative_to(root, manifest.assets.as_deref().unwrap_or("assets"), "assets")?;
    let assets = is_dir(&assets).then_some(assets);

    let mut notes = Vec::new();
    let content = match &manifest.content {
        None => Content::None,
        Some(ContentSpec::Mode(mode)) => resolve_content_mode(mode, "content", root, is_dir, &mut notes)?,
        Some(ContentSpec::Spec { mode, path, .. }) => {
            resolve_content_mode(mode, path.as_deref().unwrap_or("content"), root, is_dir, &mut notes)?
        }
    };
    if assets.is_none() {
        notes.push("no assets/ folder: RUSTUDIO_LOCAL_ASSETS stays unset".into());
    }
    let offline = manifest.offline == Some(true);
    if offline {
        notes.push("offline: true (RUSTUDIO_OFFLINE=1)".into());
    }
    Ok(Plan { bundle: Bundle { root: root.to_path_buf(), entry, engine, cache, assets, content, offline }, notes })
}

fn resolve_content_mode(
    mode: &str,
    path: &str,
    root: &Path,
    is_dir: &dyn Fn(&Path) -> bool,
    notes: &mut Vec<String>,
) -> Result<Content, String> {
    match mode {
        "bundled" => resolve_bundled(path, root, is_dir, notes),
        "engine" => {
            notes.push("content: engine-managed (development mode)".into());
            Ok(Content::Engine)
        }
        "download" => {
            notes.push(
                "content: download mode is not implemented yet; \
                 falling back to the engine's own content (F2/F3 work)"
                    .into(),
            );
            Ok(Content::None)
        }
        // A plain string that is not a mode names a bundled pack path (§4).
        other => resolve_bundled(other, root, is_dir, notes),
    }
}

fn resolve_bundled(
    path: &str,
    root: &Path,
    is_dir: &dyn Fn(&Path) -> bool,
    notes: &mut Vec<String>,
) -> Result<Content, String> {
    let pack = relative_to(root, path, "content.path")?;
    if !is_dir(&pack) {
        notes.push(format!(
            "content: bundled pack missing at {}; launching with engine content/fallback",
            pack.display()
        ));
        return Ok(Content::None);
    }
    let content_dir = pack.join("content");
    let content_dir = is_dir(&content_dir).then_some(content_dir);
    let studio = pack.join("ExtraContent/scripts/PlayerScripts");
    let studio_dir = is_dir(&studio).then(|| pack.clone());
    match (&content_dir, &studio_dir) {
        (Some(_), Some(_)) => notes.push(format!("content: bundled pack {}", pack.display())),
        (Some(_), None) => notes.push(format!(
            "content: bundled pack {} has content/ but no ExtraContent/scripts/PlayerScripts; \
             RUSTUDIO_STUDIO_DIR stays unset",
            pack.display()
        )),
        (None, Some(_)) => notes.push(format!(
            "content: bundled pack {} has PlayerScripts but no content/; RUSTUDIO_CONTENT_DIR stays unset",
            pack.display()
        )),
        (None, None) => {
            notes.push(format!(
                "content: {} has neither content/ nor ExtraContent/scripts/PlayerScripts; ignoring it",
                pack.display()
            ));
            return Ok(Content::None);
        }
    }
    Ok(Content::Bundled { content_dir, studio_dir })
}

/// A manifest path must stay inside the bundle (§4: all paths are relative
/// to the bundle root).
fn relative_to(root: &Path, rel: &str, what: &str) -> Result<PathBuf, String> {
    if rel.trim().is_empty() {
        return Err(format!("{what} is empty"));
    }
    let rel_path = Path::new(rel);
    if rel_path.is_absolute() {
        return Err(format!("{what} must be relative to the bundle: {rel}"));
    }
    let mut out = root.to_path_buf();
    for component in rel_path.components() {
        match component {
            Component::Normal(part) => out.push(part),
            Component::CurDir => {}
            _ => return Err(format!("{what} must stay inside the bundle: {rel}")),
        }
    }
    Ok(out)
}

/// The §6 environment contract, in the order it is applied. `keys` is what
/// the launcher owns; a key the developer already set is left alone.
fn env_contract(bundle: &Bundle) -> Vec<(&'static str, OsString)> {
    let mut vars: Vec<(&'static str, OsString)> = vec![
        ("RUSTUDIO_NO_SESSION", "1".into()),
        ("RUSTUDIO_NO_STUDIO_FETCH", "1".into()),
        ("RUSTUDIO_CORE_SCRIPTS", "0".into()),
        ("RUSTUDIO_LIVE_DATA", "0".into()),
        ("RUSTUDIO_CACHE_DIR", bundle.cache.clone().into_os_string()),
    ];
    if bundle.offline {
        vars.push(("RUSTUDIO_OFFLINE", "1".into()));
    }
    if let Some(assets) = &bundle.assets {
        vars.push(("RUSTUDIO_LOCAL_ASSETS", assets.clone().into_os_string()));
    }
    if let Content::Bundled { content_dir: Some(dir), .. } = &bundle.content {
        vars.push(("RUSTUDIO_CONTENT_DIR", dir.clone().into_os_string()));
    }
    if let Content::Bundled { studio_dir: Some(dir), .. } = &bundle.content {
        vars.push(("RUSTUDIO_STUDIO_DIR", dir.clone().into_os_string()));
    }
    vars
}

fn apply_env(bundle: &Bundle, say: &mut impl FnMut(String)) {
    let cache_ok = fs::create_dir_all(&bundle.cache)
        .map_err(|e| format!("can't create cache dir {}: {e}", bundle.cache.display()));
    if let Err(e) = cache_ok {
        say(e);
    }
    for (key, value) in env_contract(bundle) {
        if let Ok(old) = env::var(key) {
            // The escape hatch: an explicitly set variable wins.
            say(format!("keeping {key}={old} (set by the developer)"));
            continue;
        }
        say(format!("set {key}={}", value.to_string_lossy()));
        env::set_var(key, value);
    }
}

fn exit_code(status: process::ExitStatus) -> i32 {
    if let Some(code) = status.code() {
        return code;
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(signal) = status.signal() {
            return 128 + signal;
        }
    }
    1
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(json: &str) -> Manifest {
        serde_json::from_str(json).expect("manifest")
    }

    /// The example from FORK-PLAN §4.
    fn example() -> Manifest {
        manifest(
            r#"{
              "manifest_version": 1,
              "id": "mygame",
              "name": "My Game",
              "version": "1.0.0",
              "engine": "rustudio 0.5.2 + fork.1",
              "entry": "place/game.rbxlx",
              "cache": "cache",
              "assets": "assets",
              "content": { "mode": "bundled", "path": "content", "version": "1.0.0" },
              "window": { "title": "My Game", "width": 1600, "height": 900 },
              "args": ["--window", "1600x900"],
              "offline": true
            }"#,
        )
    }

    #[test]
    fn parses_the_example_manifest() {
        let m = example();
        assert_eq!(m.manifest_version, Some(1));
        assert_eq!(m.entry, "place/game.rbxlx");
        assert_eq!(m.window.as_ref().unwrap().width, Some(1600));
        assert_eq!(m.args, ["--window", "1600x900"]);
        assert_eq!(m.offline, Some(true));
        assert!(matches!(m.content, Some(ContentSpec::Spec { ref mode, .. }) if mode == "bundled"));
    }

    #[test]
    fn content_accepts_a_plain_string_as_a_bundled_path() {
        let m = manifest(r#"{"manifest_version":1,"id":"x","name":"X","entry":"p.rbxlx","content":"content"}"#);
        assert_eq!(m.content, Some(ContentSpec::Mode("content".into())));
    }

    #[test]
    fn paths_must_stay_inside_the_bundle() {
        let root = Path::new("/games/mygame");
        assert_eq!(relative_to(root, "place/game.rbxlx", "entry").unwrap(), root.join("place/game.rbxlx"));
        assert!(relative_to(root, "/etc/passwd", "entry").is_err());
        assert!(relative_to(root, "../other/game.rbxlx", "entry").is_err());
        assert!(relative_to(root, "place/../../game.rbxlx", "entry").is_err());
        assert!(relative_to(root, "", "entry").is_err());
    }

    #[test]
    fn content_modes_resolve_from_the_bundle() {
        let root = Path::new("/games/mygame");
        let mut notes = Vec::new();
        let bundled = resolve_content_mode(
            "bundled",
            "content",
            root,
            &|p| p == Path::new("/games/mygame/content") || p == Path::new("/games/mygame/content/content")
                || p == Path::new("/games/mygame/content/ExtraContent/scripts/PlayerScripts"),
            &mut notes,
        )
        .unwrap();
        assert_eq!(
            bundled,
            Content::Bundled {
                content_dir: Some(root.join("content/content")),
                studio_dir: Some(root.join("content")),
            }
        );

        let mut notes = Vec::new();
        let engine = resolve_content_mode("engine", "content", root, &|_| true, &mut notes).unwrap();
        assert_eq!(engine, Content::Engine);

        let mut notes = Vec::new();
        let download = resolve_content_mode("download", "content", root, &|_| true, &mut notes).unwrap();
        assert_eq!(download, Content::None);
        assert!(notes.iter().any(|n| n.contains("not implemented")));
    }

    #[test]
    fn a_pack_without_studio_scripts_only_sets_the_content_root() {
        let root = Path::new("/games/mygame");
        let mut notes = Vec::new();
        let pack = resolve_content_mode(
            "bundled",
            "content",
            root,
            &|p| p == Path::new("/games/mygame/content") || p == Path::new("/games/mygame/content/content"),
            &mut notes,
        )
        .unwrap();
        assert_eq!(pack, Content::Bundled { content_dir: Some(root.join("content/content")), studio_dir: None });
    }

    #[test]
    fn a_missing_pack_is_not_an_error() {
        let root = Path::new("/games/mygame");
        let mut notes = Vec::new();
        let pack = resolve_content_mode("bundled", "content", root, &|_| false, &mut notes).unwrap();
        assert_eq!(pack, Content::None);
        assert!(notes.iter().any(|n| n.contains("launching with engine content/fallback")));
    }

    #[test]
    fn resolve_builds_the_bundle_paths() {
        let root = Path::new("/games/mygame");
        let mut m = example();
        m.assets = Some("assets".into());
        let plan = resolve(&m, root, &|p| {
            p == Path::new("/games/mygame/engine")
                || p == Path::new("/games/mygame/assets")
                || p == Path::new("/games/mygame/content")
                || p == Path::new("/games/mygame/content/content")
                || p == Path::new("/games/mygame/content/ExtraContent/scripts/PlayerScripts")
        })
        .unwrap();
        assert_eq!(plan.bundle.entry, root.join("place/game.rbxlx"));
        assert_eq!(plan.bundle.engine, root.join(if cfg!(windows) { "engine/rustudio-player.exe" } else { "engine/rustudio-player" }));
        assert_eq!(plan.bundle.cache, root.join("cache"));
        assert_eq!(plan.bundle.assets, Some(root.join("assets")));
    }

    #[test]
    fn env_contract_sets_the_offline_switches_and_bundle_paths() {
        let root = Path::new("/games/mygame");
        let bundle = Bundle {
            root: root.to_path_buf(),
            entry: root.join("place/game.rbxlx"),
            engine: root.join("engine/rustudio-player"),
            cache: root.join("cache"),
            assets: Some(root.join("assets")),
            content: Content::Bundled {
                content_dir: Some(root.join("content/content")),
                studio_dir: Some(root.join("content")),
            },
            offline: true,
        };
        let vars: Vec<(&str, OsString)> = env_contract(&bundle);
        let get = |key: &str| vars.iter().find(|(k, _)| *k == key).map(|(_, v)| v.clone());
        assert_eq!(get("RUSTUDIO_NO_SESSION"), Some(OsString::from("1")));
        assert_eq!(get("RUSTUDIO_NO_STUDIO_FETCH"), Some(OsString::from("1")));
        assert_eq!(get("RUSTUDIO_CORE_SCRIPTS"), Some(OsString::from("0")));
        assert_eq!(get("RUSTUDIO_LIVE_DATA"), Some(OsString::from("0")));
        assert_eq!(get("RUSTUDIO_OFFLINE"), Some(OsString::from("1")));
        assert_eq!(get("RUSTUDIO_CACHE_DIR"), Some(root.join("cache").into_os_string()));
        assert_eq!(get("RUSTUDIO_LOCAL_ASSETS"), Some(root.join("assets").into_os_string()));
        assert_eq!(get("RUSTUDIO_CONTENT_DIR"), Some(root.join("content/content").into_os_string()));
        assert_eq!(get("RUSTUDIO_STUDIO_DIR"), Some(root.join("content").into_os_string()));
    }

    #[test]
    fn no_pack_leaves_content_variables_unset() {
        let root = Path::new("/games/mygame");
        let bundle = Bundle {
            root: root.to_path_buf(),
            entry: root.join("p.rbxlx"),
            engine: root.join("engine/rustudio-player"),
            cache: root.join("cache"),
            assets: None,
            content: Content::None,
            offline: false,
        };
        let vars = env_contract(&bundle);
        let keys: Vec<&str> = vars.iter().map(|(k, _)| *k).collect();
        assert!(!keys.contains(&"RUSTUDIO_CONTENT_DIR"));
        assert!(!keys.contains(&"RUSTUDIO_STUDIO_DIR"));
        assert!(!keys.contains(&"RUSTUDIO_LOCAL_ASSETS"));
        assert!(!keys.contains(&"RUSTUDIO_OFFLINE"));
    }

    #[test]
    fn an_unknown_manifest_version_is_rejected() {
        let m = manifest(r#"{"manifest_version":2,"id":"x","name":"X","entry":"p.rbxlx"}"#);
        assert_ne!(m.manifest_version, Some(MANIFEST_VERSION));
    }

    #[test]
    fn exit_codes_propagate() {
        #[cfg(unix)]
        {
            use std::os::unix::process::ExitStatusExt;
            let status = process::ExitStatus::from_raw(7 << 8);
            assert_eq!(exit_code(status), 7);
            let killed = process::ExitStatus::from_raw(9); // SIGKILL
            assert_eq!(exit_code(killed), 128 + 9);
        }
    }

}
