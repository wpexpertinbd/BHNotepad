//! Checking GitHub for a newer BHNotepad.
//!
//! Follows the same rules as BHUninstaller's `selfupdate`, for the same
//! reasons:
//!
//! - **Only GitHub.** A download URL taken from the API must be exactly
//!   `github.com` or `objects.githubusercontent.com`. An exact match, not a
//!   prefix — `starts_with` would happily accept `github.com.evil.example`.
//! - **Numbers, not strings.** `0.10` is newer than `0.9`; comparing as text
//!   says otherwise and would nag forever.
//! - **A failed check is never "you are up to date".** Being offline is
//!   normal; turning that into reassurance is a different and much more
//!   misleading statement.
//! - **Throttled.** GitHub's unauthenticated API allows 60 requests an hour
//!   per *IP*, shared by everyone behind it. Checking on every launch would be
//!   rude to the user's whole network.
//! - **Nothing is installed behind the user's back.** The installer is
//!   downloaded and handed to their file manager; opening it is their call.
//!
//! The request carries a user agent and nothing else — no identifier, no
//! usage data, no telemetry of any kind.

/// Splits `1.2.3`, `v1.2.3` or `1.2.3-beta.1` into its numeric parts.
///
/// A pre-release suffix is ignored for ordering: BHNotepad does not publish
/// them, and treating `1.2.3-beta` as distinct from `1.2.3` would be a
/// guess about semantics we never use.
fn parts(version: &str) -> Vec<u64> {
    version
        .trim()
        .trim_start_matches(['v', 'V'])
        .split('-')
        .next()
        .unwrap_or("")
        .split('.')
        .map(|part| part.trim().parse::<u64>().unwrap_or(0))
        .collect()
}

/// Whether `candidate` is a later version than `current`.
///
/// Missing components count as zero, so `1.3` is newer than `1.2.9` and equal
/// to `1.3.0`. Anything unparseable becomes zero rather than an error: a
/// malformed tag should mean "no update", never a crash or a false alarm.
pub fn is_newer(candidate: &str, current: &str) -> bool {
    let a = parts(candidate);
    let b = parts(current);
    for index in 0..a.len().max(b.len()) {
        let left = a.get(index).copied().unwrap_or(0);
        let right = b.get(index).copied().unwrap_or(0);
        if left != right {
            return left > right;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_higher_version_is_newer() {
        assert!(is_newer("0.3.1", "0.3.0"));
        assert!(is_newer("0.4.0", "0.3.9"));
        assert!(is_newer("1.0.0", "0.99.99"));
    }

    #[test]
    fn the_same_version_is_not_newer() {
        assert!(!is_newer("0.3.0", "0.3.0"));
        assert!(
            !is_newer("v0.3.0", "0.3.0"),
            "a leading v is not a difference"
        );
        assert!(
            !is_newer("0.3", "0.3.0"),
            "a missing component counts as zero"
        );
    }

    #[test]
    fn an_older_version_is_not_newer() {
        assert!(!is_newer("0.2.9", "0.3.0"));
        assert!(!is_newer("0.3.0", "0.3.1"));
    }

    #[test]
    fn components_compare_numerically_not_as_text() {
        // The bug every hand-rolled version check has: "10" < "9" as strings.
        assert!(is_newer("0.10.0", "0.9.0"));
        assert!(is_newer("1.0.10", "1.0.9"));
        assert!(!is_newer("0.9.0", "0.10.0"));
    }

    #[test]
    fn a_pre_release_suffix_is_ignored() {
        assert!(is_newer("0.4.0-beta.1", "0.3.0"));
        assert!(!is_newer("0.3.0-beta.1", "0.3.0"));
    }

    #[test]
    fn rubbish_never_reports_an_update() {
        // A malformed tag must not nag the user; zero compares as not-newer.
        assert!(!is_newer("", "0.3.0"));
        assert!(!is_newer("not-a-version", "0.3.0"));
        assert!(!is_newer("latest", "0.3.0"));
    }

    #[test]
    fn a_long_tag_still_orders_correctly() {
        assert!(is_newer("1.2.3.4", "1.2.3.3"));
        assert!(!is_newer("1.2.3", "1.2.3.1"));
    }
}

// ── Asking GitHub ────────────────────────────────────────────────────────
//
// Behind a feature so the core stays dependency-light and so the test suite,
// which must never touch the network, can be run without it.

#[cfg(feature = "updates")]
mod remote {
    use std::path::PathBuf;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    use serde::{Deserialize, Serialize};

    const REPO: &str = "wpexpertinbd/BHNotepad";
    /// Exact hosts, never prefixes. See the module docs.
    const ALLOWED_HOSTS: &[&str] = &["github.com", "objects.githubusercontent.com"];

    /// How long to leave between automatic checks.
    pub const CHECK_INTERVAL_SECS: i64 = 24 * 60 * 60;

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct Release {
        pub version: String,
        pub notes: String,
        /// The release page, for someone who would rather look first.
        pub page_url: String,
        pub asset_name: Option<String>,
        pub asset_url: Option<String>,
        pub asset_size: u64,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct CheckResult {
        /// The newer release, when there is one.
        pub update: Option<Release>,
        /// What is running now.
        pub current: String,
        /// Why the check could not be completed. Distinct from "no update":
        /// the UI must not turn a failure into reassurance.
        pub error: Option<String>,
    }

    pub fn now_secs() -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0)
    }

    /// Whether an automatic check is due.
    pub fn check_due(last_check: i64) -> bool {
        now_secs().saturating_sub(last_check) >= CHECK_INTERVAL_SECS
    }

    fn agent() -> ureq::Agent {
        ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(15)))
            // A 404 means "no releases published yet", which is an answer
            // rather than a failure. Without this ureq turns it into an error
            // and the user is told the check broke.
            .http_status_as_error(false)
            .user_agent(concat!("BHNotepad/", env!("CARGO_PKG_VERSION")))
            .build()
            .new_agent()
    }

    /// Is this a URL we are willing to download from?
    fn host_allowed(url: &str) -> bool {
        let Some(rest) = url.strip_prefix("https://") else {
            return false;
        };
        let host = rest
            .split('/')
            .next()
            .unwrap_or("")
            .split('@')
            .next_back()
            .unwrap_or("")
            .split(':')
            .next()
            .unwrap_or("");
        ALLOWED_HOSTS.iter().any(|h| host.eq_ignore_ascii_case(h))
    }

    /// The installer for this platform: (name, url, size).
    fn pick_asset(json: &serde_json::Value) -> Option<(String, String, u64)> {
        let wanted: &[&str] = if cfg!(target_os = "macos") {
            &[".dmg"]
        } else if cfg!(target_os = "windows") {
            &[".msi", "-setup.exe"]
        } else {
            &[".deb", ".rpm"]
        };

        let assets = json.get("assets")?.as_array()?;
        for suffix in wanted {
            for asset in assets {
                let name = asset
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();
                if !name.to_lowercase().ends_with(suffix) {
                    continue;
                }
                // Two macOS and two Linux builds ship per release; take the one
                // for this architecture rather than whichever sorts first.
                if !matches_arch(name) {
                    continue;
                }
                let url = asset
                    .get("browser_download_url")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();
                if !host_allowed(url) {
                    continue;
                }
                let size = asset.get("size").and_then(|v| v.as_u64()).unwrap_or(0);
                return Some((name.to_string(), url.to_string(), size));
            }
        }
        None
    }

    /// Whether an asset's name is for the architecture this build runs on.
    fn matches_arch(name: &str) -> bool {
        let lower = name.to_lowercase();
        if cfg!(target_arch = "aarch64") {
            lower.contains("aarch64") || lower.contains("arm64")
        } else {
            lower.contains("x64") || lower.contains("x86_64") || lower.contains("amd64")
        }
    }

    #[cfg(test)]
    pub fn host_allowed_for_test(url: &str) -> bool {
        host_allowed(url)
    }

    #[cfg(test)]
    pub fn matches_arch_for_test(name: &str) -> bool {
        matches_arch(name)
    }

    /// Ask GitHub for the newest release.
    pub fn check() -> CheckResult {
        let current = env!("CARGO_PKG_VERSION").to_string();
        let url = format!("https://api.github.com/repos/{REPO}/releases/latest");

        let body = match agent().get(&url).call() {
            Ok(mut response) => {
                if response.status() != 200 {
                    return CheckResult {
                        update: None,
                        current,
                        // 404 means no releases yet: an answer, not a problem.
                        error: (response.status() != 404)
                            .then(|| format!("GitHub answered {}", response.status())),
                    };
                }
                match response
                    .body_mut()
                    .with_config()
                    .limit(1024 * 1024)
                    .read_to_string()
                {
                    Ok(text) => text,
                    Err(error) => {
                        return CheckResult {
                            update: None,
                            current,
                            error: Some(error.to_string()),
                        }
                    }
                }
            }
            Err(error) => {
                return CheckResult {
                    update: None,
                    current,
                    error: Some(error.to_string()),
                }
            }
        };

        let Ok(json) = serde_json::from_str::<serde_json::Value>(&body) else {
            return CheckResult {
                update: None,
                current,
                error: Some("could not read GitHub's answer".into()),
            };
        };

        let tag = json
            .get("tag_name")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .trim_start_matches('v')
            .to_string();

        if tag.is_empty() || !super::is_newer(&tag, &current) {
            return CheckResult {
                update: None,
                current,
                error: None,
            };
        }

        let asset = pick_asset(&json);
        CheckResult {
            update: Some(Release {
                version: tag,
                notes: json
                    .get("body")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string(),
                page_url: json
                    .get("html_url")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| format!("https://github.com/{REPO}/releases")),
                asset_name: asset.as_ref().map(|a| a.0.clone()),
                asset_url: asset.as_ref().map(|a| a.1.clone()),
                asset_size: asset.as_ref().map(|a| a.2).unwrap_or(0),
            }),
            current,
            error: None,
        }
    }

    /// A scratch directory only this process could have created.
    ///
    /// The download is an installer the user will be asked to approve, so it
    /// must not be possible for anything to have prepared the path in advance
    /// — as a directory to write into later, or as a symlink pointing
    /// somewhere else. The name is unpredictable and `create_dir` refuses a
    /// path that exists at all, symlink included, rather than following it.
    fn private_temp_dir(prefix: &str) -> Result<PathBuf, String> {
        let base = std::env::temp_dir();
        let mut last = String::new();
        for attempt in 0..8u32 {
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.subsec_nanos())
                .unwrap_or(0);
            let dir = base.join(format!(
                "{prefix}-{}-{:08x}{:x}",
                std::process::id(),
                nanos,
                attempt
            ));
            match std::fs::create_dir(&dir) {
                Ok(()) => {
                    let meta = std::fs::symlink_metadata(&dir)
                        .map_err(|e| format!("could not verify {}: {e}", dir.display()))?;
                    if meta.is_symlink() || !meta.is_dir() {
                        return Err("the scratch folder was not what we created — refusing".into());
                    }
                    return Ok(dir);
                }
                Err(e) => last = e.to_string(),
            }
        }
        Err(format!("could not create a scratch folder: {last}"))
    }

    /// The file this process last downloaded.
    ///
    /// Whatever launches the installer has to be certain it is running *the
    /// download* and not a path someone else chose. Comparing against this is
    /// exact; deriving the expected directory instead would accept any file
    /// that happened to be sitting in it.
    static DOWNLOADED: std::sync::Mutex<Option<PathBuf>> = std::sync::Mutex::new(None);

    pub fn last_download() -> Option<PathBuf> {
        DOWNLOADED.lock().ok().and_then(|l| l.clone())
    }

    /// Downloads a release's installer and returns where it landed.
    pub fn download(release: &Release) -> Result<PathBuf, String> {
        let url = release
            .asset_url
            .as_ref()
            .ok_or("this release has no download for this platform")?;
        if !host_allowed(url) {
            return Err("that download is not hosted on GitHub — refusing it".into());
        }
        let name = release.asset_name.as_deref().unwrap_or("BHNotepad-update");

        let mut response = agent()
            .get(url)
            .call()
            .map_err(|error| format!("download failed: {error}"))?;
        if response.status() != 200 {
            return Err(format!(
                "download failed: GitHub answered {}",
                response.status()
            ));
        }

        // Far larger than any build of this app; the cap only stops an
        // unexpected response filling the disk.
        let bytes = response
            .body_mut()
            .with_config()
            .limit(250 * 1024 * 1024)
            .read_to_vec()
            .map_err(|error| format!("download failed: {error}"))?;

        let dir = private_temp_dir("BHNotepad-update")?;
        // The name comes from GitHub, so it is not trusted as a path.
        let safe: String = name
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        let path = dir.join(safe);
        // `create_new` refuses an existing file — a symlink included — rather
        // than writing through it.
        let _ = std::fs::remove_file(&path);
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|error| format!("could not save the download: {error}"))?;
        std::io::Write::write_all(&mut file, &bytes)
            .map_err(|error| format!("could not save the download: {error}"))?;

        if let Ok(mut last) = DOWNLOADED.lock() {
            *last = Some(path.clone());
        }
        Ok(path)
    }
}

#[cfg(feature = "updates")]
pub use remote::{
    check, check_due, download, last_download, now_secs, CheckResult, Release, CHECK_INTERVAL_SECS,
};

#[cfg(all(test, feature = "updates"))]
mod remote_tests {
    use super::remote::*;

    // `host_allowed` and `matches_arch` are private; these exercise them
    // through the small shims the module exposes for testing.

    #[test]
    fn only_exact_github_hosts_are_accepted() {
        for good in [
            "https://github.com/wpexpertinbd/BHNotepad/releases/download/v1/x.dmg",
            "https://objects.githubusercontent.com/whatever",
            "https://GitHub.com/case/insensitive.dmg",
        ] {
            assert!(host_allowed_for_test(good), "should allow {good}");
        }

        for bad in [
            // The whole reason this is an exact match and not `starts_with`.
            "https://github.com.evil.example.com/payload.dmg",
            "https://notgithub.com/x.dmg",
            "https://evil.example.com/github.com/x.dmg",
            // Credentials in the authority must not smuggle the host past us.
            "https://github.com@evil.example.com/x.dmg",
            // Plain HTTP is never acceptable for an installer.
            "http://github.com/x.dmg",
            "ftp://github.com/x.dmg",
            "",
        ] {
            assert!(!host_allowed_for_test(bad), "should refuse {bad}");
        }
    }

    #[test]
    fn the_asset_for_this_architecture_is_chosen() {
        // Every release ships two macOS and two Linux builds; picking the
        // wrong one hands the user an installer that will not run.
        let aarch64 = matches_arch_for_test("BHNotepad_0.3.0_aarch64.dmg");
        let x64 = matches_arch_for_test("BHNotepad_0.3.0_x64.dmg");
        assert_ne!(
            aarch64, x64,
            "exactly one of the two should match this build"
        );
        if cfg!(target_arch = "aarch64") {
            assert!(aarch64 && !x64);
            assert!(matches_arch_for_test("BHNotepad_0.3.0_arm64.deb"));
        } else {
            assert!(x64 && !aarch64);
            assert!(matches_arch_for_test("BHNotepad_0.3.0_amd64.deb"));
        }
    }
}
