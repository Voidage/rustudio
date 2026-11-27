//! File-backed local DataStores (F5): one JSON record per key under
//! `RUSTUDIO_DATASTORE_DIR`.
//!
//! The Luau side (`data_local.rs`) owns the DataStore semantics (versions,
//! `DataStoreKeyInfo`, `UpdateAsync`); this module only moves opaque JSON
//! strings to and from disk. The layout is
//! `<root>/<hex(store)>/<hex(scope)>/<hex(key)>.json`, so a key can contain
//! any character and can never escape the root. Hex keeps `Key` and `key`
//! distinct on case-insensitive filesystems.
//!
//! Writes are atomic: write a unique temp file, fsync it, then rename over
//! the target. A crash leaves either the old record or the new one, never a
//! partial one.

use std::{
    fs,
    io,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

#[derive(Clone)]
pub struct Files {
    root: PathBuf,
}

impl Files {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    fn dir(&self, store: &str, scope: &str) -> PathBuf {
        self.root.join(hex(store)).join(hex(scope))
    }

    fn file(&self, store: &str, scope: &str, key: &str) -> PathBuf {
        self.dir(store, scope).join(format!("{}.json", hex(key)))
    }

    /// One record's JSON, or `None` when the key has never been written.
    pub fn read(&self, store: &str, scope: &str, key: &str) -> io::Result<Option<String>> {
        checked(store, scope, Some(key))?;
        match fs::read_to_string(self.file(store, scope, key)) {
            Ok(text) => Ok(Some(text)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Replace one record atomically. Creates the store/scope directories.
    pub fn write(&self, store: &str, scope: &str, key: &str, json: &str) -> io::Result<()> {
        checked(store, scope, Some(key))?;
        let path = self.file(store, scope, key);
        let parent = path.parent().expect("a save file has a parent");
        fs::create_dir_all(parent)?;
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("save.json");
        let tmp = parent.join(format!(
            ".{name}.{}.{}.tmp",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        {
            use io::Write;
            let mut f = fs::File::create(&tmp)?;
            f.write_all(json.as_bytes())?;
            f.sync_all()?;
        }
        // Rename replaces an existing target on Unix and Windows.
        fs::rename(&tmp, &path).inspect_err(|_| {
            let _ = fs::remove_file(&tmp);
        })
    }

    /// Remove one record. A missing record is not an error.
    pub fn remove(&self, store: &str, scope: &str, key: &str) -> io::Result<()> {
        checked(store, scope, Some(key))?;
        match fs::remove_file(self.file(store, scope, key)) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e),
        }
    }

    /// Every key stored for this store/scope, decoded from the file names.
    pub fn keys(&self, store: &str, scope: &str) -> io::Result<Vec<String>> {
        checked(store, scope, None)?;
        let dir = self.dir(store, scope);
        let entries = match fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(e),
        };
        let mut keys = Vec::new();
        for entry in entries {
            let path = entry?.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else { continue };
            if let Some(key) = unhex(stem) {
                keys.push(key);
            }
        }
        keys.sort();
        Ok(keys)
    }
}

/// DataStore names, scopes and keys must be non-empty; an empty key would
/// otherwise produce a file name that `keys` cannot list back.
fn checked(store: &str, scope: &str, key: Option<&str>) -> io::Result<()> {
    for (what, value) in [("store", Some(store)), ("scope", Some(scope)), ("key", key)] {
        if let Some(value) = value {
            if value.is_empty() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("empty DataStore {what}"),
                ));
            }
        }
    }
    Ok(())
}

fn hex(s: &str) -> String {
    use std::fmt::Write;
    let mut out = String::with_capacity(s.len() * 2);
    for b in s.bytes() {
        let _ = write!(out, "{b:02x}");
    }
    out
}

fn unhex(s: &str) -> Option<String> {
    if s.len() % 2 != 0 {
        return None;
    }
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(s.len() / 2);
    for pair in bytes.chunks_exact(2) {
        let hi = (pair[0] as char).to_digit(16)?;
        let lo = (pair[1] as char).to_digit(16)?;
        out.push((hi * 16 + lo) as u8);
    }
    String::from_utf8(out).ok()
}

/// Keep the on-disk name of a root out of script-visible errors.
pub fn io_error(e: io::Error) -> mlua::Error {
    mlua::Error::RuntimeError(format!("DataStore file error: {e}"))
}

/// Remove a test's save tree. Best effort; tests should not fail on cleanup.
#[cfg(test)]
pub fn remove_tree(root: &std::path::Path) {
    let _ = fs::remove_dir_all(root);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("rustudio-datastores-{name}-{}", std::process::id()))
    }

    #[test]
    fn round_trips_records_and_lists_keys() {
        let root = temp_root("roundtrip");
        let _ = fs::remove_dir_all(&root);
        let saves = Files::new(root.clone());
        assert_eq!(saves.read("Store", "scope", "key").unwrap(), None);
        saves.write("Store", "scope", "key", r#"{"schema":1,"value":2}"#).unwrap();
        assert_eq!(
            saves.read("Store", "scope", "key").unwrap().as_deref(),
            Some(r#"{"schema":1,"value":2}"#)
        );
        // Case-sensitive keys stay distinct on a case-insensitive filesystem.
        saves.write("Store", "scope", "Key", r#"{"schema":1,"value":3}"#).unwrap();
        assert_eq!(saves.keys("Store", "scope").unwrap(), ["Key", "key"]);
        saves.remove("Store", "scope", "key").unwrap();
        assert_eq!(saves.keys("Store", "scope").unwrap(), ["Key"]);
        saves.remove("Store", "scope", "key").unwrap(); // absent: no error
        remove_tree(&root);
    }

    #[test]
    fn names_with_path_characters_stay_inside_the_root() {
        let root = temp_root("escape");
        let _ = fs::remove_dir_all(&root);
        let saves = Files::new(root.clone());
        saves.write("../store", "a/../b", "x/../../y", r#"{"schema":1}"#).unwrap();
        assert_eq!(saves.read("../store", "a/../b", "x/../../y").unwrap().as_deref(), Some(r#"{"schema":1}"#));
        // Nothing escaped: every entry is under the root.
        for entry in walkdir(&root) {
            assert!(entry.starts_with(&root), "{} escaped {}", entry.display(), root.display());
        }
        remove_tree(&root);
    }

    fn walkdir(root: &std::path::Path) -> Vec<PathBuf> {
        let mut out = Vec::new();
        if let Ok(entries) = fs::read_dir(root) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    out.extend(walkdir(&path));
                } else {
                    out.push(path);
                }
            }
        }
        out
    }
}
