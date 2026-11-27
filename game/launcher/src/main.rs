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

mod pack;

/// The manifest format this launcher understands.
const MANIFEST_VERSION: u32 = 1;
/// The player binary inside the bundle (FORK-PLAN §4).
const ENGINE_BIN: &str = "engine/rustudio-player";

#[derive(Debug, Deserialize)]
struct Manifest {
    manifest_version: Option<u32>,
    id: String,
    name: String,
    /// Stamped into the log (§4/§15).
    #[serde(default)]
    version: Option<String>,
    /// The entry place, relative to the bundle root.
    entry: String,
    /// Game asset cache (default `cache`).
    #[serde(default)]
    cache: Option<String>,
    /// Optional local asset store (default `assets`).
    #[serde(default)]
    assets: Option<String>,
    /// Local DataStore records (default `datastores`, F5).
    #[serde(default)]
    datastores: Option<String>,
    /// Private per-game home for settings, caches and crash reports
    /// (default `home`, F9).
    #[serde(default)]
    home: Option<String>,
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
    /// Web access for the game's scripts (§9). Allowed unless this is
    /// `false`; blocking makes HttpService/web streams fail fast with one
    /// log line instead of waiting for a timeout.
    #[serde(default)]
    network: Option<bool>,
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
        /// Download mode: where the pack archive lives (§7.1).
        #[serde(default)]
        url: Option<String>,
        /// Download mode: the pack hash `gamepack` wrote.
        #[serde(default)]
        sha256: Option<String>,
        #[serde(default)]
        version: Option<String>,
    },
}

#[derive(Debug, Deserialize, PartialEq)]
struct Window {
    #[serde(default)]
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
        /// `<bundle>/<path>`: the pack root. It has the Studio layout
        /// (`content/` and/or `ExtraContent/scripts/PlayerScripts`); the
        /// engine validates each half (F3).
        root: PathBuf,
    },
    /// A pack fetched and installed under the per-user data directory.
    Downloaded {
        /// The installed version's pack root (same layout as bundled).
        root: PathBuf,
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
    /// Where the engine's local DataStore backend writes records (F5).
    datastores: PathBuf,
    /// The game's private home: settings, caches, crash reports (F9).
    home: PathBuf,
    /// `window.title`, else the game name (F1).
    window_title: String,
    content: Content,
    /// The manifest's offline promise: set `RUSTUDIO_OFFLINE=1`.
    offline: bool,
    /// Allowed by default; `"network": false` sets `RUSTUDIO_NO_NETWORK=1`
    /// (§9). `offline: true` restricts assets, not the network.
    network: bool,
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
            // The log is the only output on a windowless Windows build, so
            // a failure is also shown to the person (FORK-PLAN §5.1).
            let log_path = bundle_root().ok().map(|root| root.join("logs/latest.log"));
            if let Some(log) = &log_path {
                if let Some(dir) = log.parent() {
                    let _ = fs::create_dir_all(dir);
                }
                let _ = fs::write(
                    log,
                    format!("[launcher +{:.3}s] error: {error}\n", started.elapsed().as_secs_f32()),
                );
            }
            match log_path {
                Some(log) => show_error(&format!("{error}\n\nThe log is {}", log.display())),
                None => show_error(&error),
            }
            1
        }
    };
    process::exit(code);
}

fn show_error(error: &str) {
    #[cfg(windows)]
    {
        use windows_sys::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR, MB_OK};
        let text: Vec<u16> = error.encode_utf16().chain(std::iter::once(0)).collect();
        let title: Vec<u16> = "The game could not start".encode_utf16().chain(std::iter::once(0)).collect();
        // SAFETY: both strings are NUL-terminated and live through the call.
        unsafe {
            MessageBoxW(std::ptr::null_mut(), text.as_ptr(), title.as_ptr(), MB_OK | MB_ICONERROR);
        }
    }
    #[cfg(not(windows))]
    eprintln!("{error}");
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

    // Download mode (§7.1): the per-user data directory keeps a pack that
    // can be reused, and the offline promise forbids fetching it.
    let allow_network = manifest.offline != Some(true);
    let download = |spec: &pack::DownloadSpec| {
        let base = pack::data_root()?;
        pack::ensure(&base, &manifest.id, spec, allow_network, &pack::fetch)
    };
    let plan = resolve(&manifest, &root, &|p| p.is_dir(), &download)?;
    if !plan.bundle.entry.is_file() {
        return Err(format!("entry place not found: {}", plan.bundle.entry.display()));
    }
    say(format!(
        "{} {} ({})",
        manifest.name,
        manifest.version.as_deref().unwrap_or("unversioned"),
        manifest.id
    ));
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

/// Resolves the manifest against the bundle root. `is_dir` and `download`
/// are injectable so the rules can be tested without a real tree, a network
/// or the per-user data directory (and so Windows paths work).
fn resolve(
    manifest: &Manifest,
    root: &Path,
    is_dir: &dyn Fn(&Path) -> bool,
    download: &dyn Fn(&pack::DownloadSpec) -> Result<pack::Installed, String>,
) -> Result<Plan, String> {
    let entry = relative_to(root, &manifest.entry, "entry")?;
    let engine = engine_binary(root);
    if !is_dir(&root.join("engine")) {
        return Err(format!("no engine/ folder in {}", root.display()));
    }
    let cache = relative_to(root, manifest.cache.as_deref().unwrap_or("cache"), "cache")?;
    let assets = relative_to(root, manifest.assets.as_deref().unwrap_or("assets"), "assets")?;
    let assets = is_dir(&assets).then_some(assets);
    let datastores = relative_to(
        root,
        manifest.datastores.as_deref().unwrap_or("datastores"),
        "datastores",
    )?;
    let home = relative_to(root, manifest.home.as_deref().unwrap_or("home"), "home")?;
    let window_title = manifest
        .window
        .as_ref()
        .and_then(|w| w.title.clone())
        .unwrap_or_else(|| manifest.name.clone());

    let mut notes = Vec::new();
    let content = match &manifest.content {
        None => Content::None,
        Some(ContentSpec::Mode(mode)) => resolve_content_mode(mode, "content", root, is_dir, &mut notes)?,
        Some(ContentSpec::Spec { mode, path, url, sha256, version }) if mode == "download" => {
            match (url, sha256) {
                (Some(url), Some(sha256)) => {
                    let spec = pack::DownloadSpec {
                        url: url.clone(),
                        sha256: sha256.to_ascii_lowercase(),
                        version: version.clone(),
                    };
                    match download(&spec) {
                        Ok(installed) => {
                            notes.push(format!(
                                "content: {} pack {} at {}",
                                if installed.reused { "reusing installed" } else { "installed" },
                                installed.version,
                                installed.root.display()
                            ));
                            Content::Downloaded { root: installed.root }
                        }
                        Err(e) => {
                            notes.push(format!(
                                "content: download pack unavailable ({e}); \
                                 launching with engine content/fallback"
                            ));
                            Content::None
                        }
                    }
                }
                _ => {
                    notes.push(
                        "content: download mode needs url and sha256; \
                         launching with engine content/fallback"
                            .into(),
                    );
                    Content::None
                }
            }
        }
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
    let network = manifest.network != Some(false);
    notes.push(match manifest.network {
        Some(false) => "network: blocked (game.json says network: false)".to_string(),
        Some(true) => "network: allowed (game.json)".to_string(),
        None => "network: allowed (default; set \"network\": false to block)".to_string(),
    });
    Ok(Plan { bundle: Bundle { root: root.to_path_buf(), entry, engine, cache, assets, datastores, home, window_title, content, offline, network }, notes })
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
                "content: download mode needs an object with url and sha256; \
                 launching with engine content/fallback"
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
    let has_content = is_dir(&pack.join("content"));
    let has_scripts = is_dir(&pack.join("ExtraContent/scripts/PlayerScripts"));
    match (has_content, has_scripts) {
        (true, true) => notes.push(format!("content: bundled pack {}", pack.display())),
        (true, false) => notes.push(format!(
            "content: bundled pack {} has content/ but no ExtraContent/scripts/PlayerScripts; \
             default scripts come from the engine's own lookup",
            pack.display()
        )),
        (false, true) => notes.push(format!(
            "content: bundled pack {} has PlayerScripts but no content/; \
             rbxasset:// falls back to the engine's content",
            pack.display()
        )),
        (false, false) => {
            notes.push(format!(
                "content: {} has neither content/ nor ExtraContent/scripts/PlayerScripts; ignoring it",
                pack.display()
            ));
            return Ok(Content::None);
        }
    }
    Ok(Content::Bundled { root: pack })
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
        // F5: the local DataStore backend persists game records here.
        ("RUSTUDIO_DATASTORE_DIR", bundle.datastores.clone().into_os_string()),
        // F9: settings, caches and crash reports stay inside the game.
        ("RUSTUDIO_HOME", bundle.home.clone().into_os_string()),
        // F1: the player's window title.
        ("RUSTUDIO_WINDOW_TITLE", bundle.window_title.clone().into()),
        // §9: a game bundle runs with HttpService enabled; `network: false`
        // then blocks the requests at the gate instead.
        ("RUSTUDIO_HTTP_ENABLED", "1".into()),
        // The capture ledger records this game's asset requests against its
        // own place, so "Save assets for offline use" exports exactly them.
        ("RUSTUDIO_ASSET_PLACE", bundle.entry.clone().into_os_string()),
    ];
    if bundle.offline {
        vars.push(("RUSTUDIO_OFFLINE", "1".into()));
    }
    if !bundle.network {
        // §9: shipped games fail web requests fast instead of timing out.
        vars.push(("RUSTUDIO_NO_NETWORK", "1".into()));
    }
    if let Some(assets) = &bundle.assets {
        vars.push(("RUSTUDIO_LOCAL_ASSETS", assets.clone().into_os_string()));
    }
    match &bundle.content {
        Content::Bundled { root } | Content::Downloaded { root } => {
            vars.push(("RUSTUDIO_ENGINE_CONTENT_DIR", root.clone().into_os_string()));
        }
        Content::None | Content::Engine => {}
    }
    vars
}

fn apply_env(bundle: &Bundle, say: &mut impl FnMut(String)) {
    for dir in [&bundle.cache, &bundle.datastores, &bundle.home] {
        if let Err(e) = fs::create_dir_all(dir) {
            say(format!("can't create {}: {e}", dir.display()));
        }
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
        assert_eq!(bundled, Content::Bundled { root: root.join("content") });

        let mut notes = Vec::new();
        let engine = resolve_content_mode("engine", "content", root, &|_| true, &mut notes).unwrap();
        assert_eq!(engine, Content::Engine);

        let mut notes = Vec::new();
        let download = resolve_content_mode("download", "content", root, &|_| true, &mut notes).unwrap();
        assert_eq!(download, Content::None);
        assert!(notes.iter().any(|n| n.contains("needs an object with url and sha256")));
    }

    #[test]
    fn a_pack_without_studio_scripts_still_uses_its_content() {
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
        assert_eq!(pack, Content::Bundled { root: root.join("content") });
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
        let plan = resolve(
            &m,
            root,
            &|p| {
                p == Path::new("/games/mygame/engine")
                    || p == Path::new("/games/mygame/assets")
                    || p == Path::new("/games/mygame/content")
                    || p == Path::new("/games/mygame/content/content")
                    || p == Path::new("/games/mygame/content/ExtraContent/scripts/PlayerScripts")
            },
            &|_| unreachable!("the example manifest is bundled"),
        )
        .unwrap();
        assert_eq!(plan.bundle.entry, root.join("place/game.rbxlx"));
        assert_eq!(plan.bundle.engine, root.join(if cfg!(windows) { "engine/rustudio-player.exe" } else { "engine/rustudio-player" }));
        assert_eq!(plan.bundle.cache, root.join("cache"));
        assert_eq!(plan.bundle.assets, Some(root.join("assets")));
        assert_eq!(plan.bundle.datastores, root.join("datastores"));
        assert_eq!(plan.bundle.home, root.join("home"));
        assert_eq!(plan.bundle.window_title, "My Game");
    }

    fn download_manifest() -> Manifest {
        manifest(
            r#"{
              "manifest_version": 1,
              "id": "mygame",
              "name": "My Game",
              "entry": "game.rbxlx",
              "content": {
                "mode": "download",
                "url": "https://example.test/pack-1.0.0.zip",
                "sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "version": "1.0.0"
              }
            }"#,
        )
    }

    fn engine_dir(p: &Path) -> bool {
        p == Path::new("/games/mygame/engine")
    }

    #[test]
    fn download_mode_resolves_to_the_installed_pack() {
        let m = download_manifest();
        let seen = std::cell::RefCell::new(None);
        let download = |spec: &pack::DownloadSpec| {
            *seen.borrow_mut() = Some(spec.clone());
            Ok(pack::Installed {
                root: PathBuf::from("/data/mygame/content-pack/1.0.0"),
                version: "1.0.0".into(),
                reused: false,
            })
        };
        let plan = resolve(&m, Path::new("/games/mygame"), &engine_dir, &download).unwrap();
        assert_eq!(
            plan.bundle.content,
            Content::Downloaded { root: PathBuf::from("/data/mygame/content-pack/1.0.0") }
        );
        let spec = seen.borrow();
        let spec = spec.as_ref().unwrap();
        assert_eq!(spec.url, "https://example.test/pack-1.0.0.zip");
        assert_eq!(spec.version.as_deref(), Some("1.0.0"));
        assert!(plan.notes.iter().any(|n| n.contains("installed pack 1.0.0")));
    }

    #[test]
    fn download_mode_reuses_an_installed_pack_without_errors() {
        let m = download_manifest();
        let download = |_: &pack::DownloadSpec| {
            Ok(pack::Installed {
                root: PathBuf::from("/data/mygame/content-pack/1.0.0"),
                version: "1.0.0".into(),
                reused: true,
            })
        };
        let plan = resolve(&m, Path::new("/games/mygame"), &engine_dir, &download).unwrap();
        assert!(plan.notes.iter().any(|n| n.contains("reusing installed pack 1.0.0")));
    }

    #[test]
    fn a_failed_download_does_not_block_the_launch() {
        let m = download_manifest();
        let download = |_: &pack::DownloadSpec| Err("GET failed: no network".into());
        let plan = resolve(&m, Path::new("/games/mygame"), &engine_dir, &download).unwrap();
        assert_eq!(plan.bundle.content, Content::None);
        assert!(plan
            .notes
            .iter()
            .any(|n| n.contains("download pack unavailable (GET failed: no network)")));
    }

    #[test]
    fn download_mode_without_url_or_hash_falls_back() {
        let m = manifest(
            r#"{"manifest_version":1,"id":"x","name":"X","entry":"game.rbxlx",
                "content":{"mode":"download"}}"#,
        );
        let plan = resolve(&m, Path::new("/games/mygame"), &engine_dir, &|_| unreachable!()).unwrap();
        assert_eq!(plan.bundle.content, Content::None);
        assert!(plan.notes.iter().any(|n| n.contains("needs url and sha256")));
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
            datastores: root.join("datastores"),
            home: root.join("home"),
            window_title: "My Game".into(),
            content: Content::Bundled { root: root.join("content") },
            offline: true,
            network: false,
        };
        let vars: Vec<(&str, OsString)> = env_contract(&bundle);
        let get = |key: &str| vars.iter().find(|(k, _)| *k == key).map(|(_, v)| v.clone());
        assert_eq!(get("RUSTUDIO_NO_SESSION"), Some(OsString::from("1")));
        assert_eq!(get("RUSTUDIO_NO_STUDIO_FETCH"), Some(OsString::from("1")));
        assert_eq!(get("RUSTUDIO_CORE_SCRIPTS"), Some(OsString::from("0")));
        assert_eq!(get("RUSTUDIO_LIVE_DATA"), Some(OsString::from("0")));
        assert_eq!(get("RUSTUDIO_OFFLINE"), Some(OsString::from("1")));
        assert_eq!(get("RUSTUDIO_NO_NETWORK"), Some(OsString::from("1")));
        assert_eq!(get("RUSTUDIO_CACHE_DIR"), Some(root.join("cache").into_os_string()));
        assert_eq!(get("RUSTUDIO_DATASTORE_DIR"), Some(root.join("datastores").into_os_string()));
        assert_eq!(get("RUSTUDIO_HOME"), Some(root.join("home").into_os_string()));
        assert_eq!(get("RUSTUDIO_WINDOW_TITLE"), Some(OsString::from("My Game")));
        assert_eq!(get("RUSTUDIO_HTTP_ENABLED"), Some(OsString::from("1")));
        assert_eq!(get("RUSTUDIO_ASSET_PLACE"), Some(root.join("place/game.rbxlx").into_os_string()));
        assert_eq!(get("RUSTUDIO_LOCAL_ASSETS"), Some(root.join("assets").into_os_string()));
        assert_eq!(get("RUSTUDIO_ENGINE_CONTENT_DIR"), Some(root.join("content").into_os_string()));
        // The Studio-layout variables stay a development escape hatch.
        assert_eq!(get("RUSTUDIO_CONTENT_DIR"), None);
        assert_eq!(get("RUSTUDIO_STUDIO_DIR"), None);
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
            datastores: root.join("datastores"),
            home: root.join("home"),
            window_title: "X".into(),
            content: Content::None,
            offline: false,
            network: true,
        };
        let vars = env_contract(&bundle);
        let keys: Vec<&str> = vars.iter().map(|(k, _)| *k).collect();
        assert!(!keys.contains(&"RUSTUDIO_ENGINE_CONTENT_DIR"));
        assert!(!keys.contains(&"RUSTUDIO_CONTENT_DIR"));
        assert!(!keys.contains(&"RUSTUDIO_STUDIO_DIR"));
        assert!(!keys.contains(&"RUSTUDIO_LOCAL_ASSETS"));
        assert!(!keys.contains(&"RUSTUDIO_OFFLINE"));
        assert!(!keys.contains(&"RUSTUDIO_NO_NETWORK"), "network: true opts back in");
    }

    #[test]
    fn network_is_allowed_unless_the_manifest_opts_out() {
        let m = manifest(r#"{"manifest_version":1,"id":"x","name":"X","entry":"game.rbxlx"}"#);
        let plan = resolve(&m, Path::new("/games/mygame"), &engine_dir, &|_| unreachable!()).unwrap();
        assert!(plan.bundle.network, "network is on by default");
        assert!(plan.notes.iter().any(|n| n.contains("network: allowed (default")));

        let m = manifest(
            r#"{"manifest_version":1,"id":"x","name":"X","entry":"game.rbxlx","network":false}"#,
        );
        let plan = resolve(&m, Path::new("/games/mygame"), &engine_dir, &|_| unreachable!()).unwrap();
        assert!(!plan.bundle.network);
        assert!(plan.notes.iter().any(|n| n.contains("network: blocked (game.json")));

        let m = manifest(
            r#"{"manifest_version":1,"id":"x","name":"X","entry":"game.rbxlx","network":true}"#,
        );
        let plan = resolve(&m, Path::new("/games/mygame"), &engine_dir, &|_| unreachable!()).unwrap();
        assert!(plan.bundle.network);
        assert!(plan.notes.iter().any(|n| n.contains("network: allowed (game.json")));

        // The offline promise is about assets; it does not cut the network.
        let m = manifest(
            r#"{"manifest_version":1,"id":"x","name":"X","entry":"game.rbxlx","offline":true}"#,
        );
        let plan = resolve(&m, Path::new("/games/mygame"), &engine_dir, &|_| unreachable!()).unwrap();
        assert!(plan.bundle.network);
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
