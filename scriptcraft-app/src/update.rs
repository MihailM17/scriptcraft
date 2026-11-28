//! "Check for updates": reads the latest GitHub release in a background
//! thread, and installs it on request.
//!
//! Installing is a staged download, not a live patcher: the platform archive
//! and its published `.sha256` are fetched into the per-user config folder,
//! the digest is verified, and a small helper waits for this process to exit,
//! replaces the app in place and relaunches it. Nothing is executed from the
//! archive before it is verified.
//!
//! The builds are unsigned (macOS ad-hoc signed), so the operating system can
//! still warn about a downloaded update. The checksum proves the transfer,
//! not the publisher; that needs Developer ID / Authenticode signing.
//!
//! The network request shells out to `curl`, which ships with macOS and
//! Windows 10+ and is present on every Linux desktop we support. That keeps
//! the dependency list free of a TLS stack, and a missing or failing `curl`
//! is a normal "couldn't check" instead of a crash.

use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
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

/// A downloadable file attached to a release.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Asset {
    pub name: String,
    pub url: String,
    pub size: u64,
}

/// A release from the API. `version` is normalised (`v0.8.7` → `0.8.7`);
/// `url` is the human-readable release page.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Release {
    pub version: String,
    pub url: String,
    pub assets: Vec<Asset>,
}

impl Release {
    /// A release without downloadable assets, for offline tests and callers
    /// that only need the version/page.
    pub fn without_assets(version: impl Into<String>, url: impl Into<String>) -> Self {
        Self {
            version: version.into(),
            url: url.into(),
            assets: Vec::new(),
        }
    }

    /// The archive this platform can install, or `None` when the release
    /// does not contain a build for it.
    pub fn installable_asset(&self) -> Option<&Asset> {
        let (os, arch) = current_platform();
        select_asset(&self.assets, os, arch)
    }
}

/// State of a requested install. The prompt renders this verbatim.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Install {
    #[default]
    Idle,
    /// Downloading the archive and its checksum, then verifying.
    Downloading,
    /// The helper is launched; the app is about to quit and restart.
    Ready,
    Failed(String),
}

impl Install {
    pub fn is_busy(&self) -> bool {
        matches!(self, Install::Downloading | Install::Ready)
    }
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
            Check::UpToDate => format!("Kino Scenarist {CURRENT} is the latest release."),
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
    /// Install request for `prompt`, if the writer pressed Install update.
    pub install: Install,
    rx: Option<Receiver<Result<Check, String>>>,
    install_rx: Option<Receiver<Result<Staged, String>>>,
    /// The on-open check runs once per launch, not on every frame.
    startup_done: bool,
    /// Tests exercise the buttons without spawning `curl`.
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

    /// Download, verify and stage the release, then launch the post-exit
    /// helper. Safe to call twice; a request already in flight wins.
    pub fn start_install(&mut self, release: Release) {
        if self.install.is_busy() {
            return;
        }
        self.install = Install::Downloading;
        if self.skip_network_for_test {
            return;
        }
        let (tx, rx) = mpsc::channel();
        self.install_rx = Some(rx);
        thread::spawn(move || {
            let _ = tx.send(stage_release(&release));
        });
    }

    /// Drain the installer result. Returns true when the state changed.
    pub fn poll_install(&mut self) -> bool {
        let Some(rx) = self.install_rx.as_ref() else {
            return false;
        };
        match rx.try_recv() {
            Ok(Ok(staged)) => {
                self.install_rx = None;
                self.install = match launch_helper(&staged) {
                    Ok(()) => Install::Ready,
                    Err(e) => Install::Failed(e),
                };
                true
            }
            Ok(Err(why)) => {
                self.install_rx = None;
                self.install = Install::Failed(why);
                true
            }
            Err(TryRecvError::Empty) => false,
            Err(TryRecvError::Disconnected) => {
                self.install_rx = None;
                self.install = Install::Failed("the update download stopped unexpectedly".into());
                true
            }
        }
    }

    /// Forget the current install request. A worker that is already running
    /// finishes into a dropped channel and its staged files are reused on the
    /// next attempt.
    pub fn cancel_install(&mut self) {
        self.install_rx = None;
        self.install = Install::Idle;
    }

    /// True while a download or the helper is in flight.
    pub fn is_installing(&self) -> bool {
        self.install_rx.is_some() || self.install.is_busy()
    }

    /// Test hook: inject an install state without touching the network.
    #[doc(hidden)]
    pub fn set_install_for_test(&mut self, install: Install) {
        self.install_rx = None;
        self.install = install;
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
        #[serde(default)]
        assets: Vec<ApiAsset>,
    }

    #[derive(serde::Deserialize)]
    struct ApiAsset {
        name: Option<String>,
        browser_download_url: Option<String>,
        #[serde(default)]
        size: u64,
    }

    let parsed: ApiRelease =
        serde_json::from_str(json).map_err(|e| format!("GitHub sent an unreadable reply: {e}"))?;
    if parsed.draft || parsed.prerelease {
        return Ok(None);
    }
    let Some(tag) = parsed.tag_name else {
        return Err("the latest release has no tag".to_string());
    };
    let assets = parsed
        .assets
        .into_iter()
        .filter_map(|a| {
            let name = a.name?.trim().to_string();
            let url = a.browser_download_url?.trim().to_string();
            (!name.is_empty() && is_https(&url)).then_some(Asset {
                name,
                url,
                size: a.size,
            })
        })
        .collect();
    Ok(Some(Release {
        version: normalise_version(&tag),
        url: parsed
            .html_url
            .filter(|u| !u.trim().is_empty())
            .unwrap_or_else(|| RELEASES_PAGE.to_string()),
        assets,
    }))
}

fn is_https(url: &str) -> bool {
    url.starts_with("https://")
}

/// The signing caveat the prompt shows before installing. It differs by
/// platform because only the macOS build is signed at all (ad-hoc).
pub fn install_caveat() -> &'static str {
    if cfg!(target_os = "macos") {
        "This build is ad-hoc signed, so macOS may still ask you to confirm the downloaded app."
    } else if cfg!(target_os = "windows") {
        "This build is unsigned, so SmartScreen may still ask you to confirm the downloaded app."
    } else {
        "This build is unsigned, so your desktop may ask you to confirm the new binary."
    }
}

/// The OS/arch tags used in release asset names.
pub fn current_platform() -> (&'static str, &'static str) {
    let os = if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else {
        "linux"
    };
    let arch = if cfg!(target_arch = "aarch64") {
        "arm64"
    } else if cfg!(target_os = "linux") {
        "x86_64"
    } else {
        "x64"
    };
    (os, arch)
}

/// Pick the archive for `os`/`arch` from a release's assets. Exact
/// `os-arch` names win; an OS-only archive is the fallback (x64 builds run
/// on arm64 through Rosetta/emulation). Checksum sidecars are ignored.
pub fn select_asset<'a>(assets: &'a [Asset], os: &str, arch: &str) -> Option<&'a Asset> {
    let ext = match os {
        "macos" => ".dmg",
        "windows" => ".zip",
        "linux" => ".tar.gz",
        _ => return None,
    };
    let candidates: Vec<&Asset> = assets
        .iter()
        .filter(|a| {
            let name = a.name.to_ascii_lowercase();
            name.ends_with(ext) && !name.ends_with(&format!("{ext}.sha256")) && is_https(&a.url)
        })
        .collect();
    let exact = format!("{os}-{arch}");
    candidates
        .iter()
        .copied()
        .find(|a| a.name.to_ascii_lowercase().contains(&exact))
        .or_else(|| {
            candidates
                .iter()
                .copied()
                .find(|a| a.name.to_ascii_lowercase().contains(os))
        })
}

/// First SHA-256 digest in a `.sha256` file (`<hex>  <name>`).
pub fn parse_checksum(text: &str) -> Option<String> {
    text.split_whitespace()
        .map(|token| token.trim().to_ascii_lowercase())
        .find(|token| token.len() == 64 && token.bytes().all(|b| b.is_ascii_hexdigit()))
}

/// Hash a file and compare it with the published digest. Returns the digest
/// on success so the log can record what was verified.
pub fn verify_sha256(path: &Path, expected: &str) -> Result<String, String> {
    let mut file =
        std::fs::File::open(path).map_err(|e| format!("cannot read the downloaded update: {e}"))?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    use std::io::Read;
    loop {
        let n = file
            .read(&mut buf)
            .map_err(|e| format!("cannot read the downloaded update: {e}"))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    let actual: String = hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    if actual != expected.to_ascii_lowercase() {
        return Err(format!(
            "checksum mismatch: downloaded {actual}, release says {}",
            expected.to_ascii_lowercase()
        ));
    }
    Ok(actual)
}

/// Keep a release tag from escaping the staging folder.
fn safe_component(text: &str) -> String {
    let cleaned: String = text
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .collect();
    if cleaned.is_empty() || cleaned == "." || cleaned == ".." {
        "update".to_string()
    } else {
        cleaned
    }
}

fn staging_dir(version: &str) -> PathBuf {
    crate::app::config_dir()
        .join("updates")
        .join(safe_component(version))
}

/// A verified archive waiting for the app to quit.
#[derive(Debug)]
struct Staged {
    archive: PathBuf,
    target: PathBuf,
    log: PathBuf,
    dir: PathBuf,
}

fn install_target() -> Result<PathBuf, String> {
    let exe = std::env::current_exe()
        .map_err(|e| format!("cannot locate this Kino Scenarist build: {e}"))?;
    #[cfg(target_os = "macos")]
    {
        if let Some(app) = exe
            .ancestors()
            .find(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("app")))
        {
            return Ok(app.to_path_buf());
        }
    }
    exe.parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| "cannot locate this Kino Scenarist build".to_string())
}

/// Download the platform archive and its checksum, verify the digest and
/// leave the archive staged. Runs on a worker thread.
fn stage_release(release: &Release) -> Result<Staged, String> {
    let asset = release
        .installable_asset()
        .ok_or_else(|| format!("release {} has no build for this computer", release.version))?;
    let dir = staging_dir(&release.version);
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("cannot create the update folder {}: {e}", dir.display()))?;
    let file_name = Path::new(&asset.name)
        .file_name()
        .and_then(|n| n.to_str())
        .filter(|n| !n.is_empty())
        .unwrap_or("update-archive");
    let archive = dir.join(file_name);
    let partial = dir.join(format!("{file_name}.part"));

    let output = curl(&asset.url, Some(&partial))?;
    if !output.status.success() {
        let _ = std::fs::remove_file(&partial);
        return Err(curl_error("download", &output));
    }
    std::fs::rename(&partial, &archive)
        .map_err(|e| format!("cannot finish the update download: {e}"))?;

    let checksum_url = format!("{}.sha256", asset.url);
    let text = curl_text(&checksum_url)?;
    let expected = parse_checksum(&text)
        .ok_or_else(|| "the release checksum file is unreadable".to_string())?;
    let digest = verify_sha256(&archive, &expected)?;

    let log = dir.join("install.log");
    let _ = std::fs::write(
        &log,
        format!(
            "Kino Scenarist {} update
archive: {}
sha256: {}
target: {}
",
            release.version,
            archive.display(),
            digest,
            install_target()?.display()
        ),
    );
    Ok(Staged {
        archive,
        target: install_target()?,
        log,
        dir,
    })
}

/// Prevent Windows from allocating and flashing a console window when this GUI
/// app spawns console helpers (`curl`, `powershell`, `cmd`).
#[cfg(windows)]
pub(crate) fn hide_windows_console(cmd: &mut std::process::Command) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    cmd.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
pub(crate) fn hide_windows_console(_cmd: &mut std::process::Command) {}

fn curl(url: &str, output: Option<&Path>) -> Result<std::process::Output, String> {
    if !is_https(url) {
        return Err("refusing to download a non-https update".to_string());
    }
    let mut cmd = std::process::Command::new("curl");
    hide_windows_console(&mut cmd);
    cmd.args([
        "--fail",
        "--silent",
        "--show-error",
        "--location",
        "--max-time",
        "300",
        "--proto",
        "=https",
        "--proto-redir",
        "=https",
        "-H",
        "User-Agent: ScriptCraft",
    ]);
    if let Some(path) = output {
        cmd.arg("-o").arg(path);
    }
    cmd.arg(url)
        .output()
        .map_err(|e| format!("curl is not available: {e}"))
}

fn curl_text(url: &str) -> Result<String, String> {
    let output = curl(url, None)?;
    if !output.status.success() {
        return Err(curl_error("checksum", &output));
    }
    String::from_utf8(output.stdout).map_err(|_| "the checksum reply is not UTF-8".to_string())
}

fn curl_error(what: &str, output: &std::process::Output) -> String {
    let why = String::from_utf8_lossy(&output.stderr);
    let why = why.trim();
    if why.is_empty() {
        format!("update {what} failed ({})", output.status)
    } else {
        format!("update {what} failed: {why}")
    }
}

/// Write and start the post-exit helper. The helper waits for this pid, then
/// replaces the app and relaunches it.
fn launch_helper(staged: &Staged) -> Result<(), String> {
    let (file, script) = helper_script();
    let script_path = staged.dir.join(file);
    std::fs::write(&script_path, script)
        .map_err(|e| format!("cannot write the update helper: {e}"))?;

    #[cfg(windows)]
    let mut cmd = {
        let mut c = std::process::Command::new("powershell");
        hide_windows_console(&mut c);
        c.args([
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-WindowStyle",
            "Hidden",
            "-File",
        ]);
        c
    };
    #[cfg(not(windows))]
    let mut cmd = std::process::Command::new("sh");

    cmd.arg(&script_path)
        .arg(std::process::id().to_string())
        .arg(&staged.archive)
        .arg(&staged.target)
        .arg(&staged.log);
    cmd.spawn()
        .map(|_| ())
        .map_err(|e| format!("cannot start the update helper: {e}"))
}

/// Helper file name and script for the current platform. Paths are passed as
/// arguments so no user-controlled text is interpolated into the script.
fn helper_script() -> (&'static str, &'static str) {
    #[cfg(target_os = "macos")]
    {
        (
            "install-update.command",
            r#"#!/bin/sh
set -u
PID="$1"; DMG="$2"; TARGET="$3"; LOG="$4"
while kill -0 "$PID" 2>/dev/null; do sleep 0.2; done
{
  echo "== $(date) =="
  MP="$(mktemp -d "${TMPDIR:-/tmp}/scriptcraft-update.XXXXXX")" || exit 1
  cleanup() { hdiutil detach "$MP" -quiet >/dev/null 2>&1 || true; rmdir "$MP" 2>/dev/null || true; }
  trap cleanup EXIT
  if ! hdiutil attach "$DMG" -nobrowse -quiet -mountpoint "$MP"; then
    echo "could not mount $DMG"
    osascript -e 'display alert "Kino Scenarist update failed" message "The downloaded disk image could not be opened. Your current copy is unchanged."' >/dev/null 2>&1 || true
    open "$TARGET"
    exit 1
  fi
  SRC="$(find "$MP" -maxdepth 1 -name '*.app' -print | head -n 1)"
  if [ -z "$SRC" ]; then
    echo "no .app in $DMG"
    open "$TARGET"
    exit 1
  fi
  BACKUP="$TARGET.old.$$"
  if ! mv "$TARGET" "$BACKUP"; then
    echo "cannot replace $TARGET (permission denied)"
    osascript -e 'display alert "Kino Scenarist update failed" message "Kino Scenarist could not replace the app in this folder. Move it to Applications, or install the update manually."' >/dev/null 2>&1 || true
    open "$TARGET"
    exit 1
  fi
  if ! ditto "$SRC" "$TARGET"; then
    echo "copy failed; restoring the old app"
    rm -rf "$TARGET"
    mv "$BACKUP" "$TARGET"
    open "$TARGET"
    exit 1
  fi
  rm -rf "$BACKUP"
  xattr -dr com.apple.quarantine "$TARGET" 2>/dev/null || true
  echo "installed $TARGET"
  open "$TARGET"
} >>"$LOG" 2>&1
"#,
        )
    }
    #[cfg(target_os = "windows")]
    {
        (
            "install-update.ps1",
            r#"param([int]$ProcId, [string]$Zip, [string]$Target, [string]$Log)
Wait-Process -Id $ProcId -ErrorAction SilentlyContinue
Start-Transcript -Path $Log -Append | Out-Null
$exe = Join-Path $Target "Kino Scenarist.exe"
try {
  $temp = Join-Path $env:TEMP ("kinoscenarist-update-" + [guid]::NewGuid().ToString("N"))
  Expand-Archive -LiteralPath $Zip -DestinationPath $temp -Force
  $src = Get-ChildItem $temp -Recurse -Filter "*.exe" | Select-Object -First 1
  if (-not $src) { throw "the archive has no executable" }
  Copy-Item (Join-Path $src.DirectoryName "*") $Target -Recurse -Force
  $launched = Join-Path $Target $src.Name
  Remove-Item $temp -Recurse -Force -ErrorAction SilentlyContinue
  Start-Process -FilePath $launched
} catch {
  Write-Host $_
  if (Test-Path $exe) { Start-Process -FilePath $exe }
  exit 1
} finally {
  Stop-Transcript | Out-Null
}
"#,
        )
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        (
            "install-update.sh",
            r#"#!/bin/sh
set -u
PID="$1"; ARCHIVE="$2"; TARGET="$3"; LOG="$4"
while kill -0 "$PID" 2>/dev/null; do sleep 0.2; done
{
  echo "== $(date) =="
  TMP="$(mktemp -d "${TMPDIR:-/tmp}/scriptcraft-update.XXXXXX")" || exit 1
  trap 'rm -rf "$TMP"' EXIT
  if ! tar -xzf "$ARCHIVE" -C "$TMP"; then
    echo "could not unpack $ARCHIVE"
    "$TARGET/scriptcraft"
    exit 1
  fi
  SRC="$(find "$TMP" -maxdepth 3 -type f -name scriptcraft -print | head -n 1)"
  if [ -z "$SRC" ]; then
    echo "archive has no scriptcraft binary"
    "$TARGET/scriptcraft"
    exit 1
  fi
  if ! cp -a "$(dirname "$SRC")/." "$TARGET/"; then
    echo "cannot replace files in $TARGET"
    "$TARGET/scriptcraft"
    exit 1
  fi
  echo "installed into $TARGET"
  "$TARGET/scriptcraft" >/dev/null 2>&1 &
} >>"$LOG" 2>&1
"#,
        )
    }
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
    let mut cmd = std::process::Command::new("curl");
    hide_windows_console(&mut cmd);
    let output = cmd
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
    let child = {
        let mut cmd = std::process::Command::new("cmd");
        hide_windows_console(&mut cmd);
        cmd.args(["/C", "start", "", url]).spawn()
    };
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
    fn parse_release_reads_the_tag_page_and_assets() {
        let json = r#"{
            "tag_name": "v0.8.7",
            "html_url": "https://github.com/MihailM17/scriptcraft/releases/tag/v0.8.7",
            "draft": false,
            "prerelease": false,
            "assets": [
                {
                    "name": "ScriptCraft-0.8.7-macos-arm64.dmg",
                    "browser_download_url": "https://github.com/x/releases/download/v0.8.7/ScriptCraft-0.8.7-macos-arm64.dmg",
                    "size": 10004508
                },
                {
                    "name": "ScriptCraft-0.8.7-linux-x86_64.tar.gz",
                    "browser_download_url": "https://github.com/x/releases/download/v0.8.7/ScriptCraft-0.8.7-linux-x86_64.tar.gz",
                    "size": 12445659
                },
                {
                    "name": "notes.txt",
                    "browser_download_url": "http://insecure.example/notes.txt"
                }
            ]
        }"#;
        let release = parse_latest(json).unwrap().unwrap();
        assert_eq!(release.version, "0.8.7");
        assert_eq!(
            release.url,
            "https://github.com/MihailM17/scriptcraft/releases/tag/v0.8.7"
        );
        assert_eq!(release.assets.len(), 2, "non-https assets are dropped");
        assert_eq!(release.assets[0].size, 10_004_508);
    }

    fn asset(name: &str) -> Asset {
        Asset {
            name: name.into(),
            url: format!("https://example.invalid/{name}"),
            size: 1,
        }
    }

    /// Every platform picks its own archive and never a checksum sidecar.
    #[test]
    fn asset_selection_matches_os_and_architecture() {
        let assets = vec![
            asset("ScriptCraft-0.8.7-macos-arm64.dmg"),
            asset("ScriptCraft-0.8.7-macos-arm64.dmg.sha256"),
            asset("ScriptCraft-0.8.7-windows-x64.zip"),
            asset("ScriptCraft-0.8.7-linux-x86_64.tar.gz"),
            asset("ScriptCraft-0.8.7-linux-x86_64.tar.gz.sha256"),
        ];
        assert_eq!(
            select_asset(&assets, "macos", "arm64").unwrap().name,
            "ScriptCraft-0.8.7-macos-arm64.dmg"
        );
        assert_eq!(
            select_asset(&assets, "windows", "x64").unwrap().name,
            "ScriptCraft-0.8.7-windows-x64.zip"
        );
        assert_eq!(
            select_asset(&assets, "linux", "x86_64").unwrap().name,
            "ScriptCraft-0.8.7-linux-x86_64.tar.gz"
        );
        assert!(
            select_asset(&assets, "macos", "x64").is_some(),
            "OS fallback"
        );
        assert!(
            select_asset(&assets, "windows", "arm64").is_some(),
            "OS fallback"
        );
        assert!(select_asset(&[], "macos", "arm64").is_none());
        assert!(select_asset(&assets, "plan9", "x64").is_none());
    }

    /// The prompt must not tell Windows or Linux users that their build is
    /// ad-hoc signed; only macOS gets that sentence.
    #[test]
    fn install_caveat_names_the_right_signing_story() {
        let caveat = install_caveat();
        if cfg!(target_os = "macos") {
            assert!(caveat.contains("ad-hoc signed"));
        } else if cfg!(target_os = "windows") {
            assert!(caveat.contains("SmartScreen"));
            assert!(!caveat.contains("ad-hoc"));
        } else {
            assert!(caveat.contains("unsigned"));
            assert!(!caveat.contains("ad-hoc"));
        }
    }

    #[test]
    fn checksum_parser_reads_the_published_format() {
        let digest = "613a36828926fee1c33ba34cc851da3db0d99e4691d9a86989c618d09db1e7a4";
        assert_eq!(
            parse_checksum(&format!("{digest}  ScriptCraft-0.8.7-windows-x64.zip\n")),
            Some(digest.to_string())
        );
        assert_eq!(parse_checksum("not a checksum"), None);
        assert_eq!(parse_checksum(&format!("{}  x", "z".repeat(64))), None);
    }

    #[test]
    fn sha256_verification_accepts_the_matching_file_only() {
        let dir = std::env::temp_dir().join(format!("scriptcraft-sha-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("payload.bin");
        std::fs::write(&file, b"abc").unwrap();
        // SHA-256("abc")
        let digest = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
        assert_eq!(verify_sha256(&file, digest).unwrap(), digest);
        assert!(verify_sha256(&file, &"0".repeat(64)).is_err());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn helper_script_waits_for_the_app_and_takes_paths_as_arguments() {
        let (name, script) = helper_script();
        assert!(name.contains("install-update"), "{name}");
        assert!(script.contains("$1") || script.contains("$ProcId"));
        assert!(
            script.contains("kill -0") || script.contains("Wait-Process"),
            "the helper must wait for this process to exit"
        );
        assert!(
            !script.contains("{}"),
            "paths must be arguments, not interpolated"
        );
    }

    /// The generated helper runs after ScriptCraft exits, so a syntax error
    /// would only show up after the app was already gone. Ask the platform
    /// shell to parse it instead.
    #[cfg(unix)]
    #[test]
    fn helper_script_is_valid_shell_syntax() {
        let (name, script) = helper_script();
        assert!(
            script.starts_with("#!/bin/sh"),
            "{name} must declare its interpreter"
        );
        let dir = std::env::temp_dir().join(format!("scriptcraft-helper-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        std::fs::write(&path, script).unwrap();
        let out = std::process::Command::new("sh")
            .args(["-n"])
            .arg(&path)
            .output()
            .expect("sh must be available on a unix build");
        assert!(
            out.status.success(),
            "generated {name} does not parse: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(
            script.contains("\"$1\"") && script.contains("\"$4\""),
            "every path argument must stay quoted"
        );
        std::fs::remove_dir_all(&dir).ok();
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
        let up = Check::Available(Release::without_assets("0.8.7", "https://example.invalid"));
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
        state.set_for_test(Check::Available(Release::without_assets(
            "9.9.9",
            RELEASES_PAGE,
        )));
        assert!(!state.is_running());
        assert_eq!(state.prompt.as_ref().unwrap().version, "9.9.9");
        assert!(!state.poll(), "nothing to drain");
    }
}
