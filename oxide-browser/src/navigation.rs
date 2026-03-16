//! Navigation history stack for the Oxide browser.
//!
//! Manages a linear history of visited URLs with associated state, allowing
//! users (and guest binaries) to move forward and backward between views.
//! Pushing a new entry while in the middle of the stack truncates forward
//! history, matching standard browser semantics.

use std::time::{SystemTime, UNIX_EPOCH};

/// A single entry in the navigation history.
#[derive(Clone, Debug)]
pub struct HistoryEntry {
    /// The fully-resolved URL for this history point.
    pub url: String,
    /// An optional human-readable title.
    pub title: String,
    /// Opaque binary state attached by the guest via `push_state` /
    /// `replace_state`.  The guest can read this back on re-entry.
    pub state: Vec<u8>,
    /// Milliseconds since the UNIX epoch when this entry was created.
    #[allow(dead_code)]
    pub timestamp_ms: u64,
}

impl HistoryEntry {
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            title: String::new(),
            state: Vec::new(),
            timestamp_ms: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
        }
    }

    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }

    pub fn with_state(mut self, state: Vec<u8>) -> Self {
        self.state = state;
        self
    }
}

/// A linear stack of [`HistoryEntry`] items with a movable cursor.
#[derive(Clone, Debug)]
pub struct NavigationStack {
    entries: Vec<HistoryEntry>,
    /// Points to the current entry.  `-1` when the stack is empty.
    index: isize,
}

impl NavigationStack {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            index: -1,
        }
    }

    /// Push a new entry, discarding any forward history beyond the cursor.
    pub fn push(&mut self, entry: HistoryEntry) {
        let new_index = self.index + 1;
        self.entries.truncate(new_index as usize);
        self.entries.push(entry);
        self.index = new_index;
    }

    /// Replace the current entry in-place (no forward-history truncation).
    pub fn replace_current(&mut self, entry: HistoryEntry) {
        if self.index >= 0 && (self.index as usize) < self.entries.len() {
            self.entries[self.index as usize] = entry;
        } else {
            self.push(entry);
        }
    }

    /// Mutate selected fields of the current entry (used by guest
    /// `push_state` / `replace_state` when only updating metadata).
    #[allow(dead_code)]
    pub fn update_current(
        &mut self,
        title: Option<&str>,
        state: Option<Vec<u8>>,
        url: Option<&str>,
    ) {
        if let Some(entry) = self.current_mut() {
            if let Some(t) = title {
                entry.title = t.to_string();
            }
            if let Some(s) = state {
                entry.state = s;
            }
            if let Some(u) = url {
                entry.url = u.to_string();
            }
        }
    }

    /// Move the cursor backward.  Returns the new current entry.
    pub fn go_back(&mut self) -> Option<&HistoryEntry> {
        if self.index > 0 {
            self.index -= 1;
            Some(&self.entries[self.index as usize])
        } else {
            None
        }
    }

    /// Move the cursor forward.  Returns the new current entry.
    pub fn go_forward(&mut self) -> Option<&HistoryEntry> {
        if self.index + 1 < self.entries.len() as isize {
            self.index += 1;
            Some(&self.entries[self.index as usize])
        } else {
            None
        }
    }

    pub fn can_go_back(&self) -> bool {
        self.index > 0
    }

    pub fn can_go_forward(&self) -> bool {
        self.index + 1 < self.entries.len() as isize
    }

    pub fn current(&self) -> Option<&HistoryEntry> {
        if self.index >= 0 && (self.index as usize) < self.entries.len() {
            Some(&self.entries[self.index as usize])
        } else {
            None
        }
    }

    #[allow(dead_code)]
    pub fn current_mut(&mut self) -> Option<&mut HistoryEntry> {
        if self.index >= 0 && (self.index as usize) < self.entries.len() {
            Some(&mut self.entries[self.index as usize])
        } else {
            None
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    #[allow(dead_code)]
    pub fn current_index(&self) -> isize {
        self.index
    }

    #[allow(dead_code)]
    pub fn entries(&self) -> &[HistoryEntry] {
        &self.entries
    }
}

impl Default for NavigationStack {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_and_navigate() {
        let mut stack = NavigationStack::new();
        assert!(!stack.can_go_back());
        assert!(!stack.can_go_forward());

        stack.push(HistoryEntry::new("https://a.com"));
        stack.push(HistoryEntry::new("https://b.com"));
        stack.push(HistoryEntry::new("https://c.com"));

        assert_eq!(stack.current().unwrap().url, "https://c.com");
        assert!(stack.can_go_back());
        assert!(!stack.can_go_forward());

        let entry = stack.go_back().unwrap();
        assert_eq!(entry.url, "https://b.com");
        let entry = stack.go_back().unwrap();
        assert_eq!(entry.url, "https://a.com");
        assert!(!stack.can_go_back());
        assert!(stack.can_go_forward());

        let entry = stack.go_forward().unwrap();
        assert_eq!(entry.url, "https://b.com");

        stack.push(HistoryEntry::new("https://d.com"));
        assert!(!stack.can_go_forward());
        assert_eq!(stack.len(), 3); // a, b, d
    }

    #[test]
    fn replace_current() {
        let mut stack = NavigationStack::new();
        stack.push(HistoryEntry::new("https://a.com"));
        stack.replace_current(HistoryEntry::new("https://b.com"));
        assert_eq!(stack.current().unwrap().url, "https://b.com");
        assert_eq!(stack.len(), 1);
    }

    #[test]
    fn update_current_fields() {
        let mut stack = NavigationStack::new();
        stack.push(HistoryEntry::new("https://a.com"));
        stack.update_current(Some("Page A"), Some(vec![1, 2, 3]), None);
        let cur = stack.current().unwrap();
        assert_eq!(cur.title, "Page A");
        assert_eq!(cur.state, vec![1, 2, 3]);
        assert_eq!(cur.url, "https://a.com");
    }

    #[test]
    fn state_preserved_through_back_forward() {
        let mut stack = NavigationStack::new();
        stack.push(HistoryEntry::new("https://a.com").with_state(vec![10, 20]));
        stack.push(HistoryEntry::new("https://b.com"));

        let entry = stack.go_back().unwrap();
        assert_eq!(entry.state, vec![10, 20]);
    }
}
