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
pub mod settings;
pub mod store;
pub mod update;

pub use language::detect as detect_language;
pub use session::{Buffer, ClosedTab, Session, SESSION_VERSION};
pub use settings::Settings;
pub use store::{new_id, Store, StoreError};

use std::fs;
use std::path::Path;

/// Largest file that may be opened, unless the settings file says otherwise.
///
/// Re-exported for convenience; the authority is [`settings::Settings`].
pub use settings::DEFAULT_MAX_OPEN_BYTES as MAX_OPEN_BYTES;

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
    settings: Settings,
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
        let (settings, settings_note) = Settings::load(&store);
        let mut workspace = Self {
            store,
            session,
            settings,
        };

        // ⚠️ Do NOT simply prune here. `load_session` yields an empty session
        // for a file that is corrupt OR missing, and pruning against an empty
        // session deletes every snapshot in the store — which is to say, all
        // of the user's scratch notes, precisely when something has already
        // gone wrong. Closing a tab deletes its snapshot explicitly, so in
        // normal operation there are no orphans at all; one existing means a
        // crash or outside interference, and the text in it is worth more
        // than the tidiness of the directory.
        let recovered = workspace.recover_orphans()?;
        // A settings problem is worth saying out loud too, but it must never
        // take precedence over a note about the notes themselves.
        let note = match (note, settings_note) {
            (Some(a), Some(b)) => Some(format!("{a}; {b}")),
            (Some(a), None) => Some(a),
            (None, b) => b,
        };
        let note = match (note, recovered) {
            (existing, 0) => existing,
            (Some(existing), n) => Some(format!(
                "{existing}; recovered {n} unreferenced {} as tabs",
                if n == 1 { "note" } else { "notes" }
            )),
            (None, n) => Some(format!(
                "Recovered {n} unreferenced {} as tabs",
                if n == 1 { "note" } else { "notes" }
            )),
        };

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

    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    /// Changes a setting and writes it out.
    pub fn update_settings(
        &mut self,
        change: impl FnOnce(&mut Settings),
    ) -> Result<(), StoreError> {
        change(&mut self.settings);
        self.settings.normalize();
        self.settings.save(&self.store)
    }

    /// A tab's text, from wherever the authoritative copy lives.
    ///
    /// A saved file with no unsaved edits is NOT duplicated into the store —
    /// the file on disk is already the copy, and snapshotting it would mean
    /// writing a second 512 MB on open and keeping it there. Such a tab is
    /// re-read from the file; everything else comes from its snapshot.
    fn contents(&self, buffer: &Buffer) -> Result<String, StoreError> {
        match (&buffer.path, buffer.dirty) {
            (Some(path), false) => {
                let bytes = read_capped(Path::new(path), self.settings.max_open_bytes)?;
                Ok(store::decode_file(bytes).text)
            }
            _ => self.store.read_buffer(&buffer.id),
        }
    }

    /// Turns snapshots the session does not reference back into tabs.
    ///
    /// Replaces the old "prune orphans" behaviour, which deleted them. That
    /// was safe only while `session.json` could be trusted — and it is exactly
    /// when it cannot (corrupt, truncated, deleted) that the snapshots are the
    /// only surviving copy of the user's notes. Recovering is the behaviour
    /// that matches what this app promises.
    ///
    /// Empty snapshots carry nothing, so those are deleted rather than
    /// restored as blank tabs; that is what keeps the directory from growing
    /// after a crash mid-`new_scratch`.
    ///
    /// Returns how many tabs were recovered.
    fn recover_orphans(&mut self) -> Result<usize, StoreError> {
        let mut orphans: Vec<String> = Vec::new();
        if let Ok(entries) = fs::read_dir(self.store.buffers_dir()) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) != Some("txt") {
                    continue;
                }
                let Some(id) = path.file_stem().and_then(|s| s.to_str()) else {
                    continue;
                };
                // The id becomes a path again on every read and write, so a
                // file someone dropped in here with an unusable name is left
                // alone rather than adopted.
                if !session::is_safe_id(id) || self.session.buffer(id).is_some() {
                    continue;
                }
                orphans.push(id.to_string());
            }
        }
        // Deterministic order, so a recovery looks the same twice running.
        orphans.sort();

        let mut recovered = 0;
        for id in orphans {
            let text = self.store.read_buffer(&id)?;
            if text.is_empty() {
                self.store.delete_buffer(&id)?;
                continue;
            }
            let title = self.session.next_untitled_title();
            let mut buffer = Buffer::untitled(&id, title);
            // It is unsaved text by definition — show it as such.
            buffer.dirty = true;
            self.session.buffers.push(buffer);
            recovered += 1;
        }

        if recovered > 0 {
            self.persist()?;
        }
        Ok(recovered)
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

    /// Reads a tab's text from wherever its authoritative copy lives.
    pub fn text(&self, id: &str) -> Result<String, StoreError> {
        match self.session.buffer(id) {
            Some(buffer) => self.contents(buffer),
            None => Ok(String::new()),
        }
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
        // Resolve before anything else. A path stored as given would mean
        // `notes.txt` reopening against a different directory after a restart
        // and Save overwriting the wrong file, and `/tmp/a.txt` and
        // `/tmp/./a.txt` opening as two tabs whose snapshots then compete.
        let resolved =
            fs::canonicalize(path.as_ref()).unwrap_or_else(|_| path.as_ref().to_path_buf());
        let path_ref = resolved.as_path();
        let path_string = path_ref.to_string_lossy().into_owned();

        if let Some(index) = self.session.index_of_path(&path_string) {
            self.session.active = index;
            // Still counts as using the file — without this, re-opening
            // something already on screen left it sinking down the recent list.
            self.session.remember_recent(&path_string);
            let buffer = self.session.buffers[index].clone();
            // `contents`, not the snapshot: a clean file-backed tab has none.
            // Any unsaved edits in that tab win over what is on disk, which is
            // what the user expects when re-opening a file they are editing.
            let text = self.contents(&buffer)?;
            self.persist()?;
            return Ok((buffer.id, text));
        }

        // Read through ONE handle and re-check its metadata from that handle.
        // Checking the path and then reading the path resolves it twice, so a
        // file that is swapped for a symlink to /dev/zero in between would be
        // read unbounded despite having passed the check.
        let bytes = read_capped(path_ref, self.settings.max_open_bytes)?;
        let decoded = store::decode_file(bytes);
        let text = decoded.text;

        let id = new_id();
        let mut buffer = Buffer::for_path(&id, &path_string);
        buffer.eol = decoded.eol.to_string();
        buffer.lossy = decoded.lossy;
        self.session.buffers.push(buffer);
        self.session.active = self.session.buffers.len() - 1;
        self.session.remember_recent(&path_string);
        // Deliberately NO snapshot here. The file on disk is already the copy,
        // so writing one would mean a second 512 MB on open and 512 MB sitting
        // in the store for a file the user has not even edited. A snapshot
        // appears the moment it is edited, and `contents` re-reads the file
        // until then.
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

        // Refuse to overwrite a file we could not read properly. The bytes we
        // did not understand became replacement characters, and writing those
        // back would destroy them for good. Saving ELSEWHERE is still allowed,
        // so the user can rescue what we did manage to read.
        // Cloned so the buffer can be consulted while `contents` borrows self.
        let buffer = self.session.buffer(id).cloned();
        let lossy = buffer.as_ref().map(|b| b.lossy).unwrap_or(false);
        let original = buffer.as_ref().and_then(|b| b.path.clone());
        if lossy && original.as_deref() == Some(target.as_str()) {
            return Err(StoreError::Io {
                context: format!(
                    "{target} is not UTF-8 and parts of it could not be read, so saving \
                     over it would destroy them — use Save As to write a copy instead"
                ),
                source: std::io::Error::new(std::io::ErrorKind::InvalidData, "lossy decode"),
            });
        }

        let eol = buffer
            .as_ref()
            .map(|b| b.eol.clone())
            .unwrap_or_else(|| "lf".to_string());
        // Through `contents`, NOT the snapshot. A saved file with no unsaved
        // edits has no snapshot at all now, and reading one would write an
        // empty file over the user's data — which a test caught doing exactly
        // that.
        let text = match &buffer {
            Some(b) => self.contents(b)?,
            None => self.store.read_buffer(id)?,
        };
        // Put the file's own line endings back. The editor normalises to LF,
        // so without this a one-word change to a Windows file rewrites every
        // line in it.
        let on_disk = store::encode_eol(&text, &eol);
        // The user's own file gets the same atomic treatment as the store: an
        // interrupted save must not truncate a file they already had.
        let target_path = Path::new(&target);
        write_user_file(target_path, on_disk.as_bytes())?;

        if let Some(buffer) = self.session.buffer_mut(id) {
            buffer.path = Some(target.clone());
            buffer.title = language::base_name(&target).to_string();
            buffer.language = language::detect(&target).to_string();
            buffer.dirty = false;
        }
        self.session.remember_recent(&target);
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
        // Through `close`, so every tab lands in the closed history and Close
        // All is recoverable one tab at a time rather than being final. The
        // confirmation is about clearing the board, not about destroying the
        // text for good.
        let closed = self.close_each(ids)?;
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

    /// Reopens the most recently closed tab, text and all.
    ///
    /// Returns its id, or `None` when nothing has been closed.
    pub fn restore_last_closed(&mut self) -> Result<Option<String>, StoreError> {
        let Some(closed) = self.session.closed.first().cloned() else {
            return Ok(None);
        };
        // An id that is somehow live again must not be clobbered.
        if self.session.buffer(&closed.id).is_some() {
            self.session.closed.remove(0);
            return Ok(None);
        }
        self.session.closed.remove(0);

        let restored_snapshot = if closed.has_snapshot {
            self.store.revive_buffer(&closed.id)?
        } else {
            false
        };

        let mut buffer = match &closed.path {
            Some(path) => Buffer::for_path(&closed.id, path),
            None => Buffer::untitled(&closed.id, closed.title.clone()),
        };
        buffer.language = closed.language.clone();
        buffer.eol = closed.eol.clone();
        buffer.dirty = restored_snapshot;
        if buffer.title.trim().is_empty() {
            buffer.title = self.session.next_untitled_title();
        }

        self.session.buffers.push(buffer);
        self.session.active = self.session.buffers.len() - 1;
        self.persist()?;
        Ok(Some(closed.id))
    }

    /// Discards the closed-tab history and every snapshot it holds.
    pub fn forget_closed_history(&mut self) -> Result<usize, StoreError> {
        let count = self.session.closed.len();
        for closed in std::mem::take(&mut self.session.closed) {
            self.store.forget_closed(&closed.id)?;
        }
        self.persist()?;
        Ok(count)
    }

    /// Re-reads a tab from the file on disk, discarding unsaved edits.
    pub fn reload(&mut self, id: &str) -> Result<String, StoreError> {
        let Some(path) = self.session.buffer(id).and_then(|b| b.path.clone()) else {
            return Err(StoreError::Io {
                context: format!("tab {id} has no file to reload from"),
                source: std::io::Error::new(std::io::ErrorKind::InvalidInput, "no path"),
            });
        };
        let bytes = read_capped(Path::new(&path), self.settings.max_open_bytes)?;
        let decoded = store::decode_file(bytes);
        // The snapshot is what made it dirty; dropping it makes the file
        // authoritative again.
        self.store.delete_buffer(id)?;
        if let Some(buffer) = self.session.buffer_mut(id) {
            buffer.dirty = false;
            buffer.eol = decoded.eol.to_string();
            buffer.lossy = decoded.lossy;
        }
        self.persist()?;
        Ok(decoded.text)
    }

    /// Renames the file a tab is backed by, on disk and in the tab.
    pub fn rename(&mut self, id: &str, new_path: &str) -> Result<(), StoreError> {
        let Some(old) = self.session.buffer(id).and_then(|b| b.path.clone()) else {
            return Err(StoreError::Io {
                context: format!("tab {id} has never been saved, so there is nothing to rename"),
                source: std::io::Error::new(std::io::ErrorKind::InvalidInput, "no path"),
            });
        };
        if Path::new(new_path).exists() {
            return Err(StoreError::Io {
                context: format!("{new_path} already exists"),
                source: std::io::Error::new(std::io::ErrorKind::AlreadyExists, "exists"),
            });
        }
        fs::rename(&old, new_path).map_err(|source| StoreError::Io {
            context: format!("renaming {old} to {new_path}"),
            source,
        })?;
        if let Some(buffer) = self.session.buffer_mut(id) {
            buffer.path = Some(new_path.to_string());
            buffer.title = language::base_name(new_path).to_string();
            buffer.language = language::detect(new_path).to_string();
        }
        self.session.remember_recent(new_path);
        self.persist()?;
        Ok(())
    }

    /// Writes a tab's text somewhere else without the tab adopting that path.
    pub fn save_copy(&self, id: &str, path: &str) -> Result<(), StoreError> {
        let Some(buffer) = self.session.buffer(id) else {
            return Err(StoreError::Io {
                context: format!("tab {id} is not open"),
                source: std::io::Error::new(std::io::ErrorKind::NotFound, "no such tab"),
            });
        };
        let text = self.contents(buffer)?;
        write_user_file(
            Path::new(path),
            store::encode_eol(&text, &buffer.eol).as_bytes(),
        )
    }

    /// Closes every tab except one. Returns how many were closed.
    pub fn close_others(&mut self, keep: &str) -> Result<usize, StoreError> {
        let doomed: Vec<String> = self
            .session
            .buffers
            .iter()
            .filter(|b| b.id != keep)
            .map(|b| b.id.clone())
            .collect();
        self.close_each(doomed)
    }

    /// Closes every tab to the left of one.
    pub fn close_to_left(&mut self, from: &str) -> Result<usize, StoreError> {
        let Some(index) = self.session.index_of(from) else {
            return Ok(0);
        };
        let doomed: Vec<String> = self.session.buffers[..index]
            .iter()
            .map(|b| b.id.clone())
            .collect();
        self.close_each(doomed)
    }

    /// Closes every tab to the right of one.
    pub fn close_to_right(&mut self, from: &str) -> Result<usize, StoreError> {
        let Some(index) = self.session.index_of(from) else {
            return Ok(0);
        };
        let doomed: Vec<String> = self.session.buffers[index + 1..]
            .iter()
            .map(|b| b.id.clone())
            .collect();
        self.close_each(doomed)
    }

    fn close_each(&mut self, ids: Vec<String>) -> Result<usize, StoreError> {
        let mut closed = 0;
        for id in ids {
            self.close(&id)?;
            closed += 1;
        }
        Ok(closed)
    }

    /// Closes a tab. Never prompts — that is the point of the app.
    ///
    /// Closing the last tab leaves a fresh empty one, so there is always
    /// somewhere to type.
    pub fn close(&mut self, id: &str) -> Result<(), StoreError> {
        let Some(index) = self.session.index_of(id) else {
            return Ok(());
        };
        let buffer = self.session.buffers.remove(index);
        // Retire rather than delete, so Restore Recent Closed File has
        // something to bring back. A clean file-backed tab has no snapshot to
        // move; reopening it re-reads the file.
        let has_snapshot = self.store.retire_buffer(id)?;
        self.session.closed.insert(
            0,
            ClosedTab {
                id: buffer.id.clone(),
                title: buffer.title.clone(),
                path: buffer.path.clone(),
                language: buffer.language.clone(),
                eol: buffer.eol.clone(),
                has_snapshot,
            },
        );
        // Anything beyond the bound is forgotten properly, text and all.
        while self.session.closed.len() > session::CLOSED_HISTORY {
            if let Some(evicted) = self.session.closed.pop() {
                self.store.forget_closed(&evicted.id)?;
            }
        }

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

/// Atomic write for a file the user chose.
///
/// ⚠️ There is deliberately **no fall-back to a direct `fs::write`**. The
/// earlier version did that whenever anything went wrong, which meant a full
/// disk — the very case where the temporary write fails — ended by truncating
/// the user's existing file and then failing anyway. Destroying the old
/// contents is strictly worse than refusing to save. The temporary file is
/// created in the destination's own directory, so a rename can never need to
/// cross a filesystem and the fall-back bought nothing.
///
/// Permissions are decided **before** the data is written, not after: creating
/// with the umask default and chmod-ing afterwards leaves the contents of a
/// deliberately private file sitting in a world-readable temporary file, and a
/// crash in that window leaves it there.
fn write_user_file(path: &Path, data: &[u8]) -> Result<(), StoreError> {
    use std::io::Write as _;

    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let tmp = parent.join(format!(".bhn-save-{}.tmp", new_id()));

    // What the destination already is, if it exists. A rename takes the
    // temporary file's permissions, so they have to match from the start.
    let existing = fs::metadata(path).ok().map(|m| m.permissions());

    let write = (|| -> std::io::Result<()> {
        let mut file = match &existing {
            // Replacing a file: create the temp with the destination's mode so
            // a private file is never briefly readable by anyone else.
            Some(permissions) => open_with_permissions(&tmp, permissions)?,
            // A new file: the umask decides, as it would for any program.
            None => fs::File::create(&tmp)?,
        };
        file.write_all(data)?;
        file.sync_all()?;
        Ok(())
    })();

    if let Err(source) = write {
        let _ = fs::remove_file(&tmp);
        return Err(StoreError::Io {
            context: format!("saving {}", path.display()),
            source,
        });
    }

    if let Err(source) = fs::rename(&tmp, path) {
        let _ = fs::remove_file(&tmp);
        return Err(StoreError::Io {
            context: format!("saving {}", path.display()),
            source,
        });
    }
    sync_parent_dir(path);
    Ok(())
}

/// Reads a file, refusing anything that is not a regular file or is larger
/// than `limit`.
///
/// The checks are made against the open handle, not the path, so the file
/// cannot be swapped for something else between the check and the read. The
/// read itself is capped at `limit + 1` bytes, which also catches a file that
/// grows while it is being read.
fn read_capped(path: &Path, limit: u64) -> Result<Vec<u8>, StoreError> {
    use std::io::Read as _;

    let fail = |context: String, kind: std::io::ErrorKind, message: &str| StoreError::Io {
        context,
        source: std::io::Error::new(kind, message.to_string()),
    };

    // A path-level check first, so a FIFO is rejected before `open` blocks on
    // it waiting for a writer. The handle checks below are what actually
    // close the race.
    let probe = fs::metadata(path).map_err(|source| StoreError::Io {
        context: format!("opening {}", path.display()),
        source,
    })?;
    if !probe.is_file() {
        return Err(fail(
            format!("{} is not a regular file", path.display()),
            std::io::ErrorKind::InvalidInput,
            "not a regular file",
        ));
    }

    let file = fs::File::open(path).map_err(|source| StoreError::Io {
        context: format!("opening {}", path.display()),
        source,
    })?;
    let meta = file.metadata().map_err(|source| StoreError::Io {
        context: format!("opening {}", path.display()),
        source,
    })?;
    if !meta.is_file() {
        return Err(fail(
            format!("{} is not a regular file", path.display()),
            std::io::ErrorKind::InvalidInput,
            "not a regular file",
        ));
    }
    if meta.len() > limit {
        return Err(fail(
            format!(
                "{} is {} and the limit is {} — raise max_open_bytes in settings.json \
                 if you need more, but very large files may not open at all",
                path.display(),
                human_bytes(meta.len()),
                human_bytes(limit)
            ),
            std::io::ErrorKind::InvalidInput,
            "file too large",
        ));
    }

    let mut bytes = Vec::with_capacity(meta.len() as usize + 1);
    file.take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|source| StoreError::Io {
            context: format!("reading {}", path.display()),
            source,
        })?;

    if bytes.len() as u64 > limit {
        return Err(fail(
            format!(
                "{} grew past the {} limit while it was being read",
                path.display(),
                human_bytes(limit)
            ),
            std::io::ErrorKind::InvalidInput,
            "file too large",
        ));
    }
    Ok(bytes)
}

/// Flushes the directory entry itself, so the rename survives a power loss.
///
/// `sync_all` on the file commits its contents; it says nothing about the
/// directory entry that names it. Without this, a real power cut (as opposed
/// to the process being killed, which is all the test suite can simulate) can
/// leave the rename unrecorded. Best effort: not every filesystem supports it,
/// and failing to sync a directory is not a reason to report the save failed.
fn sync_parent_dir(path: &Path) {
    #[cfg(unix)]
    {
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        if let Ok(dir) = fs::File::open(parent) {
            let _ = dir.sync_all();
        }
    }
    #[cfg(not(unix))]
    {
        // Windows does not allow opening a directory as a file this way.
        let _ = path;
    }
}

/// Creates a file with exactly the given permissions from the outset.
fn open_with_permissions(path: &Path, permissions: &fs::Permissions) -> std::io::Result<fs::File> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(permissions.mode() & 0o777)
            .open(path)
    }
    #[cfg(not(unix))]
    {
        // Windows permissions are ACLs, which `fs::Permissions` does not carry
        // beyond the read-only flag; the containing directory governs the rest.
        let file = fs::File::create(path)?;
        let _ = fs::set_permissions(path, permissions.clone());
        Ok(file)
    }
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

    // ── Restore Recent Closed File ────────────────────────────────────────

    #[test]
    fn a_closed_scratch_tab_can_be_brought_back_with_its_text() {
        let (_dir, mut workspace) = workspace();
        let id = workspace.new_scratch().unwrap();
        workspace
            .update_text(&id, "the note I closed by accident")
            .unwrap();
        workspace.close(&id).unwrap();
        assert!(workspace.session().buffer(&id).is_none());

        let restored = workspace.restore_last_closed().unwrap().unwrap();

        assert_eq!(restored, id);
        assert_eq!(
            workspace.text(&id).unwrap(),
            "the note I closed by accident"
        );
        assert!(workspace.session().buffer(&id).unwrap().dirty);
    }

    #[test]
    fn a_closed_file_tab_comes_back_reading_the_file() {
        let (dir, mut workspace) = workspace();
        let file = dir.path().join("notes.sql");
        fs::write(&file, "select 1;\n").unwrap();
        let (id, _) = workspace.open_path(&file).unwrap();
        workspace.close(&id).unwrap();

        let restored = workspace.restore_last_closed().unwrap().unwrap();
        assert_eq!(restored, id);
        let buffer = workspace.session().buffer(&id).unwrap();
        let canonical = fs::canonicalize(&file).unwrap();
        assert_eq!(
            buffer.path.as_deref(),
            Some(canonical.to_string_lossy().as_ref())
        );
        assert_eq!(buffer.language, "sql");
        assert!(
            !buffer.dirty,
            "an unedited file is not dirty when it comes back"
        );
        assert_eq!(workspace.text(&id).unwrap(), "select 1;\n");
    }

    #[test]
    fn tabs_come_back_in_the_order_they_were_closed() {
        let (_dir, mut workspace) = workspace();
        let a = workspace.new_scratch().unwrap();
        workspace.update_text(&a, "first closed").unwrap();
        let b = workspace.new_scratch().unwrap();
        workspace.update_text(&b, "second closed").unwrap();
        workspace.close(&a).unwrap();
        workspace.close(&b).unwrap();

        assert_eq!(workspace.restore_last_closed().unwrap().unwrap(), b);
        assert_eq!(workspace.restore_last_closed().unwrap().unwrap(), a);
        assert!(workspace.restore_last_closed().unwrap().is_none());
    }

    #[test]
    fn the_closed_history_is_bounded_and_evicted_text_is_really_gone() {
        let (_dir, mut workspace) = workspace();
        let mut ids = Vec::new();
        for i in 0..(session::CLOSED_HISTORY + 3) {
            let id = workspace.new_scratch().unwrap();
            workspace.update_text(&id, &format!("note {i}")).unwrap();
            workspace.close(&id).unwrap();
            ids.push(id);
        }
        assert_eq!(workspace.session().closed.len(), session::CLOSED_HISTORY);

        // The oldest three were pushed out; their text must not linger.
        for id in &ids[..3] {
            assert_eq!(workspace.store().read_closed(id).unwrap(), "");
        }
        assert_eq!(
            workspace.store().read_closed(ids.last().unwrap()).unwrap(),
            format!("note {}", session::CLOSED_HISTORY + 2)
        );
    }

    #[test]
    fn close_all_is_recoverable_one_tab_at_a_time() {
        let (_dir, mut workspace) = workspace();
        let first = workspace.session().buffers[0].id.clone();
        workspace.update_text(&first, "one").unwrap();
        let second = workspace.new_scratch().unwrap();
        workspace.update_text(&second, "two").unwrap();

        workspace.close_all().unwrap();
        assert_eq!(workspace.session().buffers.len(), 1);

        workspace.restore_last_closed().unwrap();
        assert_eq!(workspace.text(&second).unwrap(), "two");
        workspace.restore_last_closed().unwrap();
        assert_eq!(workspace.text(&first).unwrap(), "one");
    }

    #[test]
    fn forgetting_the_history_removes_the_text_too() {
        let (_dir, mut workspace) = workspace();
        let id = workspace.new_scratch().unwrap();
        workspace.update_text(&id, "a secret I closed").unwrap();
        workspace.close(&id).unwrap();
        assert_eq!(
            workspace.store().read_closed(&id).unwrap(),
            "a secret I closed"
        );

        assert_eq!(workspace.forget_closed_history().unwrap(), 1);
        assert_eq!(workspace.store().read_closed(&id).unwrap(), "");
        assert!(workspace.restore_last_closed().unwrap().is_none());
    }

    #[test]
    fn a_restored_tab_survives_a_restart() {
        let dir = tempfile::tempdir().unwrap();
        let id;
        {
            let (mut workspace, _) = Workspace::open(dir.path()).unwrap();
            id = workspace.new_scratch().unwrap();
            workspace.update_text(&id, "closed then restored").unwrap();
            workspace.close(&id).unwrap();
            workspace.restore_last_closed().unwrap();
        }
        let (restored, _) = Workspace::open(dir.path()).unwrap();
        assert_eq!(restored.text(&id).unwrap(), "closed then restored");
    }

    // ── Reload, rename, save a copy, bulk closes ─────────────────────────

    #[test]
    fn reload_discards_unsaved_edits_and_takes_what_is_on_disk() {
        let (dir, mut workspace) = workspace();
        let file = dir.path().join("config.conf");
        fs::write(&file, "original\n").unwrap();
        let (id, _) = workspace.open_path(&file).unwrap();
        workspace.update_text(&id, "my unsaved edit\n").unwrap();
        fs::write(&file, "changed by another program\n").unwrap();

        let text = workspace.reload(&id).unwrap();

        assert_eq!(text, "changed by another program\n");
        assert_eq!(workspace.text(&id).unwrap(), "changed by another program\n");
        assert!(!workspace.session().buffer(&id).unwrap().dirty);
    }

    #[test]
    fn reload_refuses_a_tab_with_no_file() {
        let (_dir, mut workspace) = workspace();
        let id = workspace.session().buffers[0].id.clone();
        assert!(workspace.reload(&id).is_err());
    }

    #[test]
    fn rename_moves_the_file_and_relabels_the_tab() {
        let (dir, mut workspace) = workspace();
        let from = dir.path().join("old_name.txt");
        fs::write(&from, "body").unwrap();
        let (id, _) = workspace.open_path(&from).unwrap();

        let to = dir.path().join("new_name.sql");
        workspace.rename(&id, &to.to_string_lossy()).unwrap();

        assert!(!from.exists());
        assert_eq!(fs::read_to_string(&to).unwrap(), "body");
        let buffer = workspace.session().buffer(&id).unwrap();
        assert_eq!(buffer.title, "new_name.sql");
        assert_eq!(buffer.language, "sql");
    }

    #[test]
    fn rename_refuses_to_overwrite_an_existing_file() {
        let (dir, mut workspace) = workspace();
        let from = dir.path().join("a.txt");
        let occupied = dir.path().join("b.txt");
        fs::write(&from, "mine").unwrap();
        fs::write(&occupied, "someone else's").unwrap();
        let (id, _) = workspace.open_path(&from).unwrap();

        assert!(workspace.rename(&id, &occupied.to_string_lossy()).is_err());
        assert_eq!(fs::read_to_string(&occupied).unwrap(), "someone else's");
        assert!(from.exists(), "and the original is still where it was");
    }

    #[test]
    fn save_a_copy_writes_elsewhere_without_moving_the_tab() {
        let (dir, mut workspace) = workspace();
        let original = dir.path().join("live.txt");
        fs::write(&original, "first").unwrap();
        let (id, _) = workspace.open_path(&original).unwrap();
        workspace.update_text(&id, "edited, not saved").unwrap();

        let copy = dir.path().join("copy.txt");
        workspace.save_copy(&id, &copy.to_string_lossy()).unwrap();

        assert_eq!(fs::read_to_string(&copy).unwrap(), "edited, not saved");
        assert_eq!(
            fs::read_to_string(&original).unwrap(),
            "first",
            "original untouched"
        );
        let buffer = workspace.session().buffer(&id).unwrap();
        let canonical = fs::canonicalize(&original).unwrap();
        assert_eq!(
            buffer.path.as_deref(),
            Some(canonical.to_string_lossy().as_ref())
        );
        assert!(buffer.dirty, "and the tab is still unsaved");
    }

    #[test]
    fn close_others_left_and_right_close_the_right_tabs() {
        let (_dir, mut workspace) = workspace();
        let a = workspace.session().buffers[0].id.clone();
        let b = workspace.new_scratch().unwrap();
        let c = workspace.new_scratch().unwrap();
        let d = workspace.new_scratch().unwrap();

        assert_eq!(workspace.close_to_left(&c).unwrap(), 2);
        let left: Vec<String> = workspace
            .session()
            .buffers
            .iter()
            .map(|x| x.id.clone())
            .collect();
        assert_eq!(left, vec![c.clone(), d.clone()]);
        assert!(!left.contains(&a) && !left.contains(&b));

        assert_eq!(workspace.close_to_right(&c).unwrap(), 1);
        assert_eq!(workspace.session().buffers.len(), 1);

        let e = workspace.new_scratch().unwrap();
        workspace.new_scratch().unwrap();
        assert_eq!(workspace.close_others(&e).unwrap(), 2);
        assert_eq!(workspace.session().buffers.len(), 1);
        assert_eq!(workspace.session().buffers[0].id, e);
    }

    // ── Recent files ──────────────────────────────────────────────────────

    #[test]
    fn opening_and_saving_build_the_recent_list_newest_first() {
        let (dir, mut workspace) = workspace();
        for name in ["one.txt", "two.txt", "three.txt"] {
            let path = dir.path().join(name);
            fs::write(&path, "x").unwrap();
            workspace.open_path(&path).unwrap();
        }
        let recent = &workspace.session().recent;
        assert!(recent[0].ends_with("three.txt"), "newest first: {recent:?}");
        assert!(recent[2].ends_with("one.txt"));

        // Re-opening moves it to the front rather than duplicating it.
        workspace.open_path(dir.path().join("one.txt")).unwrap();
        let recent = &workspace.session().recent;
        assert!(recent[0].ends_with("one.txt"));
        assert_eq!(recent.len(), 3, "no duplicate: {recent:?}");
    }

    #[test]
    fn the_recent_list_is_bounded() {
        let (dir, mut workspace) = workspace();
        for i in 0..(session::RECENT_FILES + 5) {
            let path = dir.path().join(format!("f{i}.txt"));
            fs::write(&path, "x").unwrap();
            workspace.open_path(&path).unwrap();
        }
        assert_eq!(workspace.session().recent.len(), session::RECENT_FILES);
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
    fn a_windows_file_keeps_its_line_endings_through_a_save() {
        let (dir, mut workspace) = workspace();
        let file = dir.path().join("windows.txt");
        fs::write(&file, "alpha\r\nbeta\r\ngamma\r\n").unwrap();

        let (id, text) = workspace.open_path(&file).unwrap();
        assert_eq!(text, "alpha\nbeta\ngamma\n", "normalised for the editor");
        assert_eq!(workspace.session().buffer(&id).unwrap().eol, "crlf");

        workspace.update_text(&id, "alpha\nBETA\ngamma\n").unwrap();
        workspace.save(&id, None).unwrap();

        assert_eq!(
            fs::read_to_string(&file).unwrap(),
            "alpha\r\nBETA\r\ngamma\r\n",
            "a one-word change must not rewrite every line ending"
        );
    }

    #[test]
    fn a_unix_file_is_not_given_windows_line_endings() {
        let (dir, mut workspace) = workspace();
        let file = dir.path().join("unix.txt");
        fs::write(&file, "one\ntwo\n").unwrap();
        let (id, _) = workspace.open_path(&file).unwrap();
        assert_eq!(workspace.session().buffer(&id).unwrap().eol, "lf");
        workspace.update_text(&id, "one\ntwo\nthree\n").unwrap();
        workspace.save(&id, None).unwrap();
        assert_eq!(fs::read_to_string(&file).unwrap(), "one\ntwo\nthree\n");
    }

    #[test]
    fn a_file_that_could_not_be_decoded_is_not_overwritten() {
        let (dir, mut workspace) = workspace();
        let file = dir.path().join("latin1.txt");
        // 0xFF is not valid UTF-8; decoding replaces it and the byte is lost.
        fs::write(&file, b"caf\xE9 nota\xFFion").unwrap();

        let (id, _) = workspace.open_path(&file).unwrap();
        assert!(workspace.session().buffer(&id).unwrap().lossy);

        let error = workspace.save(&id, None).unwrap_err().to_string();
        assert!(
            error.contains("Save As"),
            "the message should say what to do: {error}"
        );
        assert_eq!(
            fs::read(&file).unwrap(),
            b"caf\xE9 nota\xFFion",
            "the original bytes must be untouched"
        );

        // Save As elsewhere is allowed — that is how the user rescues it.
        let copy = dir.path().join("rescued.txt");
        assert!(workspace.save(&id, Some(&copy.to_string_lossy())).is_ok());
        assert!(copy.exists());
    }

    #[test]
    fn the_same_file_reached_by_two_spellings_opens_one_tab() {
        let (dir, mut workspace) = workspace();
        let file = dir.path().join("once.txt");
        fs::write(&file, "body").unwrap();
        let awkward = dir.path().join(".").join("once.txt");

        let (first, _) = workspace.open_path(&file).unwrap();
        let count = workspace.session().buffers.len();
        let (second, _) = workspace.open_path(&awkward).unwrap();

        assert_eq!(first, second, "`./` must not produce a rival tab");
        assert_eq!(workspace.session().buffers.len(), count);
    }

    #[test]
    fn an_opened_files_path_is_stored_absolute() {
        let (dir, mut workspace) = workspace();
        let file = dir.path().join("rel.txt");
        fs::write(&file, "x").unwrap();
        let (id, _) = workspace.open_path(&file).unwrap();
        let stored = workspace
            .session()
            .buffer(&id)
            .unwrap()
            .path
            .clone()
            .unwrap();
        assert!(
            Path::new(&stored).is_absolute(),
            "a relative path would reopen against the wrong directory: {stored}"
        );
    }

    #[test]
    fn a_failed_save_leaves_the_original_file_intact() {
        let (dir, mut workspace) = workspace();
        let id = workspace.session().buffers[0].id.clone();
        workspace.update_text(&id, "new content").unwrap();

        // A directory cannot be replaced by a file, so the rename fails — and
        // the old code's fall-back would have truncated whatever was there.
        let blocked = dir.path().join("in_the_way");
        fs::create_dir(&blocked).unwrap();
        fs::write(blocked.join("keep.txt"), "precious").unwrap();

        assert!(workspace
            .save(&id, Some(&blocked.to_string_lossy()))
            .is_err());
        assert!(blocked.is_dir(), "the directory must still be a directory");
        assert_eq!(
            fs::read_to_string(blocked.join("keep.txt")).unwrap(),
            "precious"
        );

        let leftovers: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.contains(".tmp"))
            .collect();
        assert!(
            leftovers.is_empty(),
            "and no temp file left behind: {leftovers:?}"
        );
    }

    #[test]
    fn an_opened_file_is_not_copied_into_the_store() {
        // Opening a 512 MB dump used to write a second 512 MB into the store
        // for a file the user had not even edited.
        let (dir, mut workspace) = workspace();
        let file = dir.path().join("big.sql");
        fs::write(&file, "select 1;\nselect 2;\n").unwrap();

        let (id, text) = workspace.open_path(&file).unwrap();
        assert_eq!(text, "select 1;\nselect 2;\n");
        assert!(
            !workspace
                .store()
                .buffers_dir()
                .join(format!("{id}.txt"))
                .exists(),
            "a clean file-backed tab should have no snapshot at all"
        );
        // And it still reads back, by going to the file.
        assert_eq!(workspace.text(&id).unwrap(), "select 1;\nselect 2;\n");
    }

    #[test]
    fn editing_a_file_backed_tab_does_create_a_snapshot() {
        let (dir, mut workspace) = workspace();
        let file = dir.path().join("edited.sql");
        fs::write(&file, "original\n").unwrap();
        let (id, _) = workspace.open_path(&file).unwrap();

        workspace
            .update_text(&id, "edited but not saved\n")
            .unwrap();

        assert!(workspace
            .store()
            .buffers_dir()
            .join(format!("{id}.txt"))
            .exists());
        assert_eq!(workspace.text(&id).unwrap(), "edited but not saved\n");
        assert_eq!(
            fs::read_to_string(&file).unwrap(),
            "original\n",
            "the file itself is untouched until Save"
        );
    }

    #[test]
    fn unsaved_edits_to_a_file_survive_a_restart() {
        // The case the no-snapshot optimisation must not break: edits that
        // were never saved still have to come back.
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("wip.sql");
        fs::write(&file, "original\n").unwrap();
        let id;
        {
            let (mut workspace, _) = Workspace::open(dir.path()).unwrap();
            let (opened, _) = workspace.open_path(&file).unwrap();
            id = opened;
            workspace.update_text(&id, "half-finished edit\n").unwrap();
        }
        let (restored, _) = Workspace::open(dir.path()).unwrap();
        assert_eq!(restored.text(&id).unwrap(), "half-finished edit\n");
        assert!(restored.session().buffer(&id).unwrap().dirty);
    }

    #[test]
    fn a_clean_file_tab_shows_what_is_on_disk_now_after_a_restart() {
        // The flip side: an unedited tab re-reads the file, so a change made
        // elsewhere is visible rather than a stale copy being shown.
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("changes.sql");
        fs::write(&file, "before\n").unwrap();
        let id;
        {
            let (mut workspace, _) = Workspace::open(dir.path()).unwrap();
            let (opened, _) = workspace.open_path(&file).unwrap();
            id = opened;
        }
        fs::write(&file, "changed by something else\n").unwrap();

        let (restored, _) = Workspace::open(dir.path()).unwrap();
        assert_eq!(restored.text(&id).unwrap(), "changed by something else\n");
    }

    #[test]
    fn saving_a_clean_file_tab_does_not_blank_it() {
        // With no snapshot to read, a careless save path would write "".
        let (dir, mut workspace) = workspace();
        let file = dir.path().join("intact.sql");
        fs::write(&file, "important contents\n").unwrap();
        let (id, _) = workspace.open_path(&file).unwrap();

        workspace.save(&id, None).unwrap();
        assert_eq!(fs::read_to_string(&file).unwrap(), "important contents\n");
    }

    #[test]
    fn the_open_limit_comes_from_the_settings_file() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path()).unwrap();
        {
            let store = Store::new(dir.path()).unwrap();
            fs::write(
                store.root().join("settings.json"),
                r#"{"max_open_bytes": 2097152}"#,
            )
            .unwrap();
        }
        let (mut workspace, _) = Workspace::open(dir.path()).unwrap();
        assert_eq!(workspace.settings().max_open_bytes, 2 * 1024 * 1024);

        let too_big = dir.path().join("over.bin");
        let file = fs::File::create(&too_big).unwrap();
        file.set_len(3 * 1024 * 1024).unwrap();
        drop(file);
        let error = workspace.open_path(&too_big).unwrap_err().to_string();
        assert!(error.contains("limit"), "got: {error}");

        let fine = dir.path().join("under.bin");
        let file = fs::File::create(&fine).unwrap();
        file.set_len(1024 * 1024).unwrap();
        drop(file);
        assert!(workspace.open_path(&fine).is_ok());
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
    fn an_orphan_with_text_is_recovered_as_a_tab_not_deleted() {
        // This assertion used to be the opposite — orphans were pruned. That
        // was wrong: an orphan means something already went wrong, and the
        // snapshot can be the only surviving copy of a note. Inverted rather
        // than deleted so the new promise is the thing now protected.
        let dir = tempfile::tempdir().unwrap();
        {
            let (workspace, _) = Workspace::open(dir.path()).unwrap();
            workspace.store().write_buffer("leftover", "junk").unwrap();
            workspace.persist().unwrap();
        }
        let (workspace, note) = Workspace::open(dir.path()).unwrap();
        assert_eq!(workspace.store().read_buffer("leftover").unwrap(), "junk");
        assert!(
            workspace.session().buffer("leftover").is_some(),
            "recovered as a tab"
        );
        assert!(workspace.session().buffer("leftover").unwrap().dirty);
        assert!(note.unwrap().contains("Recovered 1"));
    }

    #[test]
    fn an_empty_orphan_is_discarded_rather_than_opened_as_a_blank_tab() {
        let dir = tempfile::tempdir().unwrap();
        {
            let (workspace, _) = Workspace::open(dir.path()).unwrap();
            workspace.store().write_buffer("blank", "").unwrap();
            workspace.persist().unwrap();
        }
        let (workspace, _) = Workspace::open(dir.path()).unwrap();
        assert!(workspace.session().buffer("blank").is_none());
        assert!(!workspace.store().buffers_dir().join("blank.txt").exists());
    }

    // ── The notes must survive a broken session file ──────────────────────

    #[test]
    fn a_corrupt_session_file_does_not_destroy_the_notes() {
        // Previously: a corrupt session.json produced an empty session, and
        // pruning against it deleted every snapshot in the store. The one
        // moment the notes mattered most was the moment they were erased.
        let dir = tempfile::tempdir().unwrap();
        let ids: Vec<String>;
        {
            let (mut workspace, _) = Workspace::open(dir.path()).unwrap();
            let first = workspace.session().buffers[0].id.clone();
            workspace
                .update_text(&first, "server password reminder")
                .unwrap();
            let second = workspace.new_scratch().unwrap();
            workspace.update_text(&second, "the other note").unwrap();
            ids = vec![first, second];
        }
        fs::write(dir.path().join("session.json"), "{ truncated").unwrap();

        let (restored, note) = Workspace::open(dir.path()).unwrap();

        let texts: Vec<String> = ids.iter().map(|id| restored.text(id).unwrap()).collect();
        assert!(
            texts.contains(&"server password reminder".to_string())
                && texts.contains(&"the other note".to_string()),
            "both notes should still be readable, got {texts:?}"
        );
        assert_eq!(
            restored.session().buffers.len(),
            2,
            "and both reopened as tabs"
        );
        assert!(
            note.unwrap().contains("recovered"),
            "the user should be told"
        );
    }

    #[test]
    fn a_deleted_session_file_does_not_destroy_the_notes() {
        // Same hazard by a different route: load_session returns an empty
        // session for a MISSING file too, which a prune would act on.
        let dir = tempfile::tempdir().unwrap();
        let id;
        {
            let (mut workspace, _) = Workspace::open(dir.path()).unwrap();
            id = workspace.session().buffers[0].id.clone();
            workspace.update_text(&id, "still here").unwrap();
        }
        fs::remove_file(dir.path().join("session.json")).unwrap();

        let (restored, _) = Workspace::open(dir.path()).unwrap();
        assert_eq!(restored.text(&id).unwrap(), "still here");
        assert_eq!(restored.session().buffers.len(), 1);
    }

    #[test]
    fn recovery_ignores_files_it_did_not_write() {
        let dir = tempfile::tempdir().unwrap();
        let (workspace, _) = Workspace::open(dir.path()).unwrap();
        let stray = workspace.store().buffers_dir().join("not a safe id.txt");
        fs::write(&stray, "someone else's file").unwrap();
        drop(workspace);

        let (restored, _) = Workspace::open(dir.path()).unwrap();
        assert_eq!(restored.session().buffers.len(), 1, "not adopted as a tab");
        assert!(stray.exists(), "and not deleted either");
    }
}
