//! Generic TTL-based JSON cache backed by a single file.
//!
//! # Usage
//!
//! ```rust,ignore
//! let store = CacheStore::<FeedSections>::new(
//!     config_dir.join("feed_cache.json"),
//!     Duration::from_secs(30 * 60),
//!     1, // schema_version — bump when the payload type changes
//! );
//!
//! match store.load()? {
//!     Some(data) => use_cached(data),
//!     None => {
//!         let fresh = fetch_from_network().await?;
//!         store.save(&fresh)?;
//!     }
//! }
//! ```
//!
//! ## Schema versioning
//!
//! When the payload type `T` gains or removes fields in a breaking way, bump
//! `schema_version`. Old cache files will be treated as a miss on next load
//! and silently replaced — no migration code required.

use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use anyhow::Result;
use serde::{Serialize, de::DeserializeOwned};

use super::persistence::{write_atomic, MAX_FILE_SIZE};

// ---------------------------------------------------------------------------
// On-disk envelope
// ---------------------------------------------------------------------------

#[allow(dead_code)]
#[derive(serde::Serialize, serde::Deserialize)]
struct CachedEnvelope<T> {
    /// Unix timestamp (seconds since UNIX_EPOCH) when the cache was written.
    saved_at: u64,
    /// Monotonically increasing version tied to the shape of `T`.
    /// A mismatch between the stored version and the requested version
    /// causes the cache to be treated as a miss.
    schema_version: u32,
    /// The actual cached payload.
    payload: T,
}

// ---------------------------------------------------------------------------
// CacheStore
// ---------------------------------------------------------------------------

/// A file-backed, TTL-aware cache for any `serde`-serializable type.
#[allow(dead_code)]
pub(crate) struct CacheStore<T> {
    path: PathBuf,
    ttl: Duration,
    schema_version: u32,
    _marker: std::marker::PhantomData<T>,
}

#[allow(dead_code)]
impl<T: Serialize + DeserializeOwned> CacheStore<T> {
    /// Create a new `CacheStore`.
    ///
    /// - `path` — absolute path to the JSON cache file.
    /// - `ttl` — how long a cached entry is considered fresh.
    /// - `schema_version` — bump this whenever `T`'s serialized shape changes.
    pub(crate) fn new(path: PathBuf, ttl: Duration, schema_version: u32) -> Self {
        Self {
            path,
            ttl,
            schema_version,
            _marker: std::marker::PhantomData,
        }
    }

    /// Load the cached payload if it exists, is not expired, and matches the
    /// current schema version.
    ///
    /// Returns:
    /// - `Ok(Some(T))` — cache hit (fresh, schema-compatible).
    /// - `Ok(None)` — cache miss (missing file, expired, schema mismatch, or
    ///   corrupt JSON). The caller should fetch fresh data and call [`save`].
    /// - `Err(_)` — I/O error other than "file not found".
    pub(crate) fn load(&self) -> Result<Option<T>> {
        let bytes = match std::fs::read(&self.path) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e.into()),
        };

        if bytes.len() as u64 > MAX_FILE_SIZE {
            // Oversized — treat as miss, not an error.
            return Ok(None);
        }

        let env: CachedEnvelope<T> = match serde_json::from_slice(&bytes) {
            Ok(e) => e,
            // Corrupt JSON → treat as miss so the caller can overwrite it.
            Err(_) => return Ok(None),
        };

        if env.schema_version != self.schema_version {
            return Ok(None);
        }

        let age = SystemTime::UNIX_EPOCH
            .checked_add(Duration::from_secs(env.saved_at))
            .and_then(|saved| SystemTime::now().duration_since(saved).ok())
            .unwrap_or(Duration::MAX);

        if age > self.ttl {
            return Ok(None);
        }

        Ok(Some(env.payload))
    }

    /// Persist `payload` to disk, overwriting any existing cache file.
    pub(crate) fn save(&self, payload: &T) -> Result<()> {
        let saved_at = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let env = CachedEnvelope {
            saved_at,
            schema_version: self.schema_version,
            payload,
        };

        let json = serde_json::to_vec(&env).map_err(|e| anyhow::anyhow!("Cache serialize error: {e}"))?;
        write_atomic(&self.path, &json)
    }

    /// Delete the cache file, forcing the next [`load`] to return `None`.
    pub(crate) fn invalidate(&self) {
        let _ = std::fs::remove_file(&self.path);
    }

    /// Returns the path this store writes to (useful for logging/debugging).
    #[allow(dead_code)]
    pub(crate) fn path(&self) -> &std::path::Path {
        &self.path
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[derive(Debug, PartialEq, serde::Serialize, serde::Deserialize)]
    struct Payload {
        value: String,
        count: u32,
    }

    fn store_in(dir: &std::path::Path, ttl_secs: u64, schema_version: u32) -> CacheStore<Payload> {
        CacheStore::new(
            dir.join("cache.json"),
            Duration::from_secs(ttl_secs),
            schema_version,
        )
    }

    // -- Round-trip --

    #[test]
    fn save_and_load_round_trip() {
        let tmp = TempDir::new().unwrap();
        let store = store_in(tmp.path(), 3600, 1);

        let payload = Payload {
            value: "hello".into(),
            count: 42,
        };

        store.save(&payload).unwrap();
        let loaded = store.load().unwrap();

        assert_eq!(loaded, Some(payload));
    }

    // -- Miss cases --

    #[test]
    fn load_returns_none_when_no_file() {
        let tmp = TempDir::new().unwrap();
        let store = store_in(tmp.path(), 3600, 1);

        assert_eq!(store.load().unwrap(), None);
    }

    #[test]
    fn load_returns_none_when_expired() {
        let tmp = TempDir::new().unwrap();
        // TTL of 0 seconds — always expired
        let store = store_in(tmp.path(), 0, 1);

        let payload = Payload {
            value: "stale".into(),
            count: 1,
        };
        store.save(&payload).unwrap();

        // Even immediately after saving, TTL=0 means expired
        assert_eq!(store.load().unwrap(), None);
    }

    #[test]
    fn load_returns_none_on_schema_version_mismatch() {
        let tmp = TempDir::new().unwrap();

        // Save with schema version 1
        let store_v1 = store_in(tmp.path(), 3600, 1);
        store_v1
            .save(&Payload {
                value: "old".into(),
                count: 1,
            })
            .unwrap();

        // Load with schema version 2 — should be a miss
        let store_v2 = store_in(tmp.path(), 3600, 2);
        assert_eq!(store_v2.load().unwrap(), None);
    }

    #[test]
    fn load_returns_none_on_corrupt_json() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("cache.json");

        fs::write(&path, b"not valid json {{{{").unwrap();

        let store = store_in(tmp.path(), 3600, 1);
        assert_eq!(store.load().unwrap(), None);
    }

    #[test]
    fn load_returns_none_on_oversized_file() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("cache.json");

        // Write a file larger than MAX_FILE_SIZE (10 MB)
        let big = vec![b'x'; (MAX_FILE_SIZE + 1) as usize];
        fs::write(&path, &big).unwrap();

        let store = store_in(tmp.path(), 3600, 1);
        assert_eq!(store.load().unwrap(), None);
    }

    // -- Invalidate --

    #[test]
    fn invalidate_removes_cache_file() {
        let tmp = TempDir::new().unwrap();
        let store = store_in(tmp.path(), 3600, 1);

        store
            .save(&Payload {
                value: "x".into(),
                count: 0,
            })
            .unwrap();
        assert!(store.path().exists());

        store.invalidate();
        assert!(!store.path().exists());
        assert_eq!(store.load().unwrap(), None);
    }

    // -- Overwrite --

    #[test]
    fn save_overwrites_existing_cache() {
        let tmp = TempDir::new().unwrap();
        let store = store_in(tmp.path(), 3600, 1);

        store
            .save(&Payload {
                value: "first".into(),
                count: 1,
            })
            .unwrap();
        store
            .save(&Payload {
                value: "second".into(),
                count: 2,
            })
            .unwrap();

        let loaded = store.load().unwrap().unwrap();
        assert_eq!(loaded.value, "second");
        assert_eq!(loaded.count, 2);
    }

    // -- Creates parent dirs --

    #[test]
    fn save_creates_parent_directories() {
        let tmp = TempDir::new().unwrap();
        let store: CacheStore<Payload> = CacheStore::new(
            tmp.path().join("nested").join("dir").join("cache.json"),
            Duration::from_secs(3600),
            1,
        );

        store
            .save(&Payload {
                value: "deep".into(),
                count: 99,
            })
            .unwrap();

        assert!(store.path().exists());
    }
}
