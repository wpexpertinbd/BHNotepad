//! BHNotepad's portable core.
//!
//! Owns the session model, the crash-safe on-disk store and language
//! detection. Deliberately knows nothing about Tauri, webviews, windows or
//! menus, and never asks the operating system where files live — the shell
//! injects the config directory. That is what lets the same core back the
//! macOS, Windows and Linux builds. See CLAUDE.md.
//!
//! The one promise this crate exists to keep: **a tab closed or an app quit
//! with unsaved text loses nothing and asks nothing.** Text is written through
//! to disk as it is typed, so quitting is never a decision point.

pub mod language;
pub mod session;
pub mod store;

pub use language::detect as detect_language;
pub use session::{Buffer, Session, SESSION_VERSION};
pub use store::{new_id, Store, StoreError};

use std::fs;
use std::path::Path;

/// Largest file that may be opened into a tab.
///
/// Not a performance promise — a safety ceiling. The text is held in memory,
/// copied across to the editor, and snapshotted on every autosave, so an
/// unbounded read is a way to wedge the app rather than a feature. Generous
/// enough for any log or source file someone would genuinely want to edit.
pub const MAX_OPEN_BYTES: u64 = 64 * 1024 * 1024;

/// Formats a byte count for an error the user will actually read.
fn human_bytes(bytes: u64) -> String {
    const MB: u64 = 1024 * 1024;
    const KB: u64 = 1024;
    if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{bytes} bytes")
    }
}

/// A session plus the store it is persisted to.
///
/// The shell drives this: every editor action becomes one call, and the
/// workspace decides what must reach disk. Keeping the decisions here rather
/// than in the shell is what keeps the three platform builds identical in
/// behaviour.
#[derive(Debug)]
pub struct Workspace {
    store: Store,
    session: Session,
}

impl Workspace {
    /// Opens the workspace at `root`, restoring the previous session.
    ///
    /// The second return value is a note for the log when the previous session
    /// could not be read in full. Opening never fails on a bad session file —
    /// the editor must always start.
    pub fn open(root: impl AsRef<Path>) -> Result<(Self, Option<String>), StoreError> {
        let store = Store::new(root.as_ref())?;
        let (session, note) = store.load_session();
        let mut workspace = Self { store, session };
        // Drop snapshots for tabs that are no longer open, so the store does
        // not grow for the life of the install.
        workspace.store.prune_orphans(&workspace.session);
        if workspace.ensure_not_empty() {
            // A brand new tab exists only in memory until this runs. Writing it
            // now means even a force-kill before the first keystroke still
            // leaves a session to restore, rather than a store with no record
            // of the tab the user is looking at.
            workspace
                .store
                .write_buffer(&workspace.session.buffers[0].id, "")?;
            workspace.persist()?;
        }
        Ok((workspace, note))
    }

    pub fn session(&self) -> &Session {
        &self.session
    }

    pub fn store(&self) -> &Store {
        &self.store
    }

    /// Guarantees at least one tab, so the window is never blank on launch.
    ///
    /// Returns whether a tab had to be created, so the caller knows there is
    /// something new to write to disk.
    fn ensure_not_empty(&mut self) -> bool {
        if !self.session.buffers.is_empty() {
            return false;
        }
        let title = self.session.next_untitled_title();
        self.session.buffers.push(Buffer::untitled(new_id(), title));
        self.session.active = 0;
        true
    }

    /// Writes `session.json`. Cheap — call it after anything structural.
    pub fn persist(&self) -> Result<(), StoreError> {
        self.store.save_session(&self.session)
    }

    /// Reads a tab's text back from its snapshot.
    pub fn text(&self, id: &str) -> Result<String, StoreError> {
        self.store.read_buffer(id)
    }

    /// Adds an empty scratch tab and focuses it. Returns its id.
    pub fn new_scratch(&mut self) -> Result<String, StoreError> {
        let id = new_id();
        let title = self.session.next_untitled_title();
        self.session.buffers.push(Buffer::untitled(&id, title));
        self.session.active = self.session.buffers.len() - 1;
        // Create the snapshot immediately so a crash one keystroke later still
        // finds a file to restore.
        self.store.write_buffer(&id, "")?;
        self.persist()?;
        Ok(id)
    }

    /// Opens a file from disk, or focuses the tab already showing it.
    ///
    /// Returns the tab id and its text.
    pub fn open_path(&mut self, path: impl AsRef<Path>) -> Result<(String, String), StoreError> {
        let path_ref = path.as_ref();
        let path_string = path_ref.to_string_lossy().into_owned();

        if let Some(index) = self.session.index_of_path(&path_string) {
            self.session.active = index;
            let id = self.session.buffers[index].id.clone();
            let text = self.store.read_buffer(&id)?;
            self.persist()?;
            return Ok((id, text));
        }

        // Check what this is before reading it. Now that files can be opened
        // by double-clicking, `fs::read` on its own would happily try to pull
        // a 5 GB log — or a character device like /dev/zero, which never
        // ends — into memory and then across to the editor.
        let meta = fs::metadata(path_ref).map_err(|source| StoreError::Io {
            context: format!("opening {}", path_ref.display()),
            source,
        })?;
        if !meta.is_file() {
            return Err(StoreError::Io {
                context: format!("{} is not a regular file", path_ref.display()),
                source: std::io::Error::new(std::io::ErrorKind::InvalidInput, "not a regular file"),
            });
        }
        if meta.len() > MAX_OPEN_BYTES {
            return Err(StoreError::Io {
                context: format!(
                    "{} is {} and the limit is {}",
                    path_ref.display(),
                    human_bytes(meta.len()),
                    human_bytes(MAX_OPEN_BYTES)
                ),
                source: std::io::Error::new(std::io::ErrorKind::InvalidInput, "file too large"),
            });
        }

        let bytes = fs::read(path_ref).map_err(|source| StoreError::Io {
            context: format!("opening {}", path_ref.display()),
            source,
        })?;
        let text = store::decode_utf8_lossy(bytes);

        let id = new_id();
        self.session
            .buffers
            .push(Buffer::for_path(&id, &path_string));
        self.session.active = self.session.buffers.len() - 1;
        self.store.write_buffer(&id, &text)?;
        self.persist()?;
        Ok((id, text))
    }

    /// Records edited text for a tab.
    ///
    /// This is the write-through that makes quitting safe, so the shell must
    /// call it as the user types (debounced), not at close time.
    pub fn update_text(&mut self, id: &str, text: &str) -> Result<(), StoreError> {
        // Ignore a tab that is no longer open. The UI debounces this call, so
        // a keystroke a moment before a tab is closed arrives a moment after —
        // and writing then would put the text of a closed tab back on disk,
        // where it would sit until the next launch pruned it. Someone who
        // closes a tab to be rid of what is in it should be rid of it.
        if self.session.buffer(id).is_none() {
            return Ok(());
        }
        self.store.write_buffer(id, text)?;

        let mut became_dirty = false;
        if let Some(buffer) = self.session.buffer_mut(id) {
            // A scratch buffer has no file to differ from, but showing it as
            // dirty is what tells the user the text is only in the session —
            // which is exactly how Notepad++ presents `new 1`.
            became_dirty = !buffer.dirty;
            buffer.dirty = true;
        }
        // Persist only on the false -> true edge: once per tab rather than
        // once per keystroke. Without this the flag lives in memory only, and
        // a tab that was killed before the next structural change came back
        // looking clean while its text was plainly unsaved.
        if became_dirty {
            self.persist()?;
        }
        Ok(())
    }

    /// Records where the caret and viewport are, so the tab reopens in place.
    pub fn update_position(&mut self, id: &str, caret: u64, scroll_top: u64) {
        if let Some(buffer) = self.session.buffer_mut(id) {
            buffer.caret = caret;
            buffer.scroll_top = scroll_top;
        }
    }

    pub fn set_word_wrap(&mut self, id: &str, wrap: bool) {
        if let Some(buffer) = self.session.buffer_mut(id) {
            buffer.word_wrap = wrap;
        }
    }

    pub fn set_language(&mut self, id: &str, language: &str) {
        if let Some(buffer) = self.session.buffer_mut(id) {
            buffer.language = language.to_string();
        }
    }

    pub fn set_active(&mut self, index: usize) {
        if index < self.session.buffers.len() {
            self.session.active = index;
        }
    }

    /// Moves a tab, for drag-to-reorder.
    pub fn reorder(&mut self, from: usize, to: usize) {
        let len = self.session.buffers.len();
        if from >= len || to >= len || from == to {
            return;
        }
        let active_id = self.session.buffers[self.session.active].id.clone();
        let buffer = self.session.buffers.remove(from);
        self.session.buffers.insert(to, buffer);
        // Follow the tab the user was on, not its old index.
        if let Some(index) = self.session.index_of(&active_id) {
            self.session.active = index;
        }
    }

    /// Writes a tab to a real file and clears its dirty flag.
    ///
    /// With `path` given this is Save As, and the tab adopts the new path,
    /// title and language.
    pub fn save(&mut self, id: &str, path: Option<&str>) -> Result<String, StoreError> {
        let target = match path {
            Some(path) => path.to_string(),
            None => match self.session.buffer(id).and_then(|b| b.path.clone()) {
                Some(existing) => existing,
                None => {
                    return Err(StoreError::Io {
                        context: format!("tab {id} has never been saved, so it needs a path"),
                        source: std::io::Error::new(std::io::ErrorKind::InvalidInput, "no path"),
                    })
                }
            },
        };

        let text = self.store.read_buffer(id)?;
        // The user's own file gets the same atomic treatment as the store: an
        // interrupted save must not truncate a file they already had.
        let target_path = Path::new(&target);
        write_user_file(target_path, text.as_bytes())?;

        if let Some(buffer) = self.session.buffer_mut(id) {
            buffer.path = Some(target.clone());
            buffer.title = language::base_name(&target).to_string();
            buffer.language = language::detect(&target).to_string();
            buffer.dirty = false;
        }
        self.persist()?;
        Ok(target)
    }

    /// Closes every tab, leaving one empty scratch tab behind.
    ///
    /// Returns how many were closed. Unlike [`Workspace::close`], this is a
    /// bulk destructive action: scratch text that was never saved to a file is
    /// gone afterwards. The shell is expected to ask once before calling it —
    /// that single confirmation is the exception to the app's no-prompt rule,
    /// not a softening of it.
    pub fn close_all(&mut self) -> Result<usize, StoreError> {
        let ids: Vec<String> = self.session.buffers.iter().map(|b| b.id.clone()).collect();
        let closed = ids.len();
        for id in &ids {
            self.store.delete_buffer(id)?;
        }
        self.session.buffers.clear();
        self.session.active = 0;

        if self.ensure_not_empty() {
            let fresh = self.session.buffers[0].id.clone();
            self.store.write_buffer(&fresh, "")?;
        }
        self.persist()?;
        Ok(closed)
    }

    /// Every tab that has never been saved to a file, in tab order.
    ///
    /// Save All walks these one at a time asking where each should go; tabs
    /// that already have a path are written without asking.
    pub fn unsaved_tabs(&self) -> Vec<Buffer> {
        self.session
            .buffers
            .iter()
            .filter(|b| b.is_scratch())
            .cloned()
            .collect()
    }

    /// Closes a tab and deletes its snapshot. Never prompts — that is the point
    /// of the app.
    ///
    /// Closing the last tab leaves a fresh empty one, so there is always
    /// somewhere to type.
    pub fn close(&mut self, id: &str) -> Result<(), StoreError> {
        let Some(index) = self.session.index_of(id) else {
            return Ok(());
        };
        self.session.buffers.remove(index);
        self.store.delete_buffer(id)?;

        if self.session.active >= self.session.buffers.len() {
            self.session.active = self.session.buffers.len().saturating_sub(1);
        }
        if self.ensure_not_empty() {
            let new_tab = self.session.buffers[0].id.clone();
            self.store.write_buffer(&new_tab, "")?;
        }
        self.persist()?;
        Ok(())
    }
}

/// Atomic write for a file the user chose, falling back to a direct write when
/// the directory will not take a temporary file.
///
/// A rename cannot cross a filesystem boundary and may be refused on some
/// network or synced folders; for the user's own Save, completing the write is
/// more important than the atomicity guarantee we insist on for the store.
fn write_user_file(path: &Path, data: &[u8]) -> Result<(), StoreError> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let tmp = parent.join(format!(".bhn-save-{}.tmp", new_id()));

    let attempt = (|| -> std::io::Result<()> {
        use std::io::Write as _;
        let mut file = fs::File::create(&tmp)?;
        file.write_all(data)?;
        file.sync_all()?;
        Ok(())
    })();

    // A rename replaces the destination's permissions with the temporary
    // file's. Without this, re-saving a file the user had deliberately made
    // private (0600) would silently widen it to whatever the umask gives,
    // which is usually world-readable.
    if attempt.is_ok() {
        if let Ok(existing) = fs::metadata(path) {
            let _ = fs::set_permissions(&tmp, existing.permissions());
        }
    }

    if attempt.is_ok() && fs::rename(&tmp, path).is_ok() {
        return Ok(());
    }
    let _ = fs::remove_file(&tmp);

    fs::write(path, data).map_err(|source| StoreError::Io {
        context: format!("saving {}", path.display()),
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn workspace() -> (tempfile::TempDir, Workspace) {
        let dir = tempfile::tempdir().unwrap();
        let (workspace, note) = Workspace::open(dir.path()).unwrap();
        assert!(note.is_none());
        (dir, workspace)
    }

    #[test]
    fn a_first_launch_opens_exactly_one_scratch_tab() {
        let (_dir, workspace) = workspace();
        assert_eq!(workspace.session().buffers.len(), 1);
        assert_eq!(workspace.session().buffers[0].title, "new 1");
        assert_eq!(workspace.session().active, 0);
    }

    #[test]
    fn the_first_tab_reaches_disk_before_anything_is_typed() {
        // A force-kill one second after launch must still leave a restorable
        // session, so opening cannot leave the first tab only in memory.
        let dir = tempfile::tempdir().unwrap();
        let id = {
            let (workspace, _) = Workspace::open(dir.path()).unwrap();
            workspace.session().buffers[0].id.clone()
        };

        let observer = Store::new(dir.path()).unwrap();
        assert!(
            observer.session_path().exists(),
            "session.json should exist immediately after a first launch"
        );
        let (session, note) = observer.load_session();
        assert!(note.is_none());
        assert_eq!(session.buffers.len(), 1);
        assert_eq!(session.buffers[0].id, id);
    }

    #[test]
    fn a_second_launch_does_not_add_another_empty_tab() {
        let dir = tempfile::tempdir().unwrap();
        let first = {
            let (workspace, _) = Workspace::open(dir.path()).unwrap();
            workspace.session().buffers[0].id.clone()
        };
        let (again, _) = Workspace::open(dir.path()).unwrap();
        assert_eq!(again.session().buffers.len(), 1);
        assert_eq!(again.session().buffers[0].id, first);
    }

    // ── The feature the whole app exists for ───────────────────────────────

    #[test]
    fn ten_dirty_scratch_tabs_all_come_back_after_a_quit() {
        let dir = tempfile::tempdir().unwrap();
        let mut ids = Vec::new();

        {
            let (mut workspace, _) = Workspace::open(dir.path()).unwrap();
            let first = workspace.session().buffers[0].id.clone();
            workspace.update_text(&first, "note 0").unwrap();
            workspace.update_position(&first, 6, 0);
            ids.push(first);

            for i in 1..10 {
                let id = workspace.new_scratch().unwrap();
                workspace.update_text(&id, &format!("note {i}")).unwrap();
                ids.push(id);
            }
            workspace.set_active(4);
            workspace.persist().unwrap();
            // Dropped here with no save prompt and no explicit flush: exactly
            // what a quit looks like.
        }

        let (restored, note) = Workspace::open(dir.path()).unwrap();
        assert!(note.is_none(), "restoring should be clean, got: {note:?}");
        assert_eq!(restored.session().buffers.len(), 10);
        assert_eq!(restored.session().active, 4);
        for (i, id) in ids.iter().enumerate() {
            assert_eq!(
                restored.text(id).unwrap(),
                format!("note {i}"),
                "tab {i} lost its text"
            );
        }
        assert_eq!(restored.session().buffers[0].caret, 6);
    }

    /// Models a power cut, not a quit.
    ///
    /// A child process opens the workspace, types into several tabs and then
    /// calls `abort()`, so it dies on SIGABRT with no unwinding, no `Drop`, no
    /// flush and no chance to write anything on the way out — the closest a
    /// test can get to the machine losing power. The parent then reopens the
    /// same directory and checks every tab is intact.
    #[test]
    fn tabs_survive_the_process_being_killed_outright() {
        const MARKER: &str = "BHN_ABORT_CHILD_DIR";

        // The child is this same test binary, re-entered through the marker
        // variable; `current_exe` keeps it working under cargo's temporary
        // target paths.
        if let Ok(dir) = std::env::var(MARKER) {
            let (mut workspace, _) = Workspace::open(&dir).unwrap();
            let first = workspace.session().buffers[0].id.clone();
            workspace.update_text(&first, "tab one survived").unwrap();
            for i in 2..=5 {
                let id = workspace.new_scratch().unwrap();
                workspace
                    .update_text(&id, &format!("tab {i} survived"))
                    .unwrap();
            }
            // A sixth tab, created clean and persisted clean by new_scratch
            // (which also focuses it, hence the set_active afterwards).
            let fresh = workspace.new_scratch().unwrap();
            workspace.set_active(2);
            workspace.persist().unwrap();
            // Typing into that still-clean tab is now the ONLY false -> true
            // dirty edge after the last explicit persist, so it is what proves
            // the flag is written on the edge rather than merely held in
            // memory. Editing an already-dirty tab here would make the
            // assertion below pass no matter what.
            workspace.update_text(&fresh, "tab 6 survived").unwrap();
            // No flush, no unwind, no destructors. Just gone.
            std::process::abort();
        }

        let dir = tempfile::tempdir().unwrap();
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            // `--exact` matches the full test path, so the module prefix is
            // required -- with the bare name the child silently runs nothing,
            // exits 0, and the test passes for entirely the wrong reason.
            .args([
                "tests::tabs_survive_the_process_being_killed_outright",
                "--exact",
                "--nocapture",
            ])
            .env(MARKER, dir.path())
            .output()
            .expect("could not start the child process");

        assert!(
            !status.status.success(),
            "the child was supposed to abort, not exit cleanly -- stdout: {}",
            String::from_utf8_lossy(&status.stdout)
        );

        let (restored, note) = Workspace::open(dir.path()).unwrap();
        assert!(
            note.is_none(),
            "restoring after a kill should be clean: {note:?}"
        );
        assert_eq!(
            restored.session().buffers.len(),
            6,
            "all six tabs should return"
        );
        assert_eq!(
            restored.session().active,
            2,
            "and on the tab that was in front"
        );
        assert!(
            restored.session().buffers.iter().all(|b| b.dirty),
            "every tab held unsaved text, so every tab must still look unsaved: {:?}",
            restored
                .session()
                .buffers
                .iter()
                .map(|b| (b.title.as_str(), b.dirty))
                .collect::<Vec<_>>()
        );

        let mut texts: Vec<String> = restored
            .session()
            .buffers
            .iter()
            .map(|b| restored.text(&b.id).unwrap())
            .collect();
        texts.sort();
        assert_eq!(
            texts,
            vec![
                "tab 2 survived",
                "tab 3 survived",
                "tab 4 survived",
                "tab 5 survived",
                "tab 6 survived",
                "tab one survived",
            ]
        );
    }

    #[test]
    fn text_is_on_disk_before_any_quit_happens() {
        // Proves the write-through: the snapshot is readable from a second,
        // independent handle without the first one being closed or flushed.
        let dir = tempfile::tempdir().unwrap();
        let (mut workspace, _) = Workspace::open(dir.path()).unwrap();
        let id = workspace.session().buffers[0].id.clone();
        workspace.update_text(&id, "typed but never saved").unwrap();

        let observer = Store::new(dir.path()).unwrap();
        assert_eq!(observer.read_buffer(&id).unwrap(), "typed but never saved");
    }

    #[test]
    fn a_scratch_tab_is_marked_dirty_and_that_is_not_an_error_state() {
        let (_dir, mut workspace) = workspace();
        let id = workspace.session().buffers[0].id.clone();
        workspace.update_text(&id, "x").unwrap();
        let buffer = workspace.session().buffer(&id).unwrap();
        assert!(buffer.dirty);
        assert!(buffer.is_scratch());
    }

    // ── Tabs ──────────────────────────────────────────────────────────────

    #[test]
    fn closing_a_dirty_tab_just_closes_it() {
        let (_dir, mut workspace) = workspace();
        let first = workspace.session().buffers[0].id.clone();
        let second = workspace.new_scratch().unwrap();
        workspace.update_text(&second, "unsaved work").unwrap();

        workspace.close(&second).unwrap();

        assert_eq!(workspace.session().buffers.len(), 1);
        assert_eq!(workspace.session().buffers[0].id, first);
        assert_eq!(
            workspace.store().read_buffer(&second).unwrap(),
            "",
            "a closed tab's snapshot should be gone"
        );
    }

    #[test]
    fn closing_the_last_tab_leaves_a_fresh_empty_one() {
        let (_dir, mut workspace) = workspace();
        let only = workspace.session().buffers[0].id.clone();
        workspace.close(&only).unwrap();
        assert_eq!(workspace.session().buffers.len(), 1);
        assert_ne!(workspace.session().buffers[0].id, only);
        assert_eq!(workspace.session().active, 0);
    }

    #[test]
    fn close_all_clears_every_tab_and_leaves_one_empty_one() {
        let (_dir, mut workspace) = workspace();
        let first = workspace.session().buffers[0].id.clone();
        workspace.update_text(&first, "one").unwrap();
        let second = workspace.new_scratch().unwrap();
        workspace.update_text(&second, "two").unwrap();
        let third = workspace.new_scratch().unwrap();

        let closed = workspace.close_all().unwrap();

        assert_eq!(closed, 3);
        assert_eq!(workspace.session().buffers.len(), 1);
        assert_eq!(workspace.session().active, 0);
        assert_eq!(workspace.session().buffers[0].title, "new 1");
        assert!(!workspace.session().buffers[0].dirty);
        for id in [&first, &second, &third] {
            assert_eq!(
                workspace.store().read_buffer(id).unwrap(),
                "",
                "every snapshot should be gone"
            );
        }
    }

    #[test]
    fn close_all_survives_a_restart_as_a_single_empty_tab() {
        let dir = tempfile::tempdir().unwrap();
        {
            let (mut workspace, _) = Workspace::open(dir.path()).unwrap();
            let id = workspace.session().buffers[0].id.clone();
            workspace.update_text(&id, "will be discarded").unwrap();
            workspace.new_scratch().unwrap();
            workspace.close_all().unwrap();
        }
        let (restored, note) = Workspace::open(dir.path()).unwrap();
        assert!(note.is_none());
        assert_eq!(restored.session().buffers.len(), 1);
        assert_eq!(
            restored.text(&restored.session().buffers[0].id).unwrap(),
            ""
        );
    }

    #[test]
    fn unsaved_tabs_lists_only_the_scratch_ones_in_order() {
        let (dir, mut workspace) = workspace();
        let first = workspace.session().buffers[0].id.clone();
        let file = dir.path().join("already.txt");
        fs::write(&file, "on disk").unwrap();
        workspace.open_path(&file).unwrap();
        let third = workspace.new_scratch().unwrap();

        let pending: Vec<String> = workspace.unsaved_tabs().into_iter().map(|b| b.id).collect();
        assert_eq!(
            pending,
            vec![first, third],
            "the opened file is not pending"
        );
    }

    #[test]
    fn a_tab_saved_to_a_file_drops_out_of_unsaved_tabs() {
        let (dir, mut workspace) = workspace();
        let id = workspace.session().buffers[0].id.clone();
        workspace.update_text(&id, "content").unwrap();
        assert_eq!(workspace.unsaved_tabs().len(), 1);

        let target = dir.path().join("now_named.txt");
        workspace
            .save(&id, Some(&target.to_string_lossy()))
            .unwrap();
        assert!(workspace.unsaved_tabs().is_empty());
    }

    #[test]
    fn a_late_autosave_cannot_put_a_closed_tabs_text_back_on_disk() {
        // The UI debounces update_text, so a keystroke just before a close
        // lands just after it. That must not resurrect the file.
        let (_dir, mut workspace) = workspace();
        let doomed = workspace.new_scratch().unwrap();
        workspace
            .update_text(&doomed, "a password, pasted for a second")
            .unwrap();
        workspace.close(&doomed).unwrap();

        workspace
            .update_text(&doomed, "a password, pasted for a second")
            .unwrap();

        assert_eq!(
            workspace.store().read_buffer(&doomed).unwrap(),
            "",
            "the closed tab's text must stay gone"
        );
        assert!(workspace.session().buffer(&doomed).is_none());
    }

    #[test]
    fn a_late_autosave_after_close_all_writes_nothing() {
        let (_dir, mut workspace) = workspace();
        let first = workspace.session().buffers[0].id.clone();
        workspace.update_text(&first, "secret").unwrap();
        workspace.close_all().unwrap();

        workspace.update_text(&first, "secret").unwrap();
        assert_eq!(workspace.store().read_buffer(&first).unwrap(), "");
    }

    #[test]
    fn closing_an_unknown_id_is_harmless() {
        let (_dir, mut workspace) = workspace();
        workspace.close("nosuchtab").unwrap();
        assert_eq!(workspace.session().buffers.len(), 1);
    }

    #[test]
    fn a_closed_untitled_number_is_reused() {
        let (_dir, mut workspace) = workspace();
        let second = workspace.new_scratch().unwrap();
        assert_eq!(workspace.session().buffer(&second).unwrap().title, "new 2");
        let third = workspace.new_scratch().unwrap();
        assert_eq!(workspace.session().buffer(&third).unwrap().title, "new 3");

        workspace.close(&second).unwrap();
        let fourth = workspace.new_scratch().unwrap();
        assert_eq!(
            workspace.session().buffer(&fourth).unwrap().title,
            "new 2",
            "Notepad++ reuses the lowest free number"
        );
    }

    #[test]
    fn reorder_keeps_the_user_on_the_tab_they_were_viewing() {
        let (_dir, mut workspace) = workspace();
        let b = workspace.new_scratch().unwrap();
        let _c = workspace.new_scratch().unwrap();
        let index_of_b = workspace.session().index_of(&b).unwrap();
        workspace.set_active(index_of_b);

        workspace.reorder(0, 2);

        assert_eq!(
            workspace.session().buffers[workspace.session().active].id,
            b,
            "the active tab followed its move, rather than its old index"
        );
    }

    #[test]
    fn reorder_ignores_out_of_range_requests() {
        let (_dir, mut workspace) = workspace();
        let before = workspace.session().clone();
        workspace.reorder(0, 9);
        workspace.reorder(9, 0);
        workspace.reorder(0, 0);
        assert_eq!(&before, workspace.session());
    }

    // ── Files ─────────────────────────────────────────────────────────────

    #[test]
    fn opening_a_file_reads_it_and_detects_its_language() {
        let (dir, mut workspace) = workspace();
        let file = dir.path().join("script.py");
        fs::write(&file, "print('hi')\n").unwrap();

        let (id, text) = workspace.open_path(&file).unwrap();
        assert_eq!(text, "print('hi')\n");
        let buffer = workspace.session().buffer(&id).unwrap();
        assert_eq!(buffer.language, "python");
        assert_eq!(buffer.title, "script.py");
        assert!(!buffer.dirty);
    }

    #[test]
    fn opening_the_same_file_twice_focuses_the_first_tab() {
        let (dir, mut workspace) = workspace();
        let file = dir.path().join("once.txt");
        fs::write(&file, "body").unwrap();

        let (first, _) = workspace.open_path(&file).unwrap();
        let count = workspace.session().buffers.len();
        let (second, text) = workspace.open_path(&file).unwrap();

        assert_eq!(first, second);
        assert_eq!(text, "body");
        assert_eq!(workspace.session().buffers.len(), count, "no duplicate tab");
    }

    #[test]
    fn refuses_a_file_larger_than_the_cap_without_reading_it() {
        let (dir, mut workspace) = workspace();
        let huge = dir.path().join("enormous.log");
        // Sparse: set_len does not actually write 65 MB to the disk.
        let file = fs::File::create(&huge).unwrap();
        file.set_len(MAX_OPEN_BYTES + 1).unwrap();
        drop(file);

        let before = workspace.session().clone();
        let error = workspace.open_path(&huge).unwrap_err().to_string();
        assert!(
            error.contains("limit"),
            "the message should say why: {error}"
        );
        assert_eq!(
            &before,
            workspace.session(),
            "nothing should have been opened"
        );
    }

    #[test]
    fn a_file_right_at_the_cap_is_still_allowed() {
        let (dir, mut workspace) = workspace();
        let big = dir.path().join("just_fits.log");
        let file = fs::File::create(&big).unwrap();
        file.set_len(MAX_OPEN_BYTES).unwrap();
        drop(file);
        assert!(
            workspace.open_path(&big).is_ok(),
            "the cap must be inclusive"
        );
    }

    #[test]
    fn refuses_anything_that_is_not_a_regular_file() {
        // A directory stands in for the real hazard: a character device such
        // as /dev/zero, which fs::read would never finish reading.
        let (dir, mut workspace) = workspace();
        let folder = dir.path().join("a_directory");
        fs::create_dir(&folder).unwrap();
        let error = workspace.open_path(&folder).unwrap_err().to_string();
        assert!(error.contains("not a regular file"), "got: {error}");
    }

    #[cfg(unix)]
    #[test]
    fn the_store_is_readable_only_by_this_user() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let (mut workspace, _) = Workspace::open(dir.path()).unwrap();
        let id = workspace.session().buffers[0].id.clone();
        workspace
            .update_text(&id, "a token someone pasted here")
            .unwrap();
        workspace.persist().unwrap();

        let mode = |p: &Path| fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(dir.path()), 0o700, "store directory");
        assert_eq!(
            mode(&workspace.store().buffers_dir()),
            0o700,
            "buffers directory"
        );
        assert_eq!(
            mode(&workspace.store().session_path()),
            0o600,
            "session.json"
        );
        assert_eq!(
            mode(&workspace.store().buffers_dir().join(format!("{id}.txt"))),
            0o600,
            "the scratch note itself"
        );
    }

    #[cfg(unix)]
    #[test]
    fn saving_does_not_widen_the_permissions_of_an_existing_file() {
        use std::os::unix::fs::PermissionsExt;

        let (dir, mut workspace) = workspace();
        let secret = dir.path().join("private.txt");
        fs::write(&secret, "first").unwrap();
        fs::set_permissions(&secret, fs::Permissions::from_mode(0o600)).unwrap();

        let id = workspace.session().buffers[0].id.clone();
        workspace.update_text(&id, "second").unwrap();
        workspace
            .save(&id, Some(&secret.to_string_lossy()))
            .unwrap();

        assert_eq!(fs::read_to_string(&secret).unwrap(), "second");
        assert_eq!(
            fs::metadata(&secret).unwrap().permissions().mode() & 0o777,
            0o600,
            "a deliberately private file must not become world-readable on save"
        );
    }

    #[test]
    fn opening_a_missing_file_reports_an_error_and_changes_nothing() {
        let (dir, mut workspace) = workspace();
        let before = workspace.session().clone();
        let result = workspace.open_path(dir.path().join("absent.txt"));
        assert!(result.is_err());
        assert_eq!(&before, workspace.session());
    }

    #[test]
    fn saving_writes_the_file_and_clears_dirty() {
        let (dir, mut workspace) = workspace();
        let id = workspace.session().buffers[0].id.clone();
        workspace.update_text(&id, "content to keep").unwrap();

        let target = dir.path().join("kept.md");
        let saved = workspace
            .save(&id, Some(&target.to_string_lossy()))
            .unwrap();

        assert_eq!(fs::read_to_string(&saved).unwrap(), "content to keep");
        let buffer = workspace.session().buffer(&id).unwrap();
        assert!(!buffer.dirty);
        assert_eq!(buffer.title, "kept.md");
        assert_eq!(buffer.language, "markdown");
        assert!(!buffer.is_scratch());
    }

    #[test]
    fn a_second_save_needs_no_path() {
        let (dir, mut workspace) = workspace();
        let id = workspace.session().buffers[0].id.clone();
        let target = dir.path().join("again.txt");
        workspace.update_text(&id, "one").unwrap();
        workspace
            .save(&id, Some(&target.to_string_lossy()))
            .unwrap();

        workspace.update_text(&id, "two").unwrap();
        workspace.save(&id, None).unwrap();
        assert_eq!(fs::read_to_string(&target).unwrap(), "two");
    }

    #[test]
    fn saving_a_never_saved_tab_without_a_path_is_an_error_not_a_silent_no_op() {
        let (_dir, mut workspace) = workspace();
        let id = workspace.session().buffers[0].id.clone();
        workspace.update_text(&id, "needs a destination").unwrap();
        assert!(workspace.save(&id, None).is_err());
    }

    #[test]
    fn saving_leaves_no_temporary_file_beside_the_users_file() {
        let (dir, mut workspace) = workspace();
        let id = workspace.session().buffers[0].id.clone();
        workspace.update_text(&id, "x").unwrap();
        let target = dir.path().join("tidy.txt");
        workspace
            .save(&id, Some(&target.to_string_lossy()))
            .unwrap();

        let leftovers: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.contains(".tmp"))
            .collect();
        assert!(leftovers.is_empty(), "left behind: {leftovers:?}");
    }

    #[test]
    fn a_saved_file_reopens_with_its_text_after_a_restart() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("persisted.sql");
        let id;
        {
            let (mut workspace, _) = Workspace::open(dir.path()).unwrap();
            id = workspace.session().buffers[0].id.clone();
            workspace.update_text(&id, "select 1;").unwrap();
            workspace
                .save(&id, Some(&target.to_string_lossy()))
                .unwrap();
        }
        let (restored, _) = Workspace::open(dir.path()).unwrap();
        let buffer = restored.session().buffer(&id).unwrap();
        assert_eq!(
            buffer.path.as_deref(),
            Some(target.to_string_lossy().as_ref())
        );
        assert_eq!(buffer.language, "sql");
        assert_eq!(restored.text(&id).unwrap(), "select 1;");
    }

    #[test]
    fn an_orphaned_snapshot_is_cleaned_up_on_the_next_launch() {
        let dir = tempfile::tempdir().unwrap();
        {
            let (workspace, _) = Workspace::open(dir.path()).unwrap();
            workspace.store().write_buffer("leftover", "junk").unwrap();
            workspace.persist().unwrap();
        }
        let (workspace, _) = Workspace::open(dir.path()).unwrap();
        assert_eq!(workspace.store().read_buffer("leftover").unwrap(), "");
    }
}
