//! Download mode for the content pack (FORK-PLAN §7.1, rule 3).
//!
//! The launcher fetches a versioned archive, checks the pack hash that
//! `gamepack` wrote, installs it under the game's per-user data directory and
//! reuses it on later launches. The pack archive is a zip whose root is the
//! pack (the same files `gamepack build` writes, `content-pack.json`
//! included).
//!
//! Failure never blocks the game: the caller logs the reason and launches
//! with the engine's own content/fallback.

use std::collections::BTreeMap;
use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Deserialize;
use sha2::{Digest, Sha256};

const PACK_MANIFEST: &str = "content-pack.json";
const MANIFEST_VERSION: u32 = 1;
const INSTALL_DIR: &str = "content-pack";

/// A `content.mode == "download"` manifest entry.
#[derive(Debug, Clone, PartialEq)]
pub struct DownloadSpec {
    pub url: String,
    /// The pack hash: SHA-256 over the sorted `path\0file-sha256\n` lines
    /// (`gamepack` writes the same value into `content-pack.json`).
    pub sha256: String,
    /// Optional: reject a pack whose own version differs.
    pub version: Option<String>,
}

#[derive(Debug, PartialEq)]
pub struct Installed {
    pub root: PathBuf,
    pub version: String,
    /// True when an already installed pack satisfied the request.
    pub reused: bool,
}

#[derive(Debug, Deserialize)]
struct PackManifest {
    manifest_version: u32,
    version: String,
    sha256: String,
}

/// `RUSTUDIO_GAME_DATA_DIR`, else the platform's per-user data directory.
pub fn data_root() -> Result<PathBuf, String> {
    if let Some(p) = std::env::var_os("RUSTUDIO_GAME_DATA_DIR").filter(|p| !p.is_empty()) {
        return Ok(PathBuf::from(p));
    }
    dirs::data_dir()
        .map(|d| d.join("rustudio-games"))
        .ok_or_else(|| "no per-user data directory on this platform".into())
}

/// `<data>/<game-id>/content-pack`, the folder holding every installed
/// version of one game's pack.
pub fn install_root(base: &Path, id: &str) -> PathBuf {
    base.join(safe_id(id)).join(INSTALL_DIR)
}

fn safe_id(id: &str) -> String {
    let cleaned: String = id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') {
                c
            } else {
                '_'
            }
        })
        .collect();
    let cleaned = cleaned.trim_matches('.').to_string();
    if cleaned.is_empty() {
        "game".into()
    } else {
        cleaned.chars().take(80).collect()
    }
}

/// Reuse an installed pack, else fetch and install one.
///
/// `fetch` is injectable so the install/reuse rules can be tested without a
/// server; `file://` URLs are served by `fetch` itself.
pub fn ensure(
    base: &Path,
    id: &str,
    spec: &DownloadSpec,
    allow_network: bool,
    fetch: &dyn Fn(&str) -> Result<Vec<u8>, String>,
) -> Result<Installed, String> {
    if spec.sha256.len() != 64 || !spec.sha256.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(format!(
            "content.sha256 is not a SHA-256 hex digest: {:?}",
            spec.sha256
        ));
    }
    let install = install_root(base, id);
    if let Some(found) = reuse(&install, spec) {
        return Ok(found);
    }
    if !allow_network {
        return Err("the pack is not installed and offline mode forbids the download".into());
    }
    let bytes = fetch(&spec.url)?;
    install_archive(&install, &bytes, spec)
}

fn reuse(install: &Path, spec: &DownloadSpec) -> Option<Installed> {
    for entry in fs::read_dir(install).ok()?.flatten() {
        let root = entry.path();
        if !root.is_dir() {
            continue;
        }
        let Ok(manifest) = read_manifest(&root) else {
            continue;
        };
        if manifest.sha256 != spec.sha256 {
            continue;
        }
        if let Some(want) = &spec.version {
            if &manifest.version != want {
                continue;
            }
        }
        return Some(Installed {
            root,
            version: manifest.version,
            reused: true,
        });
    }
    None
}

fn install_archive(install: &Path, bytes: &[u8], spec: &DownloadSpec) -> Result<Installed, String> {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let incoming = install.join(format!(".incoming-{}-{stamp}", std::process::id()));
    let result = install_into(&incoming, install, bytes, spec);
    if result.is_err() {
        let _ = fs::remove_dir_all(&incoming);
    }
    result
}

fn install_into(
    incoming: &Path,
    install: &Path,
    bytes: &[u8],
    spec: &DownloadSpec,
) -> Result<Installed, String> {
    let _ = fs::remove_dir_all(incoming);
    extract_zip(bytes, incoming)?;
    let manifest = read_manifest(incoming)?;
    if manifest.sha256 != spec.sha256 {
        return Err(format!(
            "pack hash {} does not match content.sha256 {}",
            manifest.sha256, spec.sha256
        ));
    }
    if let Some(want) = &spec.version {
        if &manifest.version != want {
            return Err(format!(
                "pack version {} does not match content.version {want}",
                manifest.version
            ));
        }
    }
    validate_version(&manifest.version)?;
    let files = collect(incoming)?;
    let hash = pack_hash(&files);
    if hash != spec.sha256 {
        return Err(format!(
            "unpacked pack hashes to {hash}, but content.sha256 is {}",
            spec.sha256
        ));
    }
    fs::create_dir_all(install).map_err(|e| format!("{}: {e}", install.display()))?;
    let target = install.join(&manifest.version);
    // A same-version folder that failed `reuse` is broken; replace it.
    if target.exists() {
        fs::remove_dir_all(&target).map_err(|e| format!("{}: {e}", target.display()))?;
    }
    fs::rename(incoming, &target).map_err(|e| format!("{}: {e}", target.display()))?;
    Ok(Installed {
        root: target,
        version: manifest.version,
        reused: false,
    })
}

fn extract_zip(bytes: &[u8], dest: &Path) -> Result<(), String> {
    let mut archive =
        zip::ZipArchive::new(Cursor::new(bytes)).map_err(|e| format!("pack archive: {e}"))?;
    fs::create_dir_all(dest).map_err(|e| format!("{}: {e}", dest.display()))?;
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|e| format!("pack archive entry {index}: {e}"))?;
        let Some(relative) = entry.enclosed_name() else {
            let name = entry
                .name()
                .map(|n| n.to_string())
                .unwrap_or_else(|_| "?".into());
            return Err(format!("pack archive has an unsafe path: {name}"));
        };
        if relative.as_os_str().is_empty() {
            continue;
        }
        let out = dest.join(&relative);
        if entry.is_dir() {
            fs::create_dir_all(&out).map_err(|e| format!("{}: {e}", out.display()))?;
            continue;
        }
        if let Some(dir) = out.parent() {
            fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        let mut file = fs::File::create(&out).map_err(|e| format!("{}: {e}", out.display()))?;
        std::io::copy(&mut entry, &mut file).map_err(|e| format!("{}: {e}", out.display()))?;
    }
    Ok(())
}

/// The bytes behind a `content.url`: `file://` for a local pack (tests and
/// self-hosting from a mounted folder), http(s) through ureq otherwise.
pub fn fetch(url: &str) -> Result<Vec<u8>, String> {
    if let Some(path) = url.strip_prefix("file://") {
        return fs::read(path).map_err(|e| format!("{url}: {e}"));
    }
    let mut response = ureq::get(url)
        .call()
        .map_err(|e| format!("GET {url}: {e}"))?;
    response
        .body_mut()
        .read_to_vec()
        .map_err(|e| format!("GET {url}: {e}"))
}

fn read_manifest(pack: &Path) -> Result<PackManifest, String> {
    let path = pack.join(PACK_MANIFEST);
    let text = fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let manifest: PackManifest =
        serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    if manifest.manifest_version != MANIFEST_VERSION {
        return Err(format!(
            "{}: manifest_version {} (this launcher reads {MANIFEST_VERSION})",
            path.display(),
            manifest.manifest_version
        ));
    }
    Ok(manifest)
}

fn validate_version(version: &str) -> Result<(), String> {
    let parts: Vec<&str> = version.split('.').collect();
    if parts.len() != 3
        || parts
            .iter()
            .any(|p| p.is_empty() || !p.chars().all(|c| c.is_ascii_digit()))
    {
        return Err(format!("pack version {version:?} is not X.Y.Z"));
    }
    Ok(())
}

/// Every pack file (relative, sha256), sorted; the manifest itself is not
/// part of the pack's file list. Same rules as `gamepack`.
fn collect(root: &Path) -> Result<BTreeMap<String, String>, String> {
    let mut out = BTreeMap::new();
    walk(root, root, &mut out)?;
    Ok(out)
}

fn walk(root: &Path, dir: &Path, out: &mut BTreeMap<String, String>) -> Result<(), String> {
    let entries = fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let mut paths: Vec<PathBuf> = entries.filter_map(|e| e.ok().map(|e| e.path())).collect();
    paths.sort();
    for path in paths {
        if path.is_dir() {
            walk(root, &path, out)?;
            continue;
        }
        let rel = path
            .strip_prefix(root)
            .map_err(|e| e.to_string())?
            .to_string_lossy()
            .replace('\\', "/");
        if rel == PACK_MANIFEST || rel.ends_with(".tmp") {
            continue;
        }
        let bytes = fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        out.insert(rel, hex(&Sha256::digest(&bytes)));
    }
    Ok(())
}

fn pack_hash(files: &BTreeMap<String, String>) -> String {
    let mut hasher = Sha256::new();
    for (rel, sha) in files {
        hasher.update(rel.as_bytes());
        hasher.update(b"\0");
        hasher.update(sha.as_bytes());
        hasher.update(b"\n");
    }
    hex(&hasher.finalize())
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "rustudio-launcher-pack-{name}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A minimal pack source; returns its pack hash.
    fn pack_source(root: &Path) -> String {
        fs::create_dir_all(root.join("content/avatar")).unwrap();
        fs::write(root.join("content/avatar/character.rbxm"), b"body").unwrap();
        fs::create_dir_all(root.join("ExtraContent/scripts/PlayerScripts")).unwrap();
        fs::write(
            root.join("ExtraContent/scripts/PlayerScripts/PlayerModule.module.lua"),
            b"return {}",
        )
        .unwrap();
        let files = collect(root).unwrap();
        let hash = pack_hash(&files);
        let manifest = serde_json::json!({
            "manifest_version": 1,
            "version": "1.2.3",
            "sha256": hash,
            "files": files,
        });
        fs::write(root.join(PACK_MANIFEST), manifest.to_string()).unwrap();
        hash
    }

    fn zip_of(root: &Path) -> Vec<u8> {
        let mut bytes = Cursor::new(Vec::new());
        {
            let mut zip = zip::ZipWriter::new(&mut bytes);
            let options = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated);
            let mut names: Vec<PathBuf> = Vec::new();
            fn gather(root: &Path, dir: &Path, out: &mut Vec<PathBuf>) {
                for entry in fs::read_dir(dir).unwrap().flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        gather(root, &path, out);
                    } else {
                        out.push(path.strip_prefix(root).unwrap().to_path_buf());
                    }
                }
            }
            gather(root, root, &mut names);
            names.sort();
            for rel in names {
                zip.start_file(rel.to_string_lossy().replace('\\', "/"), options)
                    .unwrap();
                let mut file = fs::File::open(root.join(&rel)).unwrap();
                std::io::copy(&mut file, &mut zip).unwrap();
            }
            zip.finish().unwrap();
        }
        bytes.into_inner()
    }

    fn spec(hash: &str) -> DownloadSpec {
        DownloadSpec {
            url: "file:///unused-pack.zip".into(),
            sha256: hash.into(),
            version: None,
        }
    }

    #[test]
    fn installs_verifies_and_reuses_the_pack() {
        let source = temp("source");
        let base = temp("data");
        let hash = pack_source(&source);
        let archive = zip_of(&source);
        let calls = std::cell::Cell::new(0);
        let fetch = |_: &str| {
            calls.set(calls.get() + 1);
            Ok(archive.clone())
        };

        let first = ensure(&base, "mygame", &spec(&hash), true, &fetch).unwrap();
        assert!(!first.reused);
        assert_eq!(first.version, "1.2.3");
        assert!(first.root.ends_with("mygame/content-pack/1.2.3"));
        assert_eq!(
            fs::read(first.root.join("content/avatar/character.rbxm")).unwrap(),
            b"body"
        );

        let second = ensure(&base, "mygame", &spec(&hash), true, &fetch).unwrap();
        assert!(second.reused);
        assert_eq!(second.root, first.root);
        assert_eq!(calls.get(), 1);
        let _ = fs::remove_dir_all(&source);
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn a_bad_hash_is_rejected_and_leaves_nothing_behind() {
        let source = temp("bad");
        let base = temp("bad-data");
        let hash = pack_source(&source);
        let archive = zip_of(&source);
        let fetch = |_: &str| Ok(archive.clone());
        let wrong = spec(&"0".repeat(64));
        assert!(ensure(&base, "mygame", &wrong, true, &fetch).is_err());
        let install = install_root(&base, "mygame");
        let leftovers: Vec<_> = fs::read_dir(&install)
            .map(|d| d.flatten().map(|e| e.file_name()).collect())
            .unwrap_or_default();
        assert!(leftovers.is_empty(), "{leftovers:?}");
        assert!(!install.join("1.2.3").exists());
        assert_eq!(pack_source(&source), hash);
        let _ = fs::remove_dir_all(&source);
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn offline_does_not_fetch() {
        let base = temp("offline");
        let fetch = |_: &str| panic!("offline must not fetch");
        let err = ensure(&base, "mygame", &spec(&"0".repeat(64)), false, &fetch).unwrap_err();
        assert!(err.contains("offline"));
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn a_different_version_is_not_reused() {
        let source = temp("version");
        let base = temp("version-data");
        let hash = pack_source(&source);
        let archive = zip_of(&source);
        let fetch = |_: &str| Ok(archive.clone());
        ensure(&base, "mygame", &spec(&hash), true, &fetch).unwrap();
        let mut other = spec(&hash);
        other.version = Some("2.0.0".into());
        assert!(ensure(&base, "mygame", &other, true, &fetch).is_err());
        let _ = fs::remove_dir_all(&source);
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn an_archive_cannot_write_outside_the_install_folder() {
        let base = temp("traversal");
        let mut bytes = Cursor::new(Vec::new());
        {
            let mut zip = zip::ZipWriter::new(&mut bytes);
            let options = zip::write::SimpleFileOptions::default();
            zip.start_file("../escaped.txt", options).unwrap();
            zip.write_all(b"no").unwrap();
            zip.finish().unwrap();
        }
        let dest = base.join("incoming");
        assert!(extract_zip(&bytes.into_inner(), &dest).is_err());
        assert!(!base.join("escaped.txt").exists());
        let _ = fs::remove_dir_all(&base);
    }
}
