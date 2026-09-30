//! Checks GitHub for a newer release and installs it after verifying its minisign signature.
//!
//! The Mac app downloads a zip with the system `curl`, unpacks it with `ditto`, swaps its own
//! `.app` bundle and relaunches. The Windows app downloads its new `.exe` with Windows' own
//! `curl.exe`, moves itself aside (Windows lets a running program be renamed but not
//! overwritten) and starts the new copy. The Linux app downloads its new program with `curl`
//! and renames it over itself (the running copy keeps its old file until it exits). Other
//! builds don't update themselves.

use std::fmt;
use std::path::{Path, PathBuf};
use std::process::Command;

use minisign_verify::{PublicKey, Signature};
use serde::{Deserialize, Serialize};

use crate::error::Error;

const REPO: &str = "willmccallion/instat-analysis";
/// Release signing key; the secret half lives only on the maintainer's machine.
const PUBLIC_KEY: &str = "RWTxOXCn0SiJjkAsQAVAkTqw1AdcEbblc4yELiY1bBQhH935ZXLevaLB";
const BUNDLE_NAME: &str = "Hockey Stats.app";
const CHECK_TIMEOUT_SECONDS: &str = "5";
const DOWNLOAD_TIMEOUT_SECONDS: &str = "300";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version {
    major: u32,
    minor: u32,
    patch: u32,
}

const fn parse_number(text: &str) -> u32 {
    let bytes = text.as_bytes();
    let mut value = 0;
    let mut i = 0;
    while i < bytes.len() {
        value = value * 10 + (bytes[i] - b'0') as u32;
        i += 1;
    }
    value
}

impl Version {
    pub const CURRENT: Self = Self {
        major: parse_number(env!("CARGO_PKG_VERSION_MAJOR")),
        minor: parse_number(env!("CARGO_PKG_VERSION_MINOR")),
        patch: parse_number(env!("CARGO_PKG_VERSION_PATCH")),
    };

    /// Parses `1.2.3` or `v1.2.3`.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let mut parts = text.strip_prefix('v').unwrap_or(text).split('.');
        let version = Self {
            major: parts.next()?.parse().ok()?,
            minor: parts.next()?.parse().ok()?,
            patch: parts.next()?.parse().ok()?,
        };
        parts.next().is_none().then_some(version)
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

impl Serialize for Version {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

/// The release file a build updates itself from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Package {
    /// A zip of `Hockey Stats.app`.
    MacZip,
    /// The app itself, a single `.exe`.
    WindowsExe,
    /// The app itself, a static x86-64 program.
    LinuxBinary,
}

impl Package {
    /// `None` for builds that don't update themselves (other operating systems or chips).
    #[must_use]
    pub const fn for_this_build() -> Option<Self> {
        if cfg!(target_os = "macos") {
            Some(Self::MacZip)
        } else if cfg!(windows) {
            Some(Self::WindowsExe)
        } else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
            Some(Self::LinuxBinary)
        } else {
            None
        }
    }

    /// The release asset's file name; the signature is this plus `.minisig`.
    #[must_use]
    pub const fn asset(self) -> &'static str {
        match self {
            Self::MacZip => "Hockey-Stats-mac.zip",
            Self::WindowsExe => "Hockey-Stats-windows.exe",
            Self::LinuxBinary => "Hockey-Stats-linux",
        }
    }
}

/// A published release with the files an update needs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Release {
    pub version: Version,
    pub notes: String,
    #[serde(skip)]
    package: Package,
    #[serde(skip)]
    download_url: String,
    #[serde(skip)]
    signature_url: String,
}

#[derive(Deserialize)]
struct GithubRelease {
    tag_name: String,
    #[serde(default)]
    body: Option<String>,
    assets: Vec<GithubAsset>,
}

#[derive(Deserialize)]
struct GithubAsset {
    name: String,
    browser_download_url: String,
}

/// Reads GitHub's "latest release" response, taking `package`'s file and signature.
pub fn parse_release(json: &[u8], package: Package) -> Result<Release, Error> {
    let release: GithubRelease = serde_json::from_slice(json)?;
    let version = Version::parse(&release.tag_name)
        .ok_or_else(|| Error::Update(format!("release tag {} is not a version", release.tag_name)))?;
    let url = |name: &str| {
        release
            .assets
            .iter()
            .find(|a| a.name == name)
            .map(|a| a.browser_download_url.clone())
            .ok_or_else(|| Error::Update(format!("release {version} has no {name}")))
    };
    Ok(Release {
        version,
        package,
        download_url: url(package.asset())?,
        signature_url: url(&format!("{}.minisig", package.asset()))?,
        notes: release.body.unwrap_or_default(),
    })
}

/// What the page shows about updates.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "status")]
pub enum UpdateStatus {
    Checking,
    UpToDate,
    Available { release: Release },
    /// Offline, GitHub unreachable, or this build can't update itself; the page stays quiet.
    Unavailable { reason: String },
}

/// The line signed with each release file, so a signature can't be reused for another
/// version or file.
#[must_use]
pub fn trusted_comment(version: Version, package: Package) -> String {
    format!("hockey-stats {version} {}", package.asset())
}

/// Checks `archive` against `signature` made with `public_key` for `package` at `version`.
pub fn verify(public_key: &str, archive: &[u8], signature: &str, version: Version, package: Package) -> Result<(), Error> {
    let key = PublicKey::from_base64(public_key).map_err(|e| Error::Update(format!("bad release key: {e}")))?;
    let signature = Signature::decode(signature).map_err(|e| Error::Update(format!("unreadable signature: {e}")))?;
    key.verify(archive, &signature, false)
        .map_err(|e| Error::Update(format!("the download failed its signature check ({e})")))?;
    if signature.trusted_comment() != trusted_comment(version, package) {
        return Err(Error::Update(format!("the download is signed for \"{}\", not version {version}", signature.trusted_comment())));
    }
    Ok(())
}

/// The system's own `curl`: Windows 10 and later ship one in System32, macOS in /usr/bin,
/// and Linux distributions put it on the `PATH`.
fn curl_program() -> PathBuf {
    if cfg!(windows) {
        let system_root = std::env::var_os("SystemRoot").map_or_else(|| PathBuf::from(r"C:\Windows"), PathBuf::from);
        system_root.join("System32").join("curl.exe")
    } else if cfg!(target_os = "macos") {
        PathBuf::from("/usr/bin/curl")
    } else {
        PathBuf::from("curl")
    }
}

fn curl(args: &[&str]) -> Result<Vec<u8>, Error> {
    let output = Command::new(curl_program()).args(["-fsSL"]).args(args).output()?;
    if !output.status.success() {
        return Err(Error::Update(String::from_utf8_lossy(&output.stderr).trim().to_owned()));
    }
    Ok(output.stdout)
}

/// Asks GitHub for the latest release and compares it with this build.
#[must_use]
pub fn check() -> UpdateStatus {
    let Some(package) = Package::for_this_build() else {
        return UpdateStatus::Unavailable { reason: "this build doesn't install updates".to_owned() };
    };
    let url = format!("https://api.github.com/repos/{REPO}/releases/latest");
    let latest = curl(&["--max-time", CHECK_TIMEOUT_SECONDS, "-H", "Accept: application/vnd.github+json", &url])
        .and_then(|json| parse_release(&json, package));
    match latest {
        Ok(release) if release.version > Version::CURRENT => UpdateStatus::Available { release },
        Ok(_) => UpdateStatus::UpToDate,
        Err(e) => UpdateStatus::Unavailable { reason: e.to_string() },
    }
}

/// The `.app` bundle this executable runs from (`…/Hockey Stats.app/Contents/MacOS/hockey-stats`).
fn bundle_of(executable: &Path) -> Option<PathBuf> {
    let bundle = executable.parent()?.parent()?.parent()?;
    bundle.extension().is_some_and(|e| e == "app").then(|| bundle.to_path_buf())
}

/// macOS runs apps opened straight from a download from a read-only copy.
fn is_translocated(bundle: &Path) -> bool {
    bundle.components().any(|c| c.as_os_str() == "AppTranslocation")
}

fn run(program: &str, args: &[&Path]) -> Result<(), Error> {
    let status = Command::new(program).args(args).status()?;
    if status.success() {
        Ok(())
    } else {
        Err(Error::Update(format!("{program} failed ({status})")))
    }
}

/// Downloads `release`'s file and checks its signature.
fn download(release: &Release) -> Result<Vec<u8>, Error> {
    let file = curl(&["--max-time", DOWNLOAD_TIMEOUT_SECONDS, &release.download_url])?;
    let signature = curl(&["--max-time", CHECK_TIMEOUT_SECONDS, &release.signature_url])?;
    verify(PUBLIC_KEY, &file, &String::from_utf8_lossy(&signature), release.version, release.package)?;
    Ok(file)
}

/// Downloads, verifies and installs `release` over the running app, then relaunches it.
pub fn install(release: &Release) -> Result<(), Error> {
    match release.package {
        Package::MacZip => install_bundle(release),
        Package::WindowsExe => install_exe(release),
        Package::LinuxBinary => install_binary(release),
    }
}

fn install_binary(release: &Release) -> Result<(), Error> {
    let executable = std::env::current_exe()?;
    let mut new_name = executable.clone().into_os_string();
    new_name.push(".new");
    let new_copy = PathBuf::from(new_name);
    std::fs::write(&new_copy, download(release)?)?;
    std::fs::set_permissions(&new_copy, std::fs::metadata(&executable)?.permissions())?;
    std::fs::rename(&new_copy, &executable)?;
    Command::new(&executable).arg("--background").spawn()?;
    Ok(())
}

/// Where a replaced Windows executable is parked until the next launch can delete it.
fn replaced_copy(executable: &Path) -> PathBuf {
    executable.with_extension("previous.exe")
}

/// Deletes the copy an update moved aside. It can still be running just after an update, so
/// failing here is expected and harmless.
pub fn remove_replaced_copy() {
    if !cfg!(windows) {
        return;
    }
    if let Ok(executable) = std::env::current_exe() {
        let _ = std::fs::remove_file(replaced_copy(&executable));
    }
}

fn install_exe(release: &Release) -> Result<(), Error> {
    let executable = std::env::current_exe()?;
    let new_copy = executable.with_extension("new.exe");
    std::fs::write(&new_copy, download(release)?)
        .map_err(|e| Error::Update(format!("couldn't save the update next to the app ({e}); move Hockey Stats to a folder you can write to, such as Documents")))?;
    let parked = replaced_copy(&executable);
    let _ = std::fs::remove_file(&parked);
    std::fs::rename(&executable, &parked)?;
    if let Err(e) = std::fs::rename(&new_copy, &executable) {
        std::fs::rename(&parked, &executable)?;
        return Err(e.into());
    }
    Command::new(&executable).spawn()?;
    Ok(())
}

fn install_bundle(release: &Release) -> Result<(), Error> {
    let executable = std::env::current_exe()?;
    let bundle = bundle_of(&executable).ok_or_else(|| Error::Update("not running from Hockey Stats.app".to_owned()))?;
    if is_translocated(&bundle) {
        return Err(Error::Update("drag Hockey Stats into your Applications folder first, then open it from there".to_owned()));
    }
    let parent = bundle.parent().ok_or_else(|| Error::Update("the app has no parent folder".to_owned()))?;
    let staging = parent.join(format!(".hockey-stats-update-{}", std::process::id()));
    let result = download_and_swap(release, &bundle, &staging);
    if let Err(e) = std::fs::remove_dir_all(&staging) {
        eprintln!("could not remove {}: {e}", staging.display());
    }
    result?;
    Command::new("/usr/bin/open").arg("-n").arg(&bundle).spawn()?;
    Ok(())
}

fn download_and_swap(release: &Release, bundle: &Path, staging: &Path) -> Result<(), Error> {
    std::fs::create_dir_all(staging)?;
    let archive = download(release)?;
    let archive_path = staging.join(Package::MacZip.asset());
    std::fs::write(&archive_path, &archive)?;
    let unpacked = staging.join("unpacked");
    run("/usr/bin/ditto", &[Path::new("-x"), Path::new("-k"), &archive_path, &unpacked])?;
    let new_bundle = unpacked.join(BUNDLE_NAME);
    if !new_bundle.join("Contents/MacOS/hockey-stats").is_file() {
        return Err(Error::Update("the download doesn't contain the app".to_owned()));
    }
    let previous = staging.join("previous.app");
    std::fs::rename(bundle, &previous)?;
    if let Err(e) = std::fs::rename(&new_bundle, bundle) {
        std::fs::rename(&previous, bundle)?;
        return Err(e.into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_order_numerically() {
        assert!(Version::parse("v1.10.0") > Version::parse("1.9.3"));
        assert_eq!(Version::parse("v2.0.1").map(|v| v.to_string()), Some("2.0.1".to_owned()));
    }

    #[test]
    fn malformed_versions_are_rejected() {
        assert_eq!(Version::parse("1.2"), None);
        assert_eq!(Version::parse("1.2.3.4"), None);
        assert_eq!(Version::parse("latest"), None);
    }

    #[test]
    fn current_version_matches_the_package() {
        assert_eq!(Version::CURRENT.to_string(), env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn release_json_yields_the_update_files() {
        let json = br#"{"tag_name":"v1.2.0","body":"Faster","assets":[
            {"name":"Hockey-Stats-mac.dmg","browser_download_url":"https://x/dmg"},
            {"name":"Hockey-Stats-mac.zip","browser_download_url":"https://x/zip"},
            {"name":"Hockey-Stats-mac.zip.minisig","browser_download_url":"https://x/sig"}]}"#;
        let release = parse_release(json, Package::MacZip).unwrap();
        assert_eq!((release.version, release.download_url.as_str(), release.signature_url.as_str()), (Version::parse("1.2.0").unwrap(), "https://x/zip", "https://x/sig"));
    }

    #[test]
    fn windows_builds_take_the_exe_and_its_signature() {
        let json = br#"{"tag_name":"v1.2.0","assets":[
            {"name":"Hockey-Stats-mac.zip","browser_download_url":"https://x/zip"},
            {"name":"Hockey-Stats-mac.zip.minisig","browser_download_url":"https://x/zipsig"},
            {"name":"Hockey-Stats-windows.exe","browser_download_url":"https://x/exe"},
            {"name":"Hockey-Stats-windows.exe.minisig","browser_download_url":"https://x/exesig"}]}"#;

        let release = parse_release(json, Package::WindowsExe).unwrap();

        assert_eq!((release.download_url.as_str(), release.signature_url.as_str()), ("https://x/exe", "https://x/exesig"));
    }

    #[test]
    fn linux_builds_take_the_program_and_its_signature() {
        let json = br#"{"tag_name":"v1.2.0","assets":[
            {"name":"Hockey-Stats-windows.exe","browser_download_url":"https://x/exe"},
            {"name":"Hockey-Stats-linux","browser_download_url":"https://x/linux"},
            {"name":"Hockey-Stats-linux.minisig","browser_download_url":"https://x/linuxsig"}]}"#;

        let release = parse_release(json, Package::LinuxBinary).unwrap();

        assert_eq!((release.download_url.as_str(), release.signature_url.as_str()), ("https://x/linux", "https://x/linuxsig"));
    }

    #[test]
    fn a_release_without_a_windows_build_offers_windows_nothing() {
        let json = br#"{"tag_name":"v1.2.0","assets":[
            {"name":"Hockey-Stats-mac.zip","browser_download_url":"https://x/zip"},
            {"name":"Hockey-Stats-mac.zip.minisig","browser_download_url":"https://x/zipsig"}]}"#;

        assert!(parse_release(json, Package::WindowsExe).is_err());
    }

    #[test]
    fn release_without_signature_is_rejected() {
        let json = br#"{"tag_name":"v1.2.0","assets":[{"name":"Hockey-Stats-mac.zip","browser_download_url":"https://x/zip"}]}"#;
        assert!(parse_release(json, Package::MacZip).is_err());
    }

    const TEST_KEY: &str = "RWRrmjdeFO8aiQV9oMWh9Al2dJLnIMsM8UiNMBuGP/YLk5UzMHvGhU94";
    const TEST_SIGNATURE: &str = "untrusted comment: signature from minisign secret key
RURrmjdeFO8aiUkNQhXcvrgLi5VOo02mor2G8/NRumKjsj/qWnfmwjRgPCUhj3k8w9GasMAqAEEHr7t9FhdxP9sJUWNdjNzTpA4=
trusted comment: hockey-stats 1.2.0 Hockey-Stats-mac.zip
7MLaJH7djywJpc/PUdJ1oNt1UkEV7ommHNrnl0TkWZMbZb/jOJ/ThEYxn0I5r52kpIA+UYjz4Ndu/+0NI/JxAA==
";
    const TEST_ARCHIVE: &[u8] = b"sample archive";

    fn v(text: &str) -> Version {
        Version::parse(text).unwrap()
    }

    #[test]
    fn correctly_signed_download_passes() {
        assert!(verify(TEST_KEY, TEST_ARCHIVE, TEST_SIGNATURE, v("1.2.0"), Package::MacZip).is_ok());
    }

    #[test]
    fn tampered_download_fails() {
        assert!(verify(TEST_KEY, b"sample archivE", TEST_SIGNATURE, v("1.2.0"), Package::MacZip).is_err());
    }

    #[test]
    fn signature_for_another_version_fails() {
        assert!(verify(TEST_KEY, TEST_ARCHIVE, TEST_SIGNATURE, v("9.0.0"), Package::MacZip).is_err());
    }

    #[test]
    fn signature_for_the_mac_zip_does_not_pass_as_the_windows_exe() {
        assert!(verify(TEST_KEY, TEST_ARCHIVE, TEST_SIGNATURE, v("1.2.0"), Package::WindowsExe).is_err());
    }

    #[test]
    fn replaced_windows_copy_sits_next_to_the_app() {
        let exe = Path::new(r"C:\Users\coach\Desktop\Hockey-Stats-windows.exe");

        assert_eq!(replaced_copy(exe), Path::new(r"C:\Users\coach\Desktop\Hockey-Stats-windows.previous.exe"));
    }

    #[test]
    fn signature_from_another_key_fails() {
        assert!(verify(PUBLIC_KEY, TEST_ARCHIVE, TEST_SIGNATURE, v("1.2.0"), Package::MacZip).is_err());
    }

    #[test]
    fn bundle_is_found_from_the_executable_path() {
        let exe = Path::new("/Applications/Hockey Stats.app/Contents/MacOS/hockey-stats");
        assert_eq!(bundle_of(exe), Some(PathBuf::from("/Applications/Hockey Stats.app")));
        assert_eq!(bundle_of(Path::new("/usr/local/bin/hockey-stats")), None);
    }

    #[test]
    fn translocated_apps_are_detected() {
        let bundle = Path::new("/private/var/folders/x/AppTranslocation/ABC/d/Hockey Stats.app");
        assert!(is_translocated(bundle));
        assert!(!is_translocated(Path::new("/Applications/Hockey Stats.app")));
    }
}
