//! Persistent bookmarks for the Oxide browser.
//!
//! Entries are stored in a [`sled`] embedded database under a dedicated [`sled::Tree`]
//! named `"bookmarks"`. Each record is keyed by URL; values hold the serialized
//! title, favorite flag, and creation time. For UI code that may run on multiple
//! threads, use [`SharedBookmarkStore`] and initialize the store when the
//! database is available.

use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};

/// A saved bookmark: canonical URL, display title, favorite flag, and creation time.
///
/// The URL is the primary key in [`BookmarkStore`]. New bookmarks from [`BookmarkStore::add`]
/// start with [`Bookmark::is_favorite`] set to `false` and [`Bookmark::created_at_ms`] set
/// to the current time in milliseconds since the UNIX epoch.
#[derive(Clone, Debug)]
pub struct Bookmark {
    /// Canonical bookmark URL; also the sled key for this entry.
    pub url: String,
    /// User-visible title (may differ from the page title at save time).
    pub title: String,
    /// When `true`, this bookmark is included in favorite-only listings.
    pub is_favorite: bool,
    /// Creation instant as milliseconds since [`UNIX_EPOCH`].
    pub created_at_ms: u64,
}

impl Bookmark {
    fn to_bytes(&self) -> Vec<u8> {
        let fav_byte: u8 = if self.is_favorite { 1 } else { 0 };
        let ts_bytes = self.created_at_ms.to_le_bytes();
        let title_bytes = self.title.as_bytes();
        let title_len = (title_bytes.len() as u32).to_le_bytes();

        let mut buf = Vec::with_capacity(1 + 8 + 4 + title_bytes.len());
        buf.push(fav_byte);
        buf.extend_from_slice(&ts_bytes);
        buf.extend_from_slice(&title_len);
        buf.extend_from_slice(title_bytes);
        buf
    }

    fn from_bytes(url: &str, data: &[u8]) -> Option<Self> {
        if data.len() < 13 {
            return None;
        }
        let is_favorite = data[0] != 0;
        let created_at_ms = u64::from_le_bytes(data[1..9].try_into().ok()?);
        let title_len = u32::from_le_bytes(data[9..13].try_into().ok()?) as usize;
        if data.len() < 13 + title_len {
            return None;
        }
        let title = String::from_utf8(data[13..13 + title_len].to_vec()).ok()?;
        Some(Self {
            url: url.to_string(),
            title,
            is_favorite,
            created_at_ms,
        })
    }
}

/// Persistent bookmark storage backed by a [`sled::Tree`] in an open [`sled::Db`].
///
/// The tree name is `"bookmarks"`. Keys are URL byte strings; values are an internal
/// binary encoding of title, favorite bit, and timestamp (see [`Bookmark`]).
#[derive(Clone)]
pub struct BookmarkStore {
    tree: sled::Tree,
}

impl BookmarkStore {
    /// Opens the bookmarks tree in `db`, creating it if it does not exist.
    pub fn open(db: &sled::Db) -> Result<Self> {
        let tree = db
            .open_tree("bookmarks")
            .context("failed to open bookmarks tree")?;
        Ok(Self { tree })
    }

    /// Inserts a new bookmark for `url` with the given `title`, or overwrites the existing entry.
    ///
    /// The bookmark is stored as not favorited with a fresh [`Bookmark::created_at_ms`].
    pub fn add(&self, url: &str, title: &str) -> Result<()> {
        let bm = Bookmark {
            url: url.to_string(),
            title: title.to_string(),
            is_favorite: false,
            created_at_ms: now_ms(),
        };
        self.tree
            .insert(url.as_bytes(), bm.to_bytes())
            .context("failed to insert bookmark")?;
        Ok(())
    }

    /// Removes the bookmark for `url`, if present.
    pub fn remove(&self, url: &str) -> Result<()> {
        self.tree
            .remove(url.as_bytes())
            .context("failed to remove bookmark")?;
        Ok(())
    }

    /// Returns whether a bookmark exists for `url`.
    pub fn contains(&self, url: &str) -> bool {
        self.tree.contains_key(url.as_bytes()).unwrap_or(false)
    }

    /// Flips the favorite flag for the bookmark at `url` and returns the new value.
    ///
    /// If the URL is missing or the stored value cannot be decoded, returns `Ok(false)` without
    /// changing storage.
    pub fn toggle_favorite(&self, url: &str) -> Result<bool> {
        if let Some(data) = self
            .tree
            .get(url.as_bytes())
            .context("failed to read bookmark")?
        {
            if let Some(mut bm) = Bookmark::from_bytes(url, &data) {
                bm.is_favorite = !bm.is_favorite;
                let new_fav = bm.is_favorite;
                self.tree
                    .insert(url.as_bytes(), bm.to_bytes())
                    .context("failed to update bookmark")?;
                return Ok(new_fav);
            }
        }
        Ok(false)
    }

    /// Returns whether the bookmark at `url` is marked as a favorite.
    ///
    /// Missing or corrupt entries are treated as not favorited.
    #[allow(dead_code)]
    pub fn is_favorite(&self, url: &str) -> bool {
        self.tree
            .get(url.as_bytes())
            .ok()
            .flatten()
            .and_then(|data| Bookmark::from_bytes(url, &data))
            .map(|bm| bm.is_favorite)
            .unwrap_or(false)
    }

    /// Returns every bookmark, ordered by [`Bookmark::created_at_ms`] descending (newest first).
    pub fn list_all(&self) -> Vec<Bookmark> {
        let mut bookmarks = Vec::new();
        for (key, val) in self.tree.iter().flatten() {
            if let Ok(url) = String::from_utf8(key.to_vec()) {
                if let Some(bm) = Bookmark::from_bytes(&url, &val) {
                    bookmarks.push(bm);
                }
            }
        }
        bookmarks.sort_by(|a, b| b.created_at_ms.cmp(&a.created_at_ms));
        bookmarks
    }

    /// Returns only bookmarks with [`Bookmark::is_favorite`] set, in the same order as [`Self::list_all`].
    #[allow(dead_code)]
    pub fn list_favorites(&self) -> Vec<Bookmark> {
        self.list_all()
            .into_iter()
            .filter(|bm| bm.is_favorite)
            .collect()
    }
}

/// Thread-safe handle to an optional [`BookmarkStore`]: [`Arc`] wrapped [`Mutex`] of [`Option`].
///
/// Use `None` before the sled database is opened; replace with `Some(store)` after
/// [`BookmarkStore::open`]. Lock the mutex when reading or updating bookmarks from worker threads.
pub type SharedBookmarkStore = Arc<Mutex<Option<BookmarkStore>>>;

/// Creates a shared bookmark store initialized to `None` (no database opened yet).
pub fn new_shared() -> SharedBookmarkStore {
    Arc::new(Mutex::new(None))
}

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

    fn test_store() -> BookmarkStore {
        let dir = tempdir().unwrap();
        let db = sled::open(dir.path()).unwrap();
        BookmarkStore::open(&db).unwrap()
    }

    #[test]
    fn add_and_list() {
        let store = test_store();
        store.add("https://a.com/app.wasm", "App A").unwrap();
        store.add("https://b.com/app.wasm", "App B").unwrap();
        let all = store.list_all();
        assert_eq!(all.len(), 2);
        assert!(store.contains("https://a.com/app.wasm"));
        assert!(!store.contains("https://c.com/app.wasm"));
    }

    #[test]
    fn remove_bookmark() {
        let store = test_store();
        store.add("https://a.com/app.wasm", "A").unwrap();
        assert!(store.contains("https://a.com/app.wasm"));
        store.remove("https://a.com/app.wasm").unwrap();
        assert!(!store.contains("https://a.com/app.wasm"));
    }

    #[test]
    fn toggle_favorite() {
        let store = test_store();
        store.add("https://a.com/app.wasm", "A").unwrap();
        assert!(!store.is_favorite("https://a.com/app.wasm"));
        store.toggle_favorite("https://a.com/app.wasm").unwrap();
        assert!(store.is_favorite("https://a.com/app.wasm"));
        store.toggle_favorite("https://a.com/app.wasm").unwrap();
        assert!(!store.is_favorite("https://a.com/app.wasm"));
    }

    #[test]
    fn list_favorites_only() {
        let store = test_store();
        store.add("https://a.com/app.wasm", "A").unwrap();
        store.add("https://b.com/app.wasm", "B").unwrap();
        store.toggle_favorite("https://a.com/app.wasm").unwrap();
        let favs = store.list_favorites();
        assert_eq!(favs.len(), 1);
        assert_eq!(favs[0].url, "https://a.com/app.wasm");
    }
}
