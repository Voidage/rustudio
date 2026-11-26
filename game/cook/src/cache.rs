//! The bundle cache: `<cache>/assets/<id>_v<version>.bin` or
//! `<id>_vlatest.bin` (FORK-PLAN §4, §8.2). This module reads what a
//! playthrough left there and hashes what the bundle will ship.

use std::collections::BTreeMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use rustudio_core::assets::AssetRef;
use sha2::{Digest, Sha256};

/// One file in `<cache>/assets`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CacheEntry {
    pub asset: AssetRef,
    pub path: PathBuf,
    pub size: u64,
    pub sha256: String,
}

/// Parses a cache file name: `<id>_vlatest.bin` or `<id>_v<version>.bin`.
pub fn parse_cache_name(name: &str) -> Option<AssetRef> {
    let stem = name.strip_suffix(".bin")?;
    let (id, version) = stem.split_once("_v")?;
    if id.is_empty() || !id.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let id: u64 = id.parse().ok()?;
    let version = match version {
        "latest" => None,
        v if !v.is_empty() && v.chars().all(|c| c.is_ascii_digit()) => Some(v.parse().ok()?),
        _ => return None,
    };
    Some(AssetRef { id, version })
}

/// Every asset file under `<cache>/assets`, in id order. Other files in the
/// cache (pipeline caches, thumbs) are not game assets and are ignored.
pub fn entries(cache: &Path) -> BTreeMap<(u64, Option<u64>), CacheEntry> {
    let dir = cache.join("assets");
    let mut out = BTreeMap::new();
    let Ok(read) = fs::read_dir(&dir) else { return out };
    for entry in read.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else { continue };
        let Some(asset) = parse_cache_name(name) else { continue };
        if !path.is_file() {
            continue;
        }
        let Ok(size) = entry.metadata().map(|m| m.len()) else { continue };
        let sha256 = match sha256_file(&path) {
            Ok(h) => h,
            Err(_) => continue,
        };
        out.insert((asset.id, asset.version), CacheEntry { asset, path, size, sha256 });
    }
    out
}

/// One cache file's entry, hashed on demand.
pub fn entry(cache: &Path, asset: AssetRef) -> Option<CacheEntry> {
    let path = expected(cache, asset);
    if !path.is_file() {
        return None;
    }
    let size = fs::metadata(&path).ok()?.len();
    let sha256 = sha256_file(&path).ok()?;
    Some(CacheEntry { asset, path, size, sha256 })
}

pub fn sha256_file(path: &Path) -> Result<String, String> {
    let mut file = fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf).map_err(|e| format!("{}: {e}", path.display()))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex(&hasher.finalize()))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// The file the engine checks first for an asset: the exact version when the
/// id names one, else `_vlatest`.
pub fn expected(cache: &Path, asset: AssetRef) -> PathBuf {
    let version = asset.version.map(|v| v.to_string()).unwrap_or_else(|| "latest".into());
    cache.join("assets").join(format!("{}_v{version}.bin", asset.id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_names_parse() {
        assert_eq!(parse_cache_name("900000001_vlatest.bin"), Some(AssetRef { id: 900000001, version: None }));
        assert_eq!(parse_cache_name("5_v12.bin"), Some(AssetRef { id: 5, version: Some(12) }));
        assert_eq!(parse_cache_name("5_v.bin"), None);
        assert_eq!(parse_cache_name("5.bin"), None);
        assert_eq!(parse_cache_name("abc_vlatest.bin"), None);
        assert_eq!(parse_cache_name("5_vlatest.wav"), None);
    }

    #[test]
    fn entries_reads_only_asset_files() {
        let dir = std::env::temp_dir().join(format!("rustudio-cook-cache-{}", std::process::id()));
        let assets = dir.join("assets");
        fs::create_dir_all(&assets).unwrap();
        fs::write(assets.join("7_vlatest.bin"), b"seven").unwrap();
        fs::write(assets.join("8_v3.bin"), b"eight").unwrap();
        fs::write(assets.join("pipeline.bin"), b"not an asset").unwrap();
        fs::create_dir_all(dir.join("thumbs")).unwrap();
        let got = entries(&dir);
        assert_eq!(got.len(), 2, "{got:?}");
        let seven = &got[&(7, None)];
        assert_eq!(seven.size, 5);
        assert_eq!(seven.sha256.len(), 64);
        assert_eq!(got[&(8, Some(3))].size, 5);
        fs::remove_dir_all(&dir).unwrap();
    }
}
