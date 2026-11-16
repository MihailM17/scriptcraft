//! "Check for updates": reads the latest GitHub release in a background
//! thread and reports whether it is newer than this build.
//!
//! Nothing here installs an update. The app is not notarised and has no
//! updater, so the prompt links to the release page and the writer downloads
//! the build the same way they got this one.
//!
//! The network request shells out to `curl`, which ships with macOS and
//! Windows 10+ and is present on every Linux desktop we support. That keeps
//! the dependency list free of a TLS stack, and a missing or failing `curl`
//! is a normal "couldn't check" instead of a crash.

use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread;

/// This repository, used by the API call and by the release link.
pub const REPO: &str = "MihailM17/scriptcraft";
/// Where a writer reads the release notes and downloads a build.
pub const RELEASES_PAGE: &str = "https://github.com/MihailM17/scriptcraft/releases";
/// GitHub's endpoint for the newest non-draft, non-prerelease release.
pub const API_LATEST: &str = "https://api.github.com/repos/MihailM17/scriptcraft/releases/latest";

/// Version compiled into this binary, without the `v` prefix.
pub const CURRENT: &str = env!("CARGO_PKG_VERSION");

/// A release from the API. `version` is normalised (`v0.8.7` → `0.8.7`);
/// `url` is the human-readable release page.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Release {
    pub version: String,
    pub url: String,
}

/// State of the last check. The Settings page renders this verbatim.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Check {
    /// No check has run this launch.
    #[default]
    Idle,
    Checking,
    UpToDate,
    Available(Release),
    /// The request failed, or GitHub answered with something unreadable.
    Failed(String),
}

impl Check {
    pub fn is_checking(&self) -> bool {
        matches!(self, Check::Checking)
    }

    /// One-line status for Settings. Failure stays short; the raw reason is
    /// available in [`Check::detail`] for the hover text.
    pub fn note(&self) -> String {
        match self {
            Check::Idle => String::new(),
            Check::Checking => "Checking GitHub…".to_string(),
            Check::UpToDate => "You're up to date.".to_string(),
            Check::Available(r) => format!("Version {} is available.", r.version),
            Check::Failed(_) => "Couldn't check for updates.".to_string(),
        }
    }

    /// Hover text: why a check failed, or which page the update comes from.
    pub fn detail(&self) -> String {
        match self {
            Check::Idle => {
                "Looks for a newer release on github.com/MihailM17/scriptcraft.".to_string()
            }
            Check::Checking => "Asking GitHub for the latest release…".to_string(),
            Check::UpToDate => format!("ScriptCraft {CURRENT} is the latest release."),
            Check::Available(r) => format!("Download it from {}", r.url),
            Check::Failed(why) => why.clone(),
        }
    }
}

/// Checker owned by the app: the last result, the in-flight request, and a
/// one-shot prompt for the newest release.
#[derive(Default)]
pub struct UpdateState {
    pub check: Check,
    /// Set when a check finds a newer release; the app shows a modal until it
    /// is dismissed.
    pub prompt: Option<Release>,
    rx: Option<Receiver<Result<Check, String>>>,
    /// The on-open check runs once per launch, not on every frame.
    startup_done: bool,
    /// Tests exercise the button without spawning `curl`.
    skip_network_for_test: bool,
}

impl UpdateState {
    /// Start the check that runs when the app opens. Safe to call twice; only
    /// the first call starts a request.
    pub fn start_on_open(&mut self) {
        if self.startup_done {
            return;
        }
        self.startup_done = true;
        self.start();
    }

    /// Ask GitHub for the latest release. No-op while a request is already in
    /// flight, so a double click cannot stack requests.
    pub fn start(&mut self) {
        if self.check.is_checking() {
            return;
        }
        if self.skip_network_for_test {
            self.check = Check::Checking;
            return;
        }
        let (tx, rx) = mpsc::channel();
        self.rx = Some(rx);
        self.check = Check::Checking;
        thread::spawn(move || {
            let _ = tx.send(fetch_latest());
        });
    }

    /// Drain the background result. Returns true when the status changed, so
    /// the caller can request a repaint.
    pub fn poll(&mut self) -> bool {
        let Some(rx) = self.rx.as_ref() else {
            return false;
        };
        match rx.try_recv() {
            Ok(Ok(check)) => {
                if let Check::Available(release) = &check {
                    self.prompt = Some(release.clone());
                }
                self.check = check;
                self.rx = None;
                true
            }
            Ok(Err(why)) => {
                self.check = Check::Failed(why);
                self.rx = None;
                true
            }
            Err(TryRecvError::Empty) => false,
            Err(TryRecvError::Disconnected) => {
                self.check = Check::Failed("the update check stopped unexpectedly".to_string());
                self.rx = None;
                true
            }
        }
    }

    /// Test hook: inject a result without touching the network.
    #[doc(hidden)]
    pub fn set_for_test(&mut self, check: Check) {
        if let Check::Available(release) = &check {
            self.prompt = Some(release.clone());
        }
        self.check = check;
        self.rx = None;
        self.startup_done = true;
    }

    /// Test hook: is a background request in flight?
    #[doc(hidden)]
    pub fn is_running(&self) -> bool {
        self.rx.is_some()
    }

    /// Test hook: record the "checking" state without touching the network,
    /// so a click on Settings ▸ Check for updates can be asserted offline.
    #[doc(hidden)]
    pub fn set_offline_for_test(&mut self) {
        self.skip_network_for_test = true;
    }
}

/// Read GitHub's latest-release JSON. `Ok(None)` means the release is a draft
/// or prerelease (the API normally filters those, this is belt and braces).
pub fn parse_latest(json: &str) -> Result<Option<Release>, String> {
    #[derive(serde::Deserialize)]
    struct ApiRelease {
        tag_name: Option<String>,
        html_url: Option<String>,
        #[serde(default)]
        draft: bool,
        #[serde(default)]
        prerelease: bool,
    }

    let parsed: ApiRelease =
        serde_json::from_str(json).map_err(|e| format!("GitHub sent an unreadable reply: {e}"))?;
    if parsed.draft || parsed.prerelease {
        return Ok(None);
    }
    let Some(tag) = parsed.tag_name else {
        return Err("the latest release has no tag".to_string());
    };
    Ok(Some(Release {
        version: normalise_version(&tag),
        url: parsed
            .html_url
            .filter(|u| !u.trim().is_empty())
            .unwrap_or_else(|| RELEASES_PAGE.to_string()),
    }))
}

/// `v0.8.7` and `0.8.7` are the same version; tags keep their pre-release
/// suffix (`0.8.7-beta.1`).
fn normalise_version(tag: &str) -> String {
    tag.trim().trim_start_matches(['v', 'V']).trim().to_string()
}

/// Is `latest` a newer release than `current`? Numeric parts compare in
/// order; a pre-release of the same version is older than the release.
pub fn is_newer(latest: &str, current: &str) -> bool {
    fn split(v: &str) -> (Vec<u64>, bool) {
        let v = normalise_version(v);
        let (core, prerelease) = match v.split_once('-') {
            Some((core, suffix)) => (core, !suffix.is_empty()),
            None => (v.as_str(), false),
        };
        let numbers = core
            .split('.')
            .map(|part| {
                part.split('+')
                    .next()
                    .unwrap_or("")
                    .parse::<u64>()
                    .unwrap_or(0)
            })
            .collect();
        (numbers, prerelease)
    }

    let (new, new_pre) = split(latest);
    let (old, old_pre) = split(current);
    for i in 0..new.len().max(old.len()) {
        let a = new.get(i).copied().unwrap_or(0);
        let b = old.get(i).copied().unwrap_or(0);
        if a != b {
            return a > b;
        }
    }
    // Same numbers: 0.8.7-beta is older than 0.8.7, and an identical
    // pre-release tag is not an update.
    old_pre && !new_pre
}

/// The request, run on a worker thread. `Ok(UpToDate)` when this build is at
/// least as new as the published one.
fn fetch_latest() -> Result<Check, String> {
    let output = std::process::Command::new("curl")
        .args([
            "--silent",
            "--show-error",
            "--location",
            "--max-time",
            "12",
            "--proto",
            "=https",
            "--tlsv1.2",
            "-H",
            "Accept: application/vnd.github+json",
            "-H",
            "User-Agent: ScriptCraft",
        ])
        .arg(API_LATEST)
        .output()
        .map_err(|e| format!("curl is not available: {e}"))?;

    if !output.status.success() {
        let why = String::from_utf8_lossy(&output.stderr);
        let why = why.trim();
        return Err(if why.is_empty() {
            format!("GitHub request failed ({})", output.status)
        } else {
            format!("GitHub request failed: {why}")
        });
    }

    let json = String::from_utf8(output.stdout)
        .map_err(|_| "GitHub sent a reply that is not UTF-8".to_string())?;
    match parse_latest(&json)? {
        Some(release) if is_newer(&release.version, CURRENT) => Ok(Check::Available(release)),
        _ => Ok(Check::UpToDate),
    }
}

/// Open the release page in the platform browser. Used by the update prompt.
pub fn open_release(url: &str) -> Result<(), String> {
    // Only ever open an https link from the checked API; a malformed release
    // record must not launch an arbitrary command.
    if !url.starts_with("https://") {
        return Err("refusing to open a non-https update link".to_string());
    }

    #[cfg(target_os = "macos")]
    let child = std::process::Command::new("open").arg(url).spawn();
    #[cfg(target_os = "windows")]
    let child = std::process::Command::new("cmd")
        .args(["/C", "start", "", url])
        .spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let child = std::process::Command::new("xdg-open").arg(url).spawn();

    child
        .map(|_| ())
        .map_err(|e| format!("could not open the release page: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_compare_orders_releases() {
        assert!(is_newer("0.8.7", "0.8.6"));
        assert!(is_newer("v0.8.7", "0.8.6"));
        assert!(is_newer("0.9.0", "0.8.99"));
        assert!(is_newer("1.0.0", "0.99.99"));
        assert!(is_newer("0.8.6.1", "0.8.6"));
        assert!(!is_newer("0.8.6", "0.8.6"));
        assert!(!is_newer("v0.8.6", "0.8.6"));
        assert!(!is_newer("0.8.5", "0.8.6"));
        assert!(!is_newer("0.8.6-beta", "0.8.6"));
        assert!(is_newer("0.8.7-beta", "0.8.6"));
    }

    #[test]
    fn parse_release_reads_the_tag_and_page() {
        let json = r#"{
            "tag_name": "v0.8.7",
            "html_url": "https://github.com/MihailM17/scriptcraft/releases/tag/v0.8.7",
            "draft": false,
            "prerelease": false
        }"#;
        let release = parse_latest(json).unwrap().unwrap();
        assert_eq!(release.version, "0.8.7");
        assert_eq!(
            release.url,
            "https://github.com/MihailM17/scriptcraft/releases/tag/v0.8.7"
        );
    }

    #[test]
    fn parse_release_skips_drafts_and_prereleases() {
        let draft = r#"{"tag_name":"v0.9.0","draft":true}"#;
        let pre = r#"{"tag_name":"v0.9.0","prerelease":true}"#;
        assert_eq!(parse_latest(draft).unwrap(), None);
        assert_eq!(parse_latest(pre).unwrap(), None);
    }

    #[test]
    fn parse_release_rejects_json_without_a_tag() {
        let err = parse_latest(r#"{"message":"Not Found"}"#).unwrap_err();
        assert!(err.contains("no tag"), "{err}");
        assert!(parse_latest("not json").is_err());
    }

    #[test]
    fn check_notes_name_the_version_and_the_failure() {
        let up = Check::Available(Release {
            version: "0.8.7".into(),
            url: "https://example.invalid".into(),
        });
        assert!(up.note().contains("0.8.7"));
        assert!(up.detail().contains("https://example.invalid"));
        assert_eq!(Check::UpToDate.note(), "You're up to date.");
        assert_eq!(
            Check::Failed("no network".into()).note(),
            "Couldn't check for updates."
        );
    }

    #[test]
    fn api_and_release_links_point_at_this_repository() {
        assert!(API_LATEST.contains(REPO));
        assert!(RELEASES_PAGE.contains(REPO));
        assert!(API_LATEST.starts_with("https://"));
        assert!(RELEASES_PAGE.starts_with("https://"));
    }

    #[test]
    fn open_release_refuses_anything_but_https() {
        assert!(open_release("http://example.com").is_err());
        assert!(open_release("file:///etc/passwd").is_err());
        assert!(open_release("").is_err());
    }

    #[test]
    fn injected_result_does_not_touch_the_network() {
        let mut state = UpdateState::default();
        state.set_for_test(Check::Available(Release {
            version: "9.9.9".into(),
            url: RELEASES_PAGE.into(),
        }));
        assert!(!state.is_running());
        assert_eq!(state.prompt.as_ref().unwrap().version, "9.9.9");
        assert!(!state.poll(), "nothing to drain");
    }
}
