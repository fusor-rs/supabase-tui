use semver::Version;
use serde::Deserialize;
use std::{
    env::{self, VarError},
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

const COMMAND: &str = env!("CARGO_BIN_NAME");
const PACKAGE: &str = env!("CARGO_PKG_NAME");
const INSTALLER: &str = include_str!("../install.sh");
/// The release workflow attaches this after the binaries and the crate are published.
const READY_ASSET: &str = "install.sh";
/// Points the release lookup at another server, for checking upgrades before a release.
const RELEASE_URL_VARIABLE: &str = "SUPABASE_TUI_RELEASE_URL";
const INSTALL_VARIABLE: &str = "SUPABASE_TUI_INSTALL";
const RELEASE_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    draft: bool,
    prerelease: bool,
    assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct Asset {
    name: String,
}

/// How this copy was installed, and so how to replace it.
enum Installation {
    Cargo(PathBuf),
    Installer(PathBuf),
}

pub(crate) fn run() -> Result<()> {
    let current = Version::parse(env!("CARGO_PKG_VERSION"))?;
    let Some(version) = latest(&current)? else {
        println!("{COMMAND} {current} is already up to date.");
        return Ok(());
    };
    let executable = fs::canonicalize(env::current_exe()?)?;
    let installation = installation(&executable)?;
    println!("Upgrading {COMMAND} {current} → {version}");
    let status = match installation {
        Installation::Cargo(root) => Command::new(cargo())
            .args(["install", PACKAGE, "--locked", "--version"])
            .arg(format!("={version}"))
            .arg("--root")
            .arg(root)
            .status()?,
        Installation::Installer(root) => installer()
            .arg(version.to_string())
            .env(INSTALL_VARIABLE, root)
            .status()?,
    };
    if !status.success() {
        return Err(
            format!("Upgrade failed ({status}); see the messages above and try again").into(),
        );
    }
    Ok(())
}

/// The newest stable release if it is newer than `current` and fully published.
fn latest(current: &Version) -> Result<Option<Version>> {
    let release = release()?;
    let version = Version::parse(release.tag_name.trim_start_matches('v'))
        .map_err(|error| format!("Invalid release version; check the release tag: {error}"))?;
    if release.draft || release.prerelease || !version.pre.is_empty() {
        return Err("The latest release is not stable; try again after it is published".into());
    }
    if !version.cmp_precedence(current).is_gt() {
        return Ok(None);
    }
    let output = installer()
        .arg("--archive")
        .arg(version.to_string())
        .output()?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr)
            .trim()
            .to_owned()
            .into());
    }
    let archive = String::from_utf8(output.stdout)?;
    let archive = archive.trim();
    let checksum = format!("{archive}.sha256");
    for required in [READY_ASSET, archive, &checksum] {
        if !release.assets.iter().any(|asset| asset.name == required) {
            return Err(
                format!("Release {version} is still being published; try again later").into(),
            );
        }
    }
    Ok(Some(version))
}

fn release() -> Result<Release> {
    let repository = env!("CARGO_PKG_REPOSITORY")
        .strip_prefix("https://github.com/")
        .expect("the package repository is hosted on GitHub");
    let url = match env::var(RELEASE_URL_VARIABLE) {
        Ok(url) => url,
        Err(VarError::NotPresent) => {
            format!("https://api.github.com/repos/{repository}/releases/latest")
        }
        Err(error) => return Err(format!("Cannot read {RELEASE_URL_VARIABLE}: {error}").into()),
    };
    let client = reqwest::Client::builder()
        .user_agent(concat!(
            env!("CARGO_PKG_NAME"),
            "/",
            env!("CARGO_PKG_VERSION")
        ))
        .timeout(RELEASE_TIMEOUT)
        .build()?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime
        .block_on(async {
            client
                .get(url)
                .send()
                .await?
                .error_for_status()?
                .json::<Release>()
                .await
        })
        .map_err(|error| format!("Cannot check releases; try again later: {error}").into())
}

fn installer() -> Command {
    let mut command = Command::new("sh");
    command.args(["-c", INSTALLER, "supabase-tui-installer"]);
    command
}

fn cargo() -> std::ffi::OsString {
    // Cargo sets CARGO for the programs it runs; otherwise the one on PATH is used.
    env::var_os("CARGO").unwrap_or_else(|| "cargo".into())
}

/// Finds the root `executable` was installed under: `<root>/bin/supatui`.
fn installation(executable: &Path) -> Result<Installation> {
    let not_installed = || {
        format!(
            "{} was not installed by Cargo or install.sh; reinstall it to upgrade",
            executable.display()
        )
    };
    let Some(root) = executable
        .parent()
        .filter(|directory| directory.ends_with("bin"))
        .and_then(Path::parent)
        .filter(|_| executable.file_name() == Some(OsStr::new(COMMAND)))
    else {
        return Err(not_installed().into());
    };
    if root.join(".crates.toml").try_exists()? && cargo_lists(root)? {
        return Ok(Installation::Cargo(root.to_owned()));
    }
    Ok(Installation::Installer(root.to_owned()))
}

/// Whether Cargo installed this package under `root`.
fn cargo_lists(root: &Path) -> Result<bool> {
    let output = Command::new(cargo())
        .args(["install", "--list", "--root"])
        .arg(root)
        .output()
        .map_err(|error| {
            format!("Cannot inspect the Cargo installation; install Cargo: {error}")
        })?;
    if !output.status.success() {
        return Err(format!(
            "Cannot inspect the Cargo installation; check Cargo and try again: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )
        .into());
    }
    let package = format!("{PACKAGE} v");
    Ok(String::from_utf8(output.stdout)?
        .lines()
        .any(|line| line.starts_with(&package)))
}
