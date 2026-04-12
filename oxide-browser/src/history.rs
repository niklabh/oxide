//! Persistent browsing history for the Oxide browser.
//!
//! Entries are stored in a [`sled`] embedded database under a [`sled::Tree`]
//! named `"history"`. Each record is keyed by a unique timestamp+URL composite
//! key so the same URL visited multiple times produces separate entries.
//! For thread-safe access use [`SharedHistoryStore`].

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};

static HISTORY_SEQ: AtomicU64 = AtomicU64::new(0);

/// A single browsing history entry.
#[derive(Clone, Debug)]
pub struct HistoryItem {
    pub url: String,
    pub title: String,
    pub visited_at_ms: u64,
}

impl HistoryItem {
    fn to_bytes(&self) -> Vec<u8> {
        let ts_bytes = self.visited_at_ms.to_le_bytes();
        let title_bytes = self.title.as_bytes();
        let title_len = (title_bytes.len() as u32).to_le_bytes();
        let url_bytes = self.url.as_bytes();
        let url_len = (url_bytes.len() as u32).to_le_bytes();

        let mut buf = Vec::with_capacity(8 + 4 + title_bytes.len() + 4 + url_bytes.len());
        buf.extend_from_slice(&ts_bytes);
        buf.extend_from_slice(&title_len);
        buf.extend_from_slice(title_bytes);
        buf.extend_from_slice(&url_len);
        buf.extend_from_slice(url_bytes);
        buf
    }

    fn from_bytes(data: &[u8]) -> Option<Self> {
        if data.len() < 16 {
            return None;
        }
        let visited_at_ms = u64::from_le_bytes(data[0..8].try_into().ok()?);
        let title_len = u32::from_le_bytes(data[8..12].try_into().ok()?) as usize;
        if data.len() < 12 + title_len + 4 {
            return None;
        }
        let title = String::from_utf8(data[12..12 + title_len].to_vec()).ok()?;
        let url_off = 12 + title_len;
        let url_len = u32::from_le_bytes(data[url_off..url_off + 4].try_into().ok()?) as usize;
        if data.len() < url_off + 4 + url_len {
            return None;
        }
        let url = String::from_utf8(data[url_off + 4..url_off + 4 + url_len].to_vec()).ok()?;
        Some(Self {
            url,
            title,
            visited_at_ms,
        })
    }
}

/// Persistent history storage backed by a [`sled::Tree`].
#[derive(Clone)]
pub struct HistoryStore {
    tree: sled::Tree,
}

impl HistoryStore {
    pub fn open(db: &sled::Db) -> Result<Self> {
        let tree = db
            .open_tree("history")
            .context("failed to open history tree")?;
        Ok(Self { tree })
    }

    /// Record a visited page. Uses timestamp + sequence number as key for uniqueness.
    pub fn record(&self, url: &str, title: &str) -> Result<()> {
        let ts = now_ms();
        let seq = HISTORY_SEQ.fetch_add(1, Ordering::Relaxed);
        let item = HistoryItem {
            url: url.to_string(),
            title: title.to_string(),
            visited_at_ms: ts,
        };
        let mut key = Vec::with_capacity(16);
        key.extend_from_slice(&ts.to_be_bytes());
        key.extend_from_slice(&seq.to_be_bytes());
        self.tree
            .insert(key, item.to_bytes())
            .context("failed to insert history entry")?;
        Ok(())
    }

    /// Returns all history entries, newest first. Each entry includes its
    /// sled key for targeted deletion via [`Self::remove_by_key`].
    pub fn list_all(&self) -> Vec<(Vec<u8>, HistoryItem)> {
        let mut items = Vec::new();
        for entry in self.tree.iter().flatten() {
            let (key, val) = entry;
            if let Some(item) = HistoryItem::from_bytes(&val) {
                items.push((key.to_vec(), item));
            }
        }
        items.reverse();
        items
    }

    /// Remove a single history entry by its sled key.
    pub fn remove_by_key(&self, key: &[u8]) -> Result<()> {
        self.tree
            .remove(key)
            .context("failed to remove history entry")?;
        Ok(())
    }

    /// Delete every entry in the history.
    pub fn clear(&self) -> Result<()> {
        self.tree.clear().context("failed to clear history")?;
        Ok(())
    }
}

pub type SharedHistoryStore = Arc<Mutex<Option<HistoryStore>>>;

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn test_store() -> HistoryStore {
        let dir = tempdir().unwrap();
        let db = sled::open(dir.path()).unwrap();
        HistoryStore::open(&db).unwrap()
    }

    #[test]
    fn record_and_list() {
        let store = test_store();
        store.record("https://a.com/app.wasm", "App A").unwrap();
        store.record("https://b.com/app.wasm", "App B").unwrap();
        let all = store.list_all();
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].1.url, "https://b.com/app.wasm");
        assert_eq!(all[1].1.url, "https://a.com/app.wasm");
    }

    #[test]
    fn duplicate_urls_create_separate_entries() {
        let store = test_store();
        store.record("https://a.com/app.wasm", "A").unwrap();
        store.record("https://a.com/app.wasm", "A").unwrap();
        let all = store.list_all();
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn remove_single_entry() {
        let store = test_store();
        store.record("https://a.com/app.wasm", "A").unwrap();
        store.record("https://b.com/app.wasm", "B").unwrap();
        let all = store.list_all();
        assert_eq!(all.len(), 2);
        store.remove_by_key(&all[0].0).unwrap();
        let remaining = store.list_all();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].1.url, "https://a.com/app.wasm");
    }

    #[test]
    fn clear_all() {
        let store = test_store();
        store.record("https://a.com/app.wasm", "A").unwrap();
        store.record("https://b.com/app.wasm", "B").unwrap();
        store.clear().unwrap();
        assert_eq!(store.list_all().len(), 0);
    }
}
