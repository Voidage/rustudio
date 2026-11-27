//! gamepack: the editor content pack's build/version/publish tool
//! (FORK-PLAN §7.1).
//!
//! A pack is the engine's defaults for a shipped game: PlayerModule, Animate,
//! the R6/R15 bodies, fonts and materials. The runtime only consumes a
//! versioned, hashed folder; this tool builds that folder from
//! `game/content`, bumps its version, verifies a copy, and publishes it.
//!
//! Layout (relative to the pack root, the engine's `RUSTUDIO_STUDIO_DIR`):
//!
//! ```text
//! content-pack.json                 this tool's manifest
//! content/...                       RUSTUDIO_CONTENT_DIR (avatar/, fonts/, ...)
//! ExtraContent/scripts/PlayerScripts/...   PlayerModule and its loader
//! ```
//!
//! `content-pack.json`:
//!
//! ```json
//! { "manifest_version": 1, "version": "0.1.0", "sha256": "...",
//!   "files": { "ExtraContent/...": "..." } }
//! ```
//!
//! The pack hash is SHA-256 over the sorted `path\0file-sha256\n` lines, so it
//! is deterministic and changes with any file's contents or a rename.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const MANIFEST: &str = "content-pack.json";
const MANIFEST_VERSION: u32 = 1;
const PLAYER_SCRIPTS: &str = "ExtraContent/scripts/PlayerScripts";

const USAGE: &str = "\
usage: gamepack COMMAND

  build <SOURCE> --out DIR [--version X.Y.Z]
        copy the pack source to DIR with content-pack.json
  update <PACK> [--version X.Y.Z]
        rehash PACK and bump its version (patch by default)
  verify <PACK>
        recompute the hashes and report differences (exit 1 on a mismatch)
  publish <PACK> <TARGET>
        copy PACK to TARGET/<version>/ after verifying it";

fn main() {
    match run(std::env::args().skip(1).collect()) {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            eprintln!("gamepack: {error}\n\n{USAGE}");
            std::process::exit(2);
        }
    }
}

fn run(args: Vec<String>) -> Result<i32, String> {
    let mut it = args.into_iter();
    let Some(command) = it.next() else {
        return Err("a command is needed".into());
    };
    match command.as_str() {
        "-h" | "--help" | "help" => {
            println!("{USAGE}");
            Ok(0)
        }
        "build" => {
            let source = it.next().ok_or("build needs a SOURCE directory")?;
            let (out, version) = options(&mut it)?;
            let out = out.ok_or("build needs --out DIR")?;
            build(Path::new(&source), Path::new(&out), &version.unwrap_or_else(|| "0.1.0".into()))?;
            Ok(0)
        }
        "update" => {
            let pack = it.next().ok_or("update needs a PACK directory")?;
            let (_, version) = options(&mut it)?;
            update(Path::new(&pack), version)?;
            Ok(0)
        }
        "verify" => {
            let pack = it.next().ok_or("verify needs a PACK directory")?;
            let problems = verify(Path::new(&pack))?;
            for p in &problems {
                println!("  - {p}");
            }
            Ok(if problems.is_empty() { 0 } else { 1 })
        }
        "publish" => {
            let pack = it.next().ok_or("publish needs a PACK directory")?;
            let target = it.next().ok_or("publish needs a TARGET directory")?;
            publish(Path::new(&pack), Path::new(&target))?;
            Ok(0)
        }
        other => Err(format!("unknown command {other}")),
    }
}

/// `--out` and `--version` after a command.
fn options(it: &mut impl Iterator<Item = String>) -> Result<(Option<String>, Option<String>), String> {
    let (mut out, mut version) = (None, None);
    while let Some(flag) = it.next() {
        match flag.as_str() {
            "--out" => out = Some(it.next().ok_or("--out needs a directory")?),
            "--version" => version = Some(it.next().ok_or("--version needs X.Y.Z")?),
            other => return Err(format!("unknown option {other}")),
        }
    }
    Ok((out, version))
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct Manifest {
    manifest_version: u32,
    version: String,
    /// SHA-256 over the sorted `path\0file-sha256\n` lines.
    sha256: String,
    /// Relative path → SHA-256 of its bytes.
    files: BTreeMap<String, String>,
}

fn build(source: &Path, out: &Path, version: &str) -> Result<(), String> {
    if !source.join(PLAYER_SCRIPTS).is_dir() {
        return Err(format!(
            "{}: no {PLAYER_SCRIPTS}/ (a pack needs the PlayerScripts the engine reads)",
            source.display()
        ));
    }
    validate_version(version)?;
    if out.exists() {
        let empty = fs::read_dir(out).map(|mut d| d.next().is_none()).unwrap_or(false);
        if !empty {
            return Err(format!("{} already exists and is not empty", out.display()));
        }
    } else {
        fs::create_dir_all(out).map_err(|e| format!("{}: {e}", out.display()))?;
    }
    let files = collect(source)?;
    for rel in files.keys() {
        let from = source.join(rel);
        let to = out.join(rel);
        if let Some(dir) = to.parent() {
            fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        fs::copy(&from, &to).map_err(|e| format!("{}: {e}", from.display()))?;
    }
    let manifest = Manifest {
        manifest_version: MANIFEST_VERSION,
        version: version.to_string(),
        sha256: pack_hash(&files),
        files,
    };
    write_manifest(out, &manifest)?;
    println!(
        "gamepack: built {} file(s), version {}, sha256 {}",
        manifest.files.len(),
        manifest.version,
        manifest.sha256
    );
    Ok(())
}

fn update(pack: &Path, version: Option<String>) -> Result<(), String> {
    let mut manifest = read_manifest(pack)?;
    let files = collect(pack)?;
    manifest.version = match version {
        Some(v) => {
            validate_version(&v)?;
            v
        }
        None => bump(&manifest.version)?,
    };
    manifest.manifest_version = MANIFEST_VERSION;
    manifest.sha256 = pack_hash(&files);
    manifest.files = files;
    write_manifest(pack, &manifest)?;
    println!(
        "gamepack: updated to version {}, sha256 {}",
        manifest.version, manifest.sha256
    );
    Ok(())
}

fn verify(pack: &Path) -> Result<Vec<String>, String> {
    let manifest = read_manifest(pack)?;
    let files = collect(pack)?;
    let mut problems = Vec::new();
    for (rel, sha) in &manifest.files {
        match files.get(rel) {
            None => problems.push(format!("missing: {rel}")),
            Some(now) if now != sha => problems.push(format!("changed: {rel}")),
            Some(_) => {}
        }
    }
    for rel in files.keys() {
        if !manifest.files.contains_key(rel) {
            problems.push(format!("new: {rel}"));
        }
    }
    let hash = pack_hash(&files);
    if hash != manifest.sha256 {
        problems.push(format!(
            "sha256: manifest {} != contents {hash}",
            manifest.sha256
        ));
    }
    if problems.is_empty() {
        println!("gamepack: {} is intact (version {}, {} files)", pack.display(), manifest.version, manifest.files.len());
    }
    Ok(problems)
}

fn publish(pack: &Path, target: &Path) -> Result<(), String> {
    let problems = verify(pack)?;
    if !problems.is_empty() {
        return Err(format!("{} does not verify ({} problem(s)); not publishing", pack.display(), problems.len()));
    }
    let manifest = read_manifest(pack)?;
    let dest = target.join(&manifest.version);
    if dest.exists() {
        return Err(format!("{} already exists", dest.display()));
    }
    fs::create_dir_all(&dest).map_err(|e| format!("{}: {e}", dest.display()))?;
    let files = collect(pack)?;
    for rel in files.keys() {
        let from = pack.join(rel);
        let to = dest.join(rel);
        if let Some(dir) = to.parent() {
            fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        fs::copy(&from, &to).map_err(|e| format!("{}: {e}", from.display()))?;
    }
    fs::copy(pack.join(MANIFEST), dest.join(MANIFEST))
        .map_err(|e| format!("{}: {e}", dest.display()))?;
    println!("gamepack: published {} to {}", manifest.version, dest.display());
    Ok(())
}

/// Every pack file (relative, absolute), sorted; the manifest itself is not
/// part of the pack's file list.
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
        let rel = path.strip_prefix(root).map_err(|e| e.to_string())?.to_string_lossy().replace('\\', "/");
        if rel == MANIFEST || rel.ends_with(".tmp") {
            continue;
        }
        let sha = sha256_file(&path)?;
        out.insert(rel, sha);
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

fn sha256_file(path: &Path) -> Result<String, String> {
    let bytes = fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(hex(&Sha256::digest(&bytes)))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn read_manifest(pack: &Path) -> Result<Manifest, String> {
    let path = pack.join(MANIFEST);
    let text = fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let manifest: Manifest = serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    if manifest.manifest_version != MANIFEST_VERSION {
        return Err(format!(
            "{}: manifest_version {} (this gamepack reads {MANIFEST_VERSION})",
            path.display(),
            manifest.manifest_version
        ));
    }
    Ok(manifest)
}

fn write_manifest(pack: &Path, manifest: &Manifest) -> Result<(), String> {
    let path = pack.join(MANIFEST);
    let json = serde_json::to_string_pretty(manifest).map_err(|e| e.to_string())?;
    fs::write(&path, format!("{json}\n")).map_err(|e| format!("{}: {e}", path.display()))
}

fn validate_version(version: &str) -> Result<(), String> {
    let parts: Vec<&str> = version.split('.').collect();
    if parts.len() != 3 || parts.iter().any(|p| p.is_empty() || !p.chars().all(|c| c.is_ascii_digit())) {
        return Err(format!("version {version:?} is not X.Y.Z"));
    }
    Ok(())
}

fn bump(version: &str) -> Result<String, String> {
    validate_version(version)?;
    let mut parts: Vec<u64> = version.split('.').map(|p| p.parse().unwrap_or(0)).collect();
    parts[2] += 1;
    Ok(format!("{}.{}.{}", parts[0], parts[1], parts[2]))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "rustudio-gamepack-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn source(root: &Path) -> PathBuf {
        let src = root.join("pack");
        fs::create_dir_all(src.join(PLAYER_SCRIPTS)).unwrap();
        fs::create_dir_all(src.join("content/avatar")).unwrap();
        fs::write(src.join(PLAYER_SCRIPTS).join("PlayerScriptsLoader.lua"), "-- loader\n").unwrap();
        fs::write(src.join("content/avatar/note.txt"), "body\n").unwrap();
        src
    }

    #[test]
    fn build_is_deterministic_and_verifies() {
        let root = temp("build");
        let src = source(&root);
        let a = root.join("a");
        let b = root.join("b");
        build(&src, &a, "1.2.3").unwrap();
        build(&src, &b, "1.2.3").unwrap();
        let ma = fs::read_to_string(a.join(MANIFEST)).unwrap();
        let mb = fs::read_to_string(b.join(MANIFEST)).unwrap();
        assert_eq!(ma, mb, "build must be deterministic");
        assert!(ma.contains("\"version\": \"1.2.3\""));
        assert_eq!(verify(&a).unwrap(), Vec::<String>::new());
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn update_bumps_and_rehashes_after_an_edit() {
        let root = temp("update");
        let src = source(&root);
        let out = root.join("out");
        build(&src, &out, "1.2.3").unwrap();
        let before: Manifest = read_manifest(&out).unwrap();
        fs::write(out.join("content/avatar/note.txt"), "different body\n").unwrap();
        assert!(!verify(&out).unwrap().is_empty());
        update(&out, None).unwrap();
        let after: Manifest = read_manifest(&out).unwrap();
        assert_eq!(after.version, "1.2.4");
        assert_ne!(after.sha256, before.sha256);
        assert_eq!(verify(&out).unwrap(), Vec::<String>::new());
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn publish_copies_the_versioned_pack() {
        let root = temp("publish");
        let src = source(&root);
        let out = root.join("out");
        let target = root.join("target");
        build(&src, &out, "0.4.2").unwrap();
        publish(&out, &target).unwrap();
        assert!(target.join("0.4.2").join(PLAYER_SCRIPTS).join("PlayerScriptsLoader.lua").is_file());
        assert!(target.join("0.4.2").join(MANIFEST).is_file());
        assert!(publish(&out, &target).is_err(), "a published version is immutable");
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn a_pack_needs_player_scripts_and_a_valid_version() {
        let root = temp("invalid");
        let empty = root.join("empty");
        fs::create_dir_all(&empty).unwrap();
        assert!(build(&empty, &root.join("x"), "1.0.0").is_err());
        let src = source(&root);
        assert!(build(&src, &root.join("y"), "1.0").is_err());
        fs::remove_dir_all(&root).unwrap();
    }
}
