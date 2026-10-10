//! The session model: which tabs are open, and where the caret sits in each.
//!
//! This is the data that makes BHNotepad's one essential promise work — quit
//! with ten dirty scratch tabs, get all ten back. See CLAUDE.md.

use serde::{Deserialize, Serialize};

use crate::language;

/// Bumped only when a change would make an older file unreadable. Readers
/// tolerate unknown future versions by falling back to an empty session
/// rather than misinterpreting fields.
pub const SESSION_VERSION: u32 = 1;

/// One open tab.
///
/// Every field after `id` carries `#[serde(default)]` so a session written by
/// an older build, or hand-edited, still loads. Losing a caret position is a
/// shrug; refusing to open is not.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Buffer {
    /// Stable, filename-safe. Also names this buffer's snapshot on disk, so it
    /// must never contain a path separator.
    pub id: String,
    /// What the tab shows. For a saved file this is its base name; for a
    /// scratch buffer it is Notepad++'s `new 1`, `new 2`, …
    #[serde(default)]
    pub title: String,
    /// Absolute path, or `None` for a scratch buffer that has never been saved.
    #[serde(default)]
    pub path: Option<String>,
    /// Language id from [`crate::language`].
    #[serde(default = "default_language")]
    pub language: String,
    /// Whether the snapshot differs from the file on disk. A scratch buffer
    /// with text in it is dirty and stays dirty — that is normal, not a
    /// problem to resolve at quit.
    #[serde(default)]
    pub dirty: bool,
    /// Caret offset in characters.
    #[serde(default)]
    pub caret: u64,
    /// First visible line, so the view is restored where it was left.
    #[serde(default)]
    pub scroll_top: u64,
    /// Per-tab word wrap, like Notepad++'s per-view setting.
    #[serde(default)]
    pub word_wrap: bool,
    /// The line ending this file arrived with: `"crlf"` or `"lf"`.
    ///
    /// The editor normalises everything to LF internally, so without this a
    /// Windows file would come back with every one of its line endings
    /// rewritten — a diff touching the whole file for a one-word change.
    #[serde(default = "default_eol")]
    pub eol: String,
    /// Set when the file could not be decoded as UTF-8 and characters were
    /// replaced. Saving over the original would destroy the bytes that were
    /// not understood, so it is refused; Save As is still allowed.
    #[serde(default)]
    pub lossy: bool,
}

/// Line endings default to LF: it is what a new scratch note should have on
/// every platform, and what the editor uses internally.
fn default_eol() -> String {
    "lf".to_string()
}

fn default_language() -> String {
    language::PLAIN.to_string()
}

impl Buffer {
    /// A new scratch buffer. `title` is normally from [`Session::next_untitled_title`].
    pub fn untitled(id: impl Into<String>, title: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            path: None,
            language: default_language(),
            dirty: false,
            caret: 0,
            scroll_top: 0,
            word_wrap: false,
            eol: default_eol(),
            lossy: false,
        }
    }

    /// A buffer backed by a file on disk, with its language detected from the path.
    pub fn for_path(id: impl Into<String>, path: impl Into<String>) -> Self {
        let path = path.into();
        Self {
            id: id.into(),
            title: language::base_name(&path).to_string(),
            language: language::detect(&path).to_string(),
            path: Some(path),
            dirty: false,
            caret: 0,
            scroll_top: 0,
            word_wrap: false,
            eol: default_eol(),
            lossy: false,
        }
    }

    /// True when this tab has never been written to a file.
    pub fn is_scratch(&self) -> bool {
        self.path.is_none()
    }
}

/// Every open tab, in tab-bar order, plus which one is in front.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Session {
    #[serde(default = "default_version")]
    pub version: u32,
    #[serde(default)]
    pub buffers: Vec<Buffer>,
    /// Index into `buffers`. Always valid after [`Session::normalize`].
    #[serde(default)]
    pub active: usize,
}

fn default_version() -> u32 {
    SESSION_VERSION
}

impl Default for Session {
    fn default() -> Self {
        Self {
            version: SESSION_VERSION,
            buffers: Vec::new(),
            active: 0,
        }
    }
}

impl Session {
    /// Repairs anything a hand-edit, a partial write or an older build could
    /// have left inconsistent. Called on every load, so the rest of the app
    /// may assume a valid session.
    ///
    /// Drops buffers with an empty or path-like `id` (the id names a file on
    /// disk, so a separator in it would escape the buffers directory), drops
    /// duplicate ids, fills in blank titles, and clamps `active` into range.
    pub fn normalize(&mut self) {
        let mut seen: Vec<String> = Vec::with_capacity(self.buffers.len());
        let active_id = self.buffers.get(self.active).map(|b| b.id.clone());

        self.buffers.retain(|b| {
            if !is_safe_id(&b.id) || seen.contains(&b.id) {
                return false;
            }
            seen.push(b.id.clone());
            true
        });

        let mut untitled_seen = 0usize;
        for buffer in &mut self.buffers {
            if buffer.title.trim().is_empty() {
                buffer.title = match &buffer.path {
                    Some(path) => language::base_name(path).to_string(),
                    None => {
                        untitled_seen += 1;
                        format!("new {untitled_seen}")
                    }
                };
            }
            if buffer.language.trim().is_empty() {
                buffer.language = default_language();
            }
        }

        // Keep whichever tab was in front rather than resetting to the first,
        // even if earlier tabs were dropped.
        self.active = active_id
            .and_then(|id| self.buffers.iter().position(|b| b.id == id))
            .unwrap_or(0);
        if self.active >= self.buffers.len() {
            self.active = self.buffers.len().saturating_sub(1);
        }
        self.version = SESSION_VERSION;
    }

    /// The next free `new N` title, matching Notepad++: the lowest number not
    /// currently in use, so closing `new 2` and opening a tab reuses `new 2`
    /// instead of climbing to `new 4`.
    pub fn next_untitled_title(&self) -> String {
        let used: Vec<u64> = self
            .buffers
            .iter()
            .filter_map(|b| b.title.strip_prefix("new ").and_then(|n| n.parse().ok()))
            .collect();
        let mut n = 1u64;
        while used.contains(&n) {
            n += 1;
        }
        format!("new {n}")
    }

    pub fn index_of(&self, id: &str) -> Option<usize> {
        self.buffers.iter().position(|b| b.id == id)
    }

    pub fn buffer(&self, id: &str) -> Option<&Buffer> {
        self.buffers.iter().find(|b| b.id == id)
    }

    pub fn buffer_mut(&mut self, id: &str) -> Option<&mut Buffer> {
        self.buffers.iter_mut().find(|b| b.id == id)
    }

    /// Returns the index of the tab already showing `path`, so opening a file
    /// twice focuses the existing tab instead of making a second copy.
    pub fn index_of_path(&self, path: &str) -> Option<usize> {
        self.buffers
            .iter()
            .position(|b| b.path.as_deref() == Some(path))
    }
}

/// An id becomes a file name inside the buffers directory, so it must not be
/// empty, must not contain a separator, and must not be a relative-path
/// component. Anything else would let a crafted session file write outside the
/// store.
pub fn is_safe_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id != "."
        && id != ".."
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(id: &str, title: &str) -> Buffer {
        Buffer::untitled(id, title)
    }

    #[test]
    fn for_path_sets_title_and_language() {
        let b = Buffer::for_path("a1", "/Users/benjamin/notes/todo.md");
        assert_eq!(b.title, "todo.md");
        assert_eq!(b.language, "markdown");
        assert!(!b.is_scratch());
    }

    #[test]
    fn untitled_numbering_reuses_the_lowest_free_number() {
        let mut s = Session::default();
        s.buffers.push(scratch("a", "new 1"));
        s.buffers.push(scratch("b", "new 3"));
        // 2 is free, so it must come next -- not 4.
        assert_eq!(s.next_untitled_title(), "new 2");

        s.buffers.push(scratch("c", "new 2"));
        assert_eq!(s.next_untitled_title(), "new 4");
    }

    #[test]
    fn untitled_numbering_ignores_similar_titles() {
        let mut s = Session::default();
        s.buffers.push(scratch("a", "new notes.txt"));
        s.buffers.push(scratch("b", "new "));
        assert_eq!(s.next_untitled_title(), "new 1");
    }

    #[test]
    fn normalize_rejects_ids_that_could_escape_the_store() {
        let mut s = Session::default();
        s.buffers.push(scratch("../../etc/passwd", "evil"));
        s.buffers.push(scratch("sub/dir", "evil2"));
        s.buffers.push(scratch("", "evil3"));
        s.buffers.push(scratch("..", "evil4"));
        s.buffers.push(scratch("ok_1", "fine"));
        s.normalize();
        assert_eq!(s.buffers.len(), 1);
        assert_eq!(s.buffers[0].id, "ok_1");
    }

    #[test]
    fn normalize_drops_duplicate_ids_keeping_the_first() {
        let mut s = Session::default();
        s.buffers.push(scratch("dup", "first"));
        s.buffers.push(scratch("dup", "second"));
        s.normalize();
        assert_eq!(s.buffers.len(), 1);
        assert_eq!(s.buffers[0].title, "first");
    }

    #[test]
    fn normalize_keeps_the_active_tab_after_earlier_ones_are_dropped() {
        let mut s = Session::default();
        s.buffers.push(scratch("bad/id", "dropped"));
        s.buffers.push(scratch("keep", "wanted"));
        s.active = 1;
        s.normalize();
        assert_eq!(s.buffers.len(), 1);
        assert_eq!(s.buffers[s.active].id, "keep");
    }

    #[test]
    fn normalize_clamps_an_out_of_range_active_index() {
        let mut s = Session::default();
        s.buffers.push(scratch("a", "one"));
        s.active = 99;
        s.normalize();
        assert_eq!(s.active, 0);

        let mut empty = Session {
            active: 7,
            ..Default::default()
        };
        empty.normalize();
        assert_eq!(
            empty.active, 0,
            "an empty session must not index past the end"
        );
    }

    #[test]
    fn normalize_fills_blank_titles() {
        let mut s = Session::default();
        s.buffers.push(Buffer {
            title: String::new(),
            ..scratch("a", "")
        });
        s.buffers.push(Buffer {
            title: "   ".into(),
            path: Some("/tmp/real.sql".into()),
            ..scratch("b", "")
        });
        s.normalize();
        assert_eq!(s.buffers[0].title, "new 1");
        assert_eq!(s.buffers[1].title, "real.sql");
    }

    #[test]
    fn index_of_path_finds_an_already_open_file() {
        let mut s = Session::default();
        s.buffers.push(scratch("a", "new 1"));
        s.buffers.push(Buffer::for_path("b", "/tmp/x.rs"));
        assert_eq!(s.index_of_path("/tmp/x.rs"), Some(1));
        assert_eq!(s.index_of_path("/tmp/other.rs"), None);
    }

    #[test]
    fn a_session_round_trips_through_json() {
        let mut s = Session::default();
        s.buffers.push(Buffer {
            caret: 42,
            scroll_top: 7,
            dirty: true,
            word_wrap: true,
            ..Buffer::for_path("id1", "/tmp/a.py")
        });
        s.active = 0;

        let text = serde_json::to_string(&s).unwrap();
        let back: Session = serde_json::from_str(&text).unwrap();
        assert_eq!(s, back);
    }

    #[test]
    fn a_session_missing_most_fields_still_loads() {
        // What a hand-edited or older file might look like.
        let back: Session = serde_json::from_str(r#"{"buffers":[{"id":"x"}]}"#).unwrap();
        assert_eq!(back.buffers.len(), 1);
        assert_eq!(back.buffers[0].language, "plain");
        assert_eq!(back.buffers[0].caret, 0);
    }
}
