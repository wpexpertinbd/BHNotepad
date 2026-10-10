//! On-disk session store: `session.json` plus one text snapshot per tab.
//!
//! ```text
//! <root>/session.json          which tabs are open, caret positions
//! <root>/buffers/<id>.txt      the text of each tab, saved or not
//! ```
//!
//! The config directory is passed in by the shell — this core never asks the
//! operating system where anything lives, which is what lets it port to
//! Windows and Linux unchanged. See CLAUDE.md.
//!
//! **Every write is atomic** (temp file, fsync, rename). A snapshot truncated
//! by a crash mid-write would destroy exactly the scratch note this app exists
//! to protect, and `rename` is the one operation all three platforms make
//! atomic. `fs::rename` replaces the destination on Windows too.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::session::{is_safe_id, Session};

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("{context}: {source}")]
    Io {
        context: String,
        #[source]
        source: std::io::Error,
    },
    /// The id came from a session file or an IPC call; it must never be used
    /// to build a path before being checked.
    #[error("unsafe buffer id: {0:?}")]
    UnsafeId(String),
}

type Result<T> = std::result::Result<T, StoreError>;

fn io(context: impl Into<String>) -> impl FnOnce(std::io::Error) -> StoreError {
    move |source| StoreError::Io {
        context: context.into(),
        source,
    }
}

/// Generates a short, unique, filename-safe id.
///
/// Dependency-free on purpose: the counter makes ids unique within a run even
/// if the clock stands still or steps backwards, and the timestamp keeps them
/// unique across runs.
pub fn new_id() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    format!("{}{}", base36(nanos), base36(seq))
}

fn base36(mut value: u64) -> String {
    const ALPHABET: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    if value == 0 {
        return "0".to_string();
    }
    let mut out = Vec::new();
    while value > 0 {
        out.push(ALPHABET[(value % 36) as usize]);
        value /= 36;
    }
    out.reverse();
    String::from_utf8(out).expect("base36 alphabet is ASCII")
}

/// Creates a file only this user can read.
///
/// Scratch notes are whatever the user typed and never had to name — exactly
/// the place a password or a token gets pasted "just for a second". The
/// default umask would make them 0644 and readable by every other account on
/// the machine, so the mode is set at creation rather than afterwards: a
/// chmod after the write leaves a window in which the file is world-readable.
///
/// Windows has no mode bits; `%APPDATA%` is already restricted to the user by
/// its ACL, so plain creation is correct there.
fn create_private(path: &Path) -> std::io::Result<fs::File> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)
    }
    #[cfg(not(unix))]
    {
        fs::File::create(path)
    }
}

/// Restricts a directory to this user. Unix only; see `create_private`.
fn make_private_dir(path: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
    Ok(())
}

/// Writes `data` to `path` so that a reader sees either the old file or the
/// new one, never a half-written one.
fn write_atomic(path: &Path, data: &[u8]) -> Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(io(format!("creating {}", parent.display())))?;

    let tmp = parent.join(format!(".bhn-{}.tmp", new_id()));
    let write_result = (|| -> std::io::Result<()> {
        let mut file = create_private(&tmp)?;
        file.write_all(data)?;
        // Without this the rename can land before the bytes do, so a power
        // loss leaves a correctly-named empty file -- worse than no file.
        file.sync_all()?;
        Ok(())
    })();

    if let Err(source) = write_result {
        let _ = fs::remove_file(&tmp);
        return Err(StoreError::Io {
            context: format!("writing {}", tmp.display()),
            source,
        });
    }

    if let Err(source) = fs::rename(&tmp, path) {
        let _ = fs::remove_file(&tmp);
        return Err(StoreError::Io {
            context: format!("replacing {}", path.display()),
            source,
        });
    }

    // `sync_all` on the file commits its contents; it says nothing about the
    // directory entry that now names it. Without this a real power cut — as
    // opposed to the process being killed, which is all a test can simulate —
    // can lose the rename and with it the note. Best effort: not every
    // filesystem supports it, and that is not a reason to report a failure.
    #[cfg(unix)]
    if let Ok(dir) = fs::File::open(parent) {
        let _ = dir.sync_all();
    }
    Ok(())
}

/// The session store rooted at one directory.
#[derive(Debug, Clone)]
pub struct Store {
    root: PathBuf,
}

impl Store {
    /// Creates the store directories if they do not exist.
    pub fn new(root: impl Into<PathBuf>) -> Result<Self> {
        let root = root.into();
        let store = Self { root };
        fs::create_dir_all(store.buffers_dir())
            .map_err(io(format!("creating {}", store.buffers_dir().display())))?;
        // Tighten both levels, including on a store created by an older build
        // that left them world-readable.
        let _ = make_private_dir(store.root());
        let _ = make_private_dir(&store.buffers_dir());
        Ok(store)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn session_path(&self) -> PathBuf {
        self.root.join("session.json")
    }

    pub fn buffers_dir(&self) -> PathBuf {
        self.root.join("buffers")
    }

    fn snapshot_path(&self, id: &str) -> Result<PathBuf> {
        if !is_safe_id(id) {
            return Err(StoreError::UnsafeId(id.to_string()));
        }
        Ok(self.buffers_dir().join(format!("{id}.txt")))
    }

    /// Loads the session, repairing it as needed.
    ///
    /// **Never fails.** A missing file means a first run; a corrupt one means
    /// something went wrong that must not stop the app from starting. In both
    /// cases the caller gets an empty session and the reason, for logging.
    pub fn load_session(&self) -> (Session, Option<String>) {
        let path = self.session_path();
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return (Session::default(), None);
            }
            Err(error) => {
                return (
                    Session::default(),
                    Some(format!("could not read {}: {error}", path.display())),
                );
            }
        };

        match serde_json::from_str::<Session>(&text) {
            Ok(mut session) => {
                session.normalize();
                (session, None)
            }
            Err(error) => {
                // Keep the unreadable file instead of overwriting it, so the
                // text is still recoverable by hand. The snapshots themselves
                // are untouched and still on disk.
                let kept = self.root.join("session.corrupt.json");
                let note = match fs::rename(&path, &kept) {
                    Ok(()) => format!(
                        "session.json was unreadable ({error}); kept it as {}",
                        kept.display()
                    ),
                    Err(rename_error) => format!(
						"session.json was unreadable ({error}); could not set it aside: {rename_error}"
					),
                };
                (Session::default(), Some(note))
            }
        }
    }

    pub fn save_session(&self, session: &Session) -> Result<()> {
        // Pretty-printed: this file is small, and being able to read or repair
        // it by hand has already proved useful.
        let text = serde_json::to_string_pretty(session).expect("Session is always serializable");
        write_atomic(&self.session_path(), text.as_bytes())
    }

    /// Reads a tab's text. Returns an empty string when no snapshot exists yet.
    pub fn read_buffer(&self, id: &str) -> Result<String> {
        let path = self.snapshot_path(id)?;
        match fs::read(&path) {
            Ok(bytes) => Ok(decode_utf8_lossy(bytes)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
            Err(source) => Err(StoreError::Io {
                context: format!("reading {}", path.display()),
                source,
            }),
        }
    }

    pub fn write_buffer(&self, id: &str, text: &str) -> Result<()> {
        write_atomic(&self.snapshot_path(id)?, text.as_bytes())
    }

    /// Removes a tab's snapshot. Succeeds when it is already gone.
    pub fn delete_buffer(&self, id: &str) -> Result<()> {
        let path = self.snapshot_path(id)?;
        match fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(source) => Err(StoreError::Io {
                context: format!("removing {}", path.display()),
                source,
            }),
        }
    }

    /// Deletes snapshots no open tab refers to, and returns how many went.
    ///
    /// Without this the buffers directory grows for the life of the install.
    /// Only ever called with a session that has just been loaded or saved — a
    /// stale session argument would delete live scratch notes.
    pub fn prune_orphans(&self, session: &Session) -> usize {
        let dir = self.buffers_dir();
        let entries = match fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(_) => return 0,
        };

        let mut removed = 0;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("txt") {
                continue;
            }
            let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            if session.buffers.iter().any(|b| b.id == stem) {
                continue;
            }
            if fs::remove_file(&path).is_ok() {
                removed += 1;
            }
        }
        removed
    }
}

/// Turns bytes into a `String`, replacing anything that is not valid UTF-8 and
/// stripping a leading BOM.
///
/// A scratch store should only ever hold UTF-8 that this app wrote, but an
/// opened file can be anything — and refusing to show a log file because of
/// one bad byte would be worse than showing it with a replacement character.
/// What decoding a file produced, and what had to be given up to do it.
#[derive(Debug, Clone, PartialEq)]
pub struct Decoded {
    /// The text, with all line endings normalised to LF.
    pub text: String,
    /// True when bytes had to be replaced because they were not valid UTF-8.
    /// Saving over the original would then destroy them.
    pub lossy: bool,
    /// `"crlf"` when the file predominantly used Windows line endings, so the
    /// save path can put them back.
    pub eol: &'static str,
}

/// Decodes a file's bytes, recording what was normalised rather than doing it
/// silently.
///
/// The editor works in LF internally. Converting on the way in and back on the
/// way out is what keeps a one-word change to a Windows file from rewriting
/// every line in it.
pub fn decode_file(bytes: Vec<u8>) -> Decoded {
    let had_bom = bytes.starts_with(&[0xEF, 0xBB, 0xBF]);
    let body = if had_bom { bytes[3..].to_vec() } else { bytes };

    let (text, lossy) = match String::from_utf8(body) {
        Ok(text) => (text, false),
        Err(error) => (String::from_utf8_lossy(error.as_bytes()).into_owned(), true),
    };

    // "Predominantly" rather than "any": a file with mixed endings is usually
    // mostly one of them, and that is what the user expects to keep.
    let crlf = text.matches("\r\n").count();
    let bare_lf = text.matches('\n').count() - crlf;
    let eol = if crlf > bare_lf { "crlf" } else { "lf" };

    Decoded {
        text: text.replace("\r\n", "\n"),
        lossy,
        eol,
    }
}

/// Puts a buffer's original line endings back for writing to disk.
pub fn encode_eol(text: &str, eol: &str) -> String {
    if eol == "crlf" {
        text.replace('\n', "\r\n")
    } else {
        text.to_string()
    }
}

pub fn decode_utf8_lossy(bytes: Vec<u8>) -> String {
    let body = match bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        Some(rest) => rest.to_vec(),
        None => bytes,
    };
    match String::from_utf8(body) {
        Ok(text) => text,
        Err(error) => String::from_utf8_lossy(error.as_bytes()).into_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::Buffer;

    fn store() -> (tempfile::TempDir, Store) {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path()).unwrap();
        (dir, store)
    }

    #[test]
    fn ids_are_unique_and_filename_safe() {
        let ids: Vec<String> = (0..1000).map(|_| new_id()).collect();
        for id in &ids {
            assert!(is_safe_id(id), "{id:?} is not safe as a file name");
        }
        let mut sorted = ids.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), ids.len(), "ids collided");
    }

    #[test]
    fn a_first_run_gets_an_empty_session_without_complaint() {
        let (_dir, store) = store();
        let (session, note) = store.load_session();
        assert!(session.buffers.is_empty());
        assert!(note.is_none(), "a missing session file is not an error");
    }

    #[test]
    fn buffers_round_trip() {
        let (_dir, store) = store();
        store.write_buffer("abc", "hello\nworld\n").unwrap();
        assert_eq!(store.read_buffer("abc").unwrap(), "hello\nworld\n");
    }

    #[test]
    fn reading_a_buffer_that_was_never_written_gives_empty_not_an_error() {
        let (_dir, store) = store();
        assert_eq!(store.read_buffer("neverwritten").unwrap(), "");
    }

    #[test]
    fn the_whole_session_survives_a_round_trip() {
        let (_dir, store) = store();
        let mut session = Session::default();
        session.buffers.push(Buffer {
            caret: 11,
            dirty: true,
            ..Buffer::untitled("s1", "new 1")
        });
        session.buffers.push(Buffer::for_path("s2", "/tmp/x.rs"));
        session.active = 1;
        store.save_session(&session).unwrap();
        store.write_buffer("s1", "scratch text").unwrap();

        let (loaded, note) = store.load_session();
        assert!(note.is_none());
        assert_eq!(loaded, session);
        assert_eq!(store.read_buffer("s1").unwrap(), "scratch text");
    }

    #[test]
    fn an_unsafe_id_cannot_reach_outside_the_buffers_directory() {
        let (dir, store) = store();
        let escape = "../../escaped";
        assert!(matches!(
            store.write_buffer(escape, "x"),
            Err(StoreError::UnsafeId(_))
        ));
        assert!(matches!(
            store.read_buffer(escape),
            Err(StoreError::UnsafeId(_))
        ));
        assert!(matches!(
            store.delete_buffer(escape),
            Err(StoreError::UnsafeId(_))
        ));
        // Prove nothing was created anywhere near the store.
        assert!(!dir.path().parent().unwrap().join("escaped.txt").exists());
    }

    #[test]
    fn a_corrupt_session_is_set_aside_and_the_app_still_starts() {
        let (_dir, store) = store();
        fs::write(store.session_path(), "{ this is not json").unwrap();

        let (session, note) = store.load_session();
        assert!(session.buffers.is_empty(), "must start, not fail");
        let note = note.expect("the reason should be reported for logging");
        assert!(note.contains("unreadable"), "got: {note}");
        assert!(
            store.root().join("session.corrupt.json").exists(),
            "the unreadable file must be kept for recovery, not overwritten"
        );
    }

    #[test]
    fn a_corrupt_session_does_not_touch_the_snapshots() {
        let (_dir, store) = store();
        store
            .write_buffer("keepme", "precious scratch note")
            .unwrap();
        fs::write(store.session_path(), "not json at all").unwrap();

        let (_session, _note) = store.load_session();
        assert_eq!(
            store.read_buffer("keepme").unwrap(),
            "precious scratch note",
            "snapshots must survive a corrupt session file"
        );
    }

    #[test]
    fn prune_removes_only_snapshots_no_tab_refers_to() {
        let (_dir, store) = store();
        store.write_buffer("live1", "a").unwrap();
        store.write_buffer("live2", "b").unwrap();
        store.write_buffer("orphan", "c").unwrap();

        let mut session = Session::default();
        session.buffers.push(Buffer::untitled("live1", "new 1"));
        session.buffers.push(Buffer::untitled("live2", "new 2"));

        assert_eq!(store.prune_orphans(&session), 1);
        assert_eq!(store.read_buffer("live1").unwrap(), "a");
        assert_eq!(store.read_buffer("live2").unwrap(), "b");
        assert_eq!(store.read_buffer("orphan").unwrap(), "");
    }

    #[test]
    fn prune_ignores_files_it_did_not_write() {
        let (_dir, store) = store();
        fs::write(store.buffers_dir().join("notes.md"), "not ours").unwrap();
        let removed = store.prune_orphans(&Session::default());
        assert_eq!(removed, 0);
        assert!(store.buffers_dir().join("notes.md").exists());
    }

    #[test]
    fn delete_is_idempotent() {
        let (_dir, store) = store();
        store.write_buffer("gone", "x").unwrap();
        store.delete_buffer("gone").unwrap();
        store.delete_buffer("gone").unwrap();
    }

    #[test]
    fn an_overwrite_leaves_no_temporary_files_behind() {
        let (_dir, store) = store();
        for i in 0..5 {
            store
                .write_buffer("rewritten", &format!("version {i}"))
                .unwrap();
        }
        assert_eq!(store.read_buffer("rewritten").unwrap(), "version 4");

        let leftovers: Vec<_> = fs::read_dir(store.buffers_dir())
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|name| name.contains(".tmp"))
            .collect();
        assert!(
            leftovers.is_empty(),
            "temp files left behind: {leftovers:?}"
        );
    }

    #[test]
    fn a_bom_is_stripped_and_invalid_bytes_do_not_lose_the_file() {
        assert_eq!(decode_utf8_lossy(vec![0xEF, 0xBB, 0xBF, b'h', b'i']), "hi");
        let decoded = decode_utf8_lossy(vec![b'a', 0xFF, b'b']);
        assert!(decoded.starts_with('a') && decoded.ends_with('b'));
    }

    #[test]
    fn text_with_emoji_and_newlines_survives_exactly() {
        let (_dir, store) = store();
        let text = "line one\r\nline two\n\tটেস্ট 🎉 done\n";
        store.write_buffer("unicode", text).unwrap();
        assert_eq!(store.read_buffer("unicode").unwrap(), text);
    }
}
