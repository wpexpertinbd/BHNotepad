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
    /// Which window this tab lives in — a [`WindowRecord::id`].
    ///
    /// Defaults to [`MAIN_WINDOW`], so every session written before windows
    /// existed loads with all of its tabs in the one window, which is exactly
    /// where they were.
    #[serde(default = "default_window")]
    pub window: String,
}

/// The first window's id. It always exists: tabs belonging to a window that
/// has gone are brought here rather than being lost.
pub const MAIN_WINDOW: &str = "main";

fn default_window() -> String {
    MAIN_WINDOW.to_string()
}

/// One editor window, so a second window can live on a second monitor and be
/// found there again next launch.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct WindowRecord {
    /// Also the Tauri window label, so it must be a valid one: see
    /// [`is_safe_window_id`].
    pub id: String,
    /// The id of the tab in front in this window. `None` until it has one.
    #[serde(default)]
    pub active: Option<String>,
    /// Screen position and size in LOGICAL units, remembered so the window
    /// reopens where it was left — including on a second monitor.
    ///
    /// ⚠️ Logical, not physical pixels. The shell reads the platform's
    /// physical values and divides by the display's scale factor before
    /// storing them here; storing physical ones made a window on a Retina
    /// screen double its position and size on every quit.
    ///
    /// ⚠️ These are SCREEN coordinates, and the monitor they refer to may not
    /// be connected next time. The shell clamps them to a visible display
    /// before using them; a window restored to a monitor that is gone is a
    /// window the user cannot see or reach.
    #[serde(default)]
    pub x: Option<i32>,
    #[serde(default)]
    pub y: Option<i32>,
    #[serde(default)]
    pub width: Option<u32>,
    #[serde(default)]
    pub height: Option<u32>,
}

impl WindowRecord {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            active: None,
            x: None,
            y: None,
            width: None,
            height: None,
        }
    }
}

/// A window id becomes a Tauri window label and is compared against stored
/// ids, so it is held to the same shape as a buffer id.
pub fn is_safe_window_id(id: &str) -> bool {
    is_safe_id(id)
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
            window: default_window(),
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
            window: default_window(),
        }
    }

    /// True when this tab has never been written to a file.
    pub fn is_scratch(&self) -> bool {
        self.path.is_none()
    }
}

/// A tab that was closed, kept so it can be reopened.
///
/// ⚠️ This deliberately RETAINS the text of a closed tab, which cuts against
/// the rule that closing a tab should be the end of what was in it. Restoring
/// a tab closed by accident is worth more than that purity — Notepad++ keeps
/// one too — but the retention is bounded, and SECURITY.md says so plainly
/// rather than leaving it as a surprise.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ClosedTab {
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default = "default_language")]
    pub language: String,
    #[serde(default = "default_eol")]
    pub eol: String,
    /// Whether a snapshot of its text was kept. A saved file with no unsaved
    /// edits needs none — reopening re-reads the file.
    #[serde(default)]
    pub has_snapshot: bool,
}

/// How many closed tabs are remembered. Ten matches the length of Notepad++'s
/// own recent list and bounds how much closed text sits on disk.
pub const CLOSED_HISTORY: usize = 10;

/// How many recently opened file paths are remembered.
pub const RECENT_FILES: usize = 10;

/// Every open tab, in tab-bar order, plus which one is in front.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Session {
    #[serde(default = "default_version")]
    pub version: u32,
    #[serde(default)]
    pub buffers: Vec<Buffer>,
    /// Legacy: index into `buffers` of the main window's active tab.
    ///
    /// ⚠️ Kept because a session written before windows existed carries the
    /// focused tab HERE and nowhere else — that is the input the migration in
    /// [`Session::normalize_windows`] reads. It is now derived, never the
    /// source of truth: ask [`WindowRecord::active`] which tab a window is
    /// showing. It is still written so that installing an older build does not
    /// lose which tab was in front.
    #[serde(default)]
    pub active: usize,
    /// Files opened or saved recently, newest first.
    #[serde(default)]
    pub recent: Vec<String>,
    /// Tabs that were closed, newest first, for Restore Recent Closed File.
    #[serde(default)]
    pub closed: Vec<ClosedTab>,
    /// Every open window. [`Session::normalize`] guarantees at least one, and
    /// that every buffer's `window` names one of these.
    #[serde(default)]
    pub windows: Vec<WindowRecord>,
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
            recent: Vec::new(),
            closed: Vec::new(),
            windows: vec![WindowRecord::new(MAIN_WINDOW)],
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

        self.normalize_windows(active_id.clone());

        // Keep whichever tab was in front rather than resetting to the first,
        // even if earlier tabs were dropped.
        self.active = active_id
            .and_then(|id| self.buffers.iter().position(|b| b.id == id))
            .unwrap_or(0);
        if self.active >= self.buffers.len() {
            self.active = self.buffers.len().saturating_sub(1);
        }
        self.closed.truncate(CLOSED_HISTORY);
        self.closed.retain(|c| is_safe_id(&c.id));
        self.recent.truncate(RECENT_FILES);
        self.version = SESSION_VERSION;
        self.sync_legacy_active();
    }

    /// Recomputes the legacy `active` index from the main window.
    ///
    /// Derived, so that a session saved now and read by a build that predates
    /// windows still opens on the right tab instead of always on the first.
    pub fn sync_legacy_active(&mut self) {
        self.active = self
            .window(MAIN_WINDOW)
            .and_then(|w| w.active.clone())
            .and_then(|id| self.buffers.iter().position(|b| b.id == id))
            .unwrap_or(0);
    }

    /// Makes the window list consistent with the buffers, and migrates a
    /// session written before windows existed.
    ///
    /// `legacy_active` is the id of the tab the old single-window `active`
    /// index pointed at, which becomes the main window's active tab.
    fn normalize_windows(&mut self, legacy_active: Option<String>) {
        // Drop malformed and duplicate window ids.
        let mut seen: Vec<String> = Vec::new();
        self.windows.retain(|w| {
            if !is_safe_window_id(&w.id) || seen.contains(&w.id) {
                return false;
            }
            seen.push(w.id.clone());
            true
        });

        // ⚠️ The main window is not optional. Every tab has to have somewhere
        // to be, and a session whose only window record was dropped would
        // otherwise leave every buffer homeless.
        if !self.windows.iter().any(|w| w.id == MAIN_WINDOW) {
            self.windows.insert(0, WindowRecord::new(MAIN_WINDOW));
        }

        // A tab pointing at a window that is not there comes to the main
        // window rather than disappearing from every view of the session.
        let known: Vec<String> = self.windows.iter().map(|w| w.id.clone()).collect();
        for buffer in &mut self.buffers {
            if !known.contains(&buffer.window) {
                buffer.window = MAIN_WINDOW.to_string();
            }
        }

        // A session from before windows existed: one window, holding
        // everything, focused where it was.
        let migrating = self.windows.len() == 1 && self.windows[0].active.is_none();
        if migrating {
            if let Some(id) = legacy_active {
                if self.buffers.iter().any(|b| b.id == id) {
                    self.windows[0].active = Some(id);
                }
            }
        }

        // Every window's active tab must be one of its own.
        for window in &mut self.windows {
            let valid = window.active.as_ref().is_some_and(|id| {
                self.buffers
                    .iter()
                    .any(|b| b.id == *id && b.window == window.id)
            });
            if !valid {
                window.active = self
                    .buffers
                    .iter()
                    .filter(|b| b.window == window.id)
                    .map(|b| b.id.clone())
                    .next_back();
            }
        }
    }

    /// This window's tabs, in tab-bar order.
    pub fn buffers_in(&self, window: &str) -> Vec<&Buffer> {
        self.buffers.iter().filter(|b| b.window == window).collect()
    }

    /// Where a tab sits within its own window's strip, not in the whole session.
    pub fn index_in_window(&self, window: &str, id: &str) -> Option<usize> {
        self.buffers_in(window).iter().position(|b| b.id == id)
    }

    pub fn window(&self, id: &str) -> Option<&WindowRecord> {
        self.windows.iter().find(|w| w.id == id)
    }

    pub fn window_mut(&mut self, id: &str) -> Option<&mut WindowRecord> {
        self.windows.iter_mut().find(|w| w.id == id)
    }

    /// Adds a window record if it is not already there. Returns whether it was.
    pub fn ensure_window(&mut self, id: &str) -> bool {
        if !is_safe_window_id(id) || self.window(id).is_some() {
            return false;
        }
        self.windows.push(WindowRecord::new(id));
        true
    }

    /// Records a file as recently used, newest first and without duplicates.
    pub fn remember_recent(&mut self, path: &str) {
        self.recent.retain(|existing| existing != path);
        self.recent.insert(0, path.to_string());
        self.recent.truncate(RECENT_FILES);
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
    fn a_session_from_before_windows_existed_keeps_every_tab_and_its_focus() {
        // Exactly what v0.4.8 and earlier wrote: no `windows`, no `window`.
        //
        // ⚠️ `active` is deliberately NOT the last tab. With it on the last
        // one this test passed even with the migration removed, because the
        // fallback picks a window's last tab anyway — a green test proving
        // nothing. Three tabs focused on the first can only pass if the old
        // index was really carried across.
        let mut back: Session = serde_json::from_str(
            r#"{"buffers":[{"id":"a","title":"new 1"},{"id":"b","title":"new 2"},
                           {"id":"c","title":"new 3"}],"active":0}"#,
        )
        .unwrap();
        back.normalize();
        assert_eq!(back.windows.len(), 1, "one window, as there always was");
        assert_eq!(back.windows[0].id, MAIN_WINDOW);
        assert_eq!(back.buffers.len(), 3, "every tab must survive the upgrade");
        assert!(back.buffers.iter().all(|b| b.window == MAIN_WINDOW));
        assert_eq!(
            back.windows[0].active.as_deref(),
            Some("a"),
            "the tab that was in front must still be in front"
        );
    }

    #[test]
    fn a_tab_whose_window_is_gone_comes_to_the_main_window() {
        let mut s = Session::default();
        s.buffers.push(Buffer {
            window: "vanished".into(),
            ..scratch("a", "new 1")
        });
        s.normalize();
        assert_eq!(s.buffers[0].window, MAIN_WINDOW);
        assert_eq!(s.buffers_in(MAIN_WINDOW).len(), 1);
    }

    #[test]
    fn the_main_window_record_cannot_be_removed() {
        let mut s = Session {
            windows: vec![WindowRecord::new("second")],
            ..Default::default()
        };
        s.normalize();
        assert!(s.window(MAIN_WINDOW).is_some());
    }

    #[test]
    fn each_window_keeps_its_own_active_tab() {
        let mut s = Session::default();
        s.ensure_window("second");
        s.buffers.push(scratch("a", "new 1"));
        s.buffers.push(Buffer {
            window: "second".into(),
            ..scratch("b", "new 2")
        });
        s.buffers.push(Buffer {
            window: "second".into(),
            ..scratch("c", "new 3")
        });
        s.window_mut("second").unwrap().active = Some("b".into());
        s.normalize();
        assert_eq!(s.window(MAIN_WINDOW).unwrap().active.as_deref(), Some("a"));
        assert_eq!(s.window("second").unwrap().active.as_deref(), Some("b"));
        assert_eq!(s.index_in_window("second", "c"), Some(1));
    }

    #[test]
    fn a_window_cannot_point_at_another_windows_tab() {
        let mut s = Session::default();
        s.ensure_window("second");
        s.buffers.push(scratch("a", "new 1"));
        // "a" lives in the main window, so "second" must not claim it.
        s.window_mut("second").unwrap().active = Some("a".into());
        s.normalize();
        assert_eq!(s.window("second").unwrap().active, None);
    }

    #[test]
    fn an_unsafe_window_id_is_rejected() {
        let mut s = Session::default();
        assert!(!s.ensure_window("../escape"));
        assert!(!s.ensure_window(""));
        assert!(s.ensure_window("w_2"));
        assert!(!s.ensure_window("w_2"), "already there");
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
