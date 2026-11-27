//! The Asset Manager's "This Place" view (FORK-PLAN §8.4 item 7).
//!
//! The capture ledger (`<cache>/capture-ledger.jsonl`, F4) already records
//! every cloud asset the open place requested, how it settled, where its
//! cache file is and how big it is. This view lists those records for the
//! open place and runs `rustudio-cook export` ("Save assets for offline
//! use"), the same tool a release bundle uses, so there is one exporter.
//!
//! Nothing here touches the network: the ledger and the cache are local.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;

/// One line of `<cache>/capture-ledger.jsonl` (written by
/// `rustudio_core::assets::capture_record`).
#[derive(Debug, Clone, Deserialize)]
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

/// One asset as the view shows it.
#[derive(Debug, Clone, PartialEq)]
pub struct LocalAsset {
    pub id: u64,
    pub version: Option<u64>,
    pub status: String,
    /// The cache file ("cached" rows only; empty for a failed request).
    pub file: PathBuf,
    pub size: u64,
    pub time: u64,
}

/// A file imported into the place's own `assets/` folder. Its reference is
/// the relative path (`assets/name.png`), which the engine resolves because
/// the place folder opts in with `.rustudio-local-content`.
#[derive(Debug, Clone, PartialEq)]
pub struct LocalImport {
    pub relative: String,
    pub file: PathBuf,
    pub size: u64,
}

#[derive(Debug, Default)]
pub struct LocalAssets {
    /// The place the rows belong to (canonical when it exists).
    pub place: Option<PathBuf>,
    /// The path as the editor asked for it, so the view does not re-read a
    /// canonicalized path every frame.
    pub requested: Option<PathBuf>,
    pub rows: Vec<LocalAsset>,
    /// Files imported into the place's own `assets/` folder.
    pub imports: Vec<LocalImport>,
    pub ledger: PathBuf,
    pub error: Option<String>,
    /// False until the first refresh for the current place.
    pub loaded: bool,
    /// (success, text) from the last export.
    pub export_status: Option<(bool, String)>,
}

impl LocalAssets {
    /// Reload the ledger for `place` from the real cache and ledger paths.
    pub fn refresh(&mut self, place: Option<&Path>) {
        let ledger = rustudio_core::assets::capture_ledger_path();
        let cache = rustudio_core::assets::cache_dir();
        self.refresh_from(place, &ledger, &cache);
    }

    fn refresh_from(&mut self, place: Option<&Path>, ledger: &Path, cache: &Path) {
        self.requested = place.map(Path::to_path_buf);
        self.place = place.map(|p| p.canonicalize().unwrap_or_else(|_| p.to_path_buf()));
        self.ledger = ledger.to_path_buf();
        self.rows.clear();
        self.imports.clear();
        self.error = None;
        self.loaded = true;
        let Some(place) = self.place.as_ref() else { return };
        // Local files live beside the place, not in the ledger.
        if let Some(dir) = place.parent() {
            if let Ok(entries) = std::fs::read_dir(dir.join("assets")) {
                for entry in entries.flatten() {
                    let file = entry.path();
                    if !file.is_file() {
                        continue;
                    }
                    let name = file.file_name().and_then(|n| n.to_str()).unwrap_or_default();
                    if name.is_empty() {
                        continue;
                    }
                    self.imports.push(LocalImport {
                        relative: format!("assets/{name}"),
                        size: entry.metadata().map(|m| m.len()).unwrap_or(0),
                        file,
                    });
                }
                self.imports.sort_by(|a, b| a.relative.cmp(&b.relative));
            }
        }
        let key = place.display().to_string();
        let text = match std::fs::read_to_string(ledger) {
            Ok(text) => text,
            // No ledger yet is a normal first run, not an error.
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return,
            Err(e) => {
                self.error = Some(format!("{}: {e}", ledger.display()));
                return;
            }
        };
        // The ledger is append-only; the last line for an id/version wins.
        let mut rows: BTreeMap<(u64, Option<u64>), LocalAsset> = BTreeMap::new();
        for line in text.lines() {
            let Ok(line) = serde_json::from_str::<LedgerLine>(line) else { continue };
            if line.place != key {
                continue;
            }
            let file = if line.path.is_empty() { PathBuf::new() } else { cache.join(&line.path) };
            rows.insert(
                (line.id, line.version),
                LocalAsset {
                    id: line.id,
                    version: line.version,
                    status: line.status,
                    file,
                    size: line.size,
                    time: line.time,
                },
            );
        }
        self.rows = rows.into_values().collect();
    }

    pub fn total_size(&self) -> u64 {
        self.rows.iter().map(|r| r.size).sum::<u64>()
            + self.imports.iter().map(|r| r.size).sum::<u64>()
    }

    /// Copy files into `<place folder>/assets/` and create the marker that
    /// lets the engine resolve them as relative place content. A name that
    /// already exists gets a `-1`, `-2` … suffix.
    pub fn import_local_files(place: &Path, files: &[PathBuf]) -> Result<Vec<LocalImport>, String> {
        let folder = place.parent().ok_or("the place has no folder")?;
        let dir = folder.join("assets");
        std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let marker = folder.join(rustudio_core::assets::LOCAL_CONTENT_MARKER);
        if !marker.exists() {
            std::fs::write(&marker, "Local files beside this place are place content (rustudio).\n")
                .map_err(|e| format!("{}: {e}", marker.display()))?;
        }
        let mut out = Vec::new();
        for src in files {
            let name = src.file_name().and_then(|n| n.to_str()).ok_or("a file has no name")?;
            let path = std::path::Path::new(name);
            let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("asset");
            let ext = path.extension().and_then(|s| s.to_str());
            let mut dest = dir.join(name);
            let mut n = 1;
            while dest.exists() {
                let candidate = match ext {
                    Some(ext) => format!("{stem}-{n}.{ext}"),
                    None => format!("{stem}-{n}"),
                };
                dest = dir.join(candidate);
                n += 1;
            }
            std::fs::copy(src, &dest)
                .map_err(|e| format!("{} -> {}: {e}", src.display(), dest.display()))?;
            let name = dest.file_name().and_then(|n| n.to_str()).unwrap_or_default();
            out.push(LocalImport {
                relative: format!("assets/{name}"),
                size: std::fs::metadata(&dest).map(|m| m.len()).unwrap_or(0),
                file: dest,
            });
        }
        Ok(out)
    }

    /// Run `rustudio-cook export` for this place into
    /// `<place folder>/offline-export/<place-stem>/`, beside the project and
    /// outside the download cache. Returns its output on success.
    pub fn export(&self, place: &Path) -> Result<String, String> {
        let cook = cook_binary().ok_or_else(|| {
            "rustudio-cook was not found. Build it with \
             `cargo build --release --manifest-path game/Cargo.toml`, or set RUSTUDIO_COOK."
                .to_string()
        })?;
        let stem = place.file_stem().and_then(|s| s.to_str()).unwrap_or("place");
        let folder = place
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let out = folder.join("offline-export").join(stem);
        let output = std::process::Command::new(&cook)
            .arg("export")
            .arg("--place")
            .arg(place)
            .arg("--out")
            .arg(&out)
            .output()
            .map_err(|e| format!("could not run {}: {e}", cook.display()))?;
        let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
        if !output.status.success() {
            text.push_str(&String::from_utf8_lossy(&output.stderr));
            return Err(format!("export failed ({}):\n{text}", output.status));
        }
        Ok(text)
    }
}

/// The cook binary beside the editor, from `RUSTUDIO_COOK`, or in the
/// downstream workspace's target folders (the development layout).
fn cook_binary() -> Option<PathBuf> {
    let name = if cfg!(windows) { "rustudio-cook.exe" } else { "rustudio-cook" };
    if let Some(p) = std::env::var_os("RUSTUDIO_COOK").filter(|p| !p.is_empty()) {
        return Some(PathBuf::from(p));
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let beside = dir.join(name);
            if beside.is_file() {
                return Some(beside);
            }
        }
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    for profile in ["release", "debug"] {
        let p = root.join("game/target").join(profile).join(name);
        if p.is_file() {
            return Some(p);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("rustudio-am-local-{name}-{}", std::process::id()))
    }

    fn line(place: &Path, id: u64, status: &str, size: u64) -> String {
        serde_json::json!({
            "time": 1, "place": place.display().to_string(), "id": id,
            "version": null, "status": status, "path": format!("assets/{id}_vlatest.bin"), "size": size,
        })
        .to_string()
    }

    #[test]
    fn filters_by_place_and_keeps_the_last_status() {
        let root = temp_root("filter");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let place = root.join("game.rbxlx");
        std::fs::write(&place, b"place").unwrap();
        let other = root.join("other.rbxlx");
        std::fs::write(&other, b"place").unwrap();
        let ledger = root.join("capture-ledger.jsonl");
        std::fs::write(
            &ledger,
            [
                line(&place, 1, "failed", 0),
                line(&other, 9, "cached", 99),
                line(&place, 1, "cached", 12),
                line(&place, 2, "cached", 30),
            ]
            .join("\n"),
        )
        .unwrap();
        let cache = root.join("cache");
        let mut local = LocalAssets::default();
        local.refresh_from(Some(&place), &ledger, &cache);
        assert_eq!(local.rows.len(), 2);
        assert_eq!(local.rows[0].id, 1);
        assert_eq!(local.rows[0].status, "cached");
        assert_eq!(local.rows[0].size, 12);
        assert_eq!(local.rows[0].file, cache.join("assets/1_vlatest.bin"));
        assert_eq!(local.total_size(), 42);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn imports_copy_assets_and_create_the_marker() {
        let root = temp_root("import");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("place")).unwrap();
        std::fs::create_dir_all(root.join("second")).unwrap();
        let place = root.join("place/game.rbxlx");
        std::fs::write(&place, b"place").unwrap();
        let first = root.join("a.png");
        std::fs::write(&first, b"1234").unwrap();
        let second = root.join("second/a.png");
        std::fs::write(&second, b"12").unwrap();

        let imports = LocalAssets::import_local_files(&place, &[first, second]).unwrap();
        assert_eq!(imports[0].relative, "assets/a.png");
        assert_eq!(imports[1].relative, "assets/a-1.png");
        assert!(root.join("place/.rustudio-local-content").is_file());
        assert_eq!(std::fs::read(root.join("place/assets/a.png")).unwrap(), b"1234");

        let mut local = LocalAssets::default();
        local.refresh_from(Some(&place), &root.join("none.jsonl"), &root.join("cache"));
        assert_eq!(local.imports.len(), 2);
        assert_eq!(local.total_size(), 6);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn missing_ledger_or_unsaved_place_is_empty_not_an_error() {
        let root = temp_root("empty");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let mut local = LocalAssets::default();
        local.refresh_from(None, &root.join("none.jsonl"), &root.join("cache"));
        assert!(local.rows.is_empty() && local.error.is_none());
        let place = root.join("game.rbxlx");
        std::fs::write(&place, b"place").unwrap();
        local.refresh_from(Some(&place), &root.join("none.jsonl"), &root.join("cache"));
        assert!(local.rows.is_empty() && local.error.is_none());
        let _ = std::fs::remove_dir_all(&root);
    }
}
