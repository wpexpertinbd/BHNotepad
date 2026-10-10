//! User settings, kept in `<configDir>/settings.json`.
//!
//! Deliberately tiny. The only thing in here is the one limit a user has a
//! real reason to disagree with — how big a file may be opened — and it is a
//! setting rather than a constant precisely because the right answer depends
//! on the machine and on what the person actually does with it.

use serde::{Deserialize, Serialize};

use crate::store::Store;

/// Default ceiling on the size of a file that may be opened.
///
/// Measured, not guessed. On a 24 GB Apple-silicon machine a 512 MB SQL dump
/// opens in about seven seconds, uses roughly 1.6 GB of memory and is fully
/// editable; a 1 GB file never finishes arriving in the editor at all, because
/// the text crosses into the webview as a single JavaScript string. 512 MB is
/// therefore the largest size known to work rather than a cautious guess — and
/// anyone whose machine disagrees can change it.
pub const DEFAULT_MAX_OPEN_BYTES: u64 = 512 * 1024 * 1024;

/// A floor, so a typo in the settings file cannot make the editor useless.
pub const MIN_MAX_OPEN_BYTES: u64 = 1024 * 1024;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Settings {
    /// Largest file that may be opened into a tab, in bytes.
    ///
    /// Raising this past the default is allowed and sometimes right, but the
    /// failure it guards against is not graceful: a file too large to reach
    /// the editor leaves the tab unable to display anything.
    #[serde(default = "default_max_open_bytes")]
    pub max_open_bytes: u64,
}

fn default_max_open_bytes() -> u64 {
    DEFAULT_MAX_OPEN_BYTES
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            max_open_bytes: DEFAULT_MAX_OPEN_BYTES,
        }
    }
}

impl Settings {
    /// Clamps anything unusable back into range.
    pub fn normalize(&mut self) {
        if self.max_open_bytes < MIN_MAX_OPEN_BYTES {
            self.max_open_bytes = MIN_MAX_OPEN_BYTES;
        }
    }

    /// Reads the settings file, falling back to defaults.
    ///
    /// Never fails: a settings file that cannot be read or parsed must not
    /// stop the editor starting, and the defaults are always usable.
    pub fn load(store: &Store) -> (Self, Option<String>) {
        let path = store.root().join("settings.json");
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return (Self::default(), None)
            }
            Err(error) => {
                return (
                    Self::default(),
                    Some(format!("could not read {}: {error}", path.display())),
                )
            }
        };

        match serde_json::from_str::<Self>(&text) {
            Ok(mut settings) => {
                settings.normalize();
                (settings, None)
            }
            Err(error) => (
                Self::default(),
                Some(format!(
                    "{} could not be read ({error}); using the defaults",
                    path.display()
                )),
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> (tempfile::TempDir, Store) {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path()).unwrap();
        (dir, store)
    }

    #[test]
    fn a_missing_settings_file_is_not_an_error() {
        let (_dir, store) = store();
        let (settings, note) = Settings::load(&store);
        assert_eq!(settings.max_open_bytes, DEFAULT_MAX_OPEN_BYTES);
        assert!(note.is_none());
    }

    #[test]
    fn a_setting_is_read_back() {
        let (_dir, store) = store();
        std::fs::write(
            store.root().join("settings.json"),
            r#"{"max_open_bytes": 1073741824}"#,
        )
        .unwrap();
        let (settings, note) = Settings::load(&store);
        assert_eq!(settings.max_open_bytes, 1024 * 1024 * 1024);
        assert!(note.is_none());
    }

    #[test]
    fn a_corrupt_settings_file_falls_back_rather_than_failing() {
        let (_dir, store) = store();
        std::fs::write(store.root().join("settings.json"), "{ not json").unwrap();
        let (settings, note) = Settings::load(&store);
        assert_eq!(settings.max_open_bytes, DEFAULT_MAX_OPEN_BYTES);
        assert!(note.unwrap().contains("could not be read"));
    }

    #[test]
    fn an_absurdly_small_limit_is_clamped() {
        let (_dir, store) = store();
        std::fs::write(
            store.root().join("settings.json"),
            r#"{"max_open_bytes": 3}"#,
        )
        .unwrap();
        let (settings, _) = Settings::load(&store);
        assert_eq!(
            settings.max_open_bytes, MIN_MAX_OPEN_BYTES,
            "a typo must not make the editor unable to open anything"
        );
    }
}
