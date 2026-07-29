//! Web Storage behind the terminal runtime: a synchronous backend seam the
//! `__uic_storage_*` natives reach the way the DOM natives reach
//! [`HostState`](crate::HostState) — through a thread-local, because the
//! flat natives capture nothing. One backend per thread: a second host on
//! the same thread replaces the slot, the same sharing the document state
//! has.

use std::cell::RefCell;
use std::collections::BTreeMap;

use boa_engine::{JsNativeError, JsResult};

/// A write the backend refused — thrown into the runtime, the browser's
/// quota behavior.
#[derive(Debug)]
pub struct StorageError(pub String);

/// Web Storage semantics, synchronous: string keys and values, last write
/// wins, `get` of a missing key is `None`, and `key(n)` enumerates in
/// sorted order so iteration stays deterministic between mutations.
pub trait StorageBackend {
    fn get(&self, key: &str) -> Option<String>;
    fn set(&mut self, key: &str, value: &str) -> Result<(), StorageError>;
    fn remove(&mut self, key: &str);
    fn clear(&mut self);
    fn key(&self, index: usize) -> Option<String>;
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// The default backend: storage for the process lifetime, sorted in memory.
#[derive(Default)]
pub struct MemoryBackend(BTreeMap<String, String>);

impl StorageBackend for MemoryBackend {
    fn get(&self, key: &str) -> Option<String> {
        self.0.get(key).cloned()
    }

    fn set(&mut self, key: &str, value: &str) -> Result<(), StorageError> {
        self.0.insert(key.to_string(), value.to_string());
        Ok(())
    }

    fn remove(&mut self, key: &str) {
        self.0.remove(key);
    }

    fn clear(&mut self) {
        self.0.clear();
    }

    fn key(&self, index: usize) -> Option<String> {
        self.0.keys().nth(index).cloned()
    }

    fn len(&self) -> usize {
        self.0.len()
    }
}

/// The file-backed backend: one `kv` table in an SQLite database, so a
/// location the app selects survives restarts. `ORDER BY key` mirrors the
/// memory backend's sorted enumeration.
#[cfg(feature = "sqlite")]
pub struct SqliteBackend {
    conn: rusqlite::Connection,
}

#[cfg(feature = "sqlite")]
impl SqliteBackend {
    /// Opens the database, creating the file and the `kv` table as needed.
    pub fn open(path: impl AsRef<std::path::Path>) -> Result<Self, crate::Error> {
        let conn = rusqlite::Connection::open(path).map_err(sqlite_error)?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS kv (key TEXT PRIMARY KEY, value TEXT NOT NULL)",
            [],
        )
        .map_err(sqlite_error)?;
        Ok(SqliteBackend { conn })
    }
}

#[cfg(feature = "sqlite")]
fn sqlite_error(err: rusqlite::Error) -> crate::Error {
    crate::Error::Storage(err.to_string())
}

#[cfg(feature = "sqlite")]
impl StorageBackend for SqliteBackend {
    fn get(&self, key: &str) -> Option<String> {
        self.conn
            .query_row("SELECT value FROM kv WHERE key = ?1", [key], |row| {
                row.get(0)
            })
            .ok()
    }

    fn set(&mut self, key: &str, value: &str) -> Result<(), StorageError> {
        self.conn
            .execute(
                "INSERT INTO kv (key, value) VALUES (?1, ?2) \
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                [key, value],
            )
            .map(|_| ())
            .map_err(|err| StorageError(err.to_string()))
    }

    fn remove(&mut self, key: &str) {
        // A failed delete leaves the row for the next write to overwrite;
        // reads treat the database's word as final either way.
        let _ = self.conn.execute("DELETE FROM kv WHERE key = ?1", [key]);
    }

    fn clear(&mut self) {
        let _ = self.conn.execute("DELETE FROM kv", []);
    }

    fn key(&self, index: usize) -> Option<String> {
        self.conn
            .query_row(
                "SELECT key FROM kv ORDER BY key LIMIT 1 OFFSET ?1",
                [index as i64],
                |row| row.get(0),
            )
            .ok()
    }

    fn len(&self) -> usize {
        self.conn
            .query_row("SELECT COUNT(*) FROM kv", [], |row| row.get::<_, i64>(0))
            .map(|count| count as usize)
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // One script for every backend: Web Storage semantics are the trait's
    // contract, not an implementation detail.
    fn speaks_web_storage(backend: &mut dyn StorageBackend) {
        backend.set("banana", "2").unwrap();
        backend.set("apple", "1").unwrap();
        backend.set("apple", "first").unwrap();
        assert_eq!(backend.get("apple").as_deref(), Some("first"));
        assert_eq!(backend.get("absent"), None);
        assert_eq!(backend.len(), 2);
        assert_eq!(backend.key(0).as_deref(), Some("apple"));
        assert_eq!(backend.key(1).as_deref(), Some("banana"));
        assert_eq!(backend.key(2), None);
        backend.remove("apple");
        assert_eq!(backend.len(), 1);
        backend.clear();
        assert!(backend.is_empty());
    }

    #[test]
    fn the_memory_backend_speaks_web_storage() {
        speaks_web_storage(&mut MemoryBackend::default());
    }

    #[cfg(feature = "sqlite")]
    #[test]
    fn the_sqlite_backend_speaks_web_storage_and_reopens() {
        let path = std::env::temp_dir().join(format!("uic-js-storage-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        speaks_web_storage(&mut SqliteBackend::open(&path).unwrap());

        let mut backend = SqliteBackend::open(&path).unwrap();
        backend.set("kept", "yes").unwrap();
        drop(backend);
        let reopened = SqliteBackend::open(&path).unwrap();
        assert_eq!(reopened.get("kept").as_deref(), Some("yes"));
        let _ = std::fs::remove_file(&path);
    }
}

thread_local! {
    static BACKEND: RefCell<Option<Box<dyn StorageBackend>>> = const { RefCell::new(None) };
}

pub(crate) fn install(backend: Box<dyn StorageBackend>) {
    BACKEND.with(|slot| *slot.borrow_mut() = Some(backend));
}

pub(crate) fn with_backend<R>(f: impl FnOnce(&mut dyn StorageBackend) -> R) -> JsResult<R> {
    BACKEND.with(|slot| {
        let mut slot = slot.borrow_mut();
        let backend = slot.as_deref_mut().ok_or_else(|| {
            JsNativeError::error().with_message("uic_js storage backend is not installed")
        })?;
        Ok(f(backend))
    })
}
