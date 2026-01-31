//! Self-update command for wake CLI

use anyhow::{bail, Context, Result};
use flate2::read::GzDecoder;
use futures_util::StreamExt;
use indicatif::{ProgressBar, ProgressStyle};
use reqwest::Client;
use serde::Deserialize;
use std::env;
use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;
use tar::Archive;

const GITHUB_RELEASES_URL: &str =
    "https://api.github.com/repos/joemckenney/wake/releases/latest";

#[derive(Debug, Deserialize)]
struct GithubRelease {
    tag_name: String,
    assets: Vec<GithubAsset>,
}

#[derive(Debug, Deserialize)]
struct GithubAsset {
    name: String,
    browser_download_url: String,
    size: u64,
}

/// Get the current platform target triple
fn get_target_triple() -> Result<String> {
    let os = match env::consts::OS {
        "linux" => "unknown-linux-gnu",
        "macos" => "apple-darwin",
        "windows" => "pc-windows-msvc",
        other => bail!("Unsupported OS: {}", other),
    };

    let arch = match env::consts::ARCH {
        "x86_64" => "x86_64",
        "aarch64" => "aarch64",
        other => bail!("Unsupported architecture: {}", other),
    };

    Ok(format!("{}-{}", arch, os))
}

/// Parse version string, removing 'v' prefix if present
fn parse_version(version: &str) -> &str {
    version.strip_prefix('v').unwrap_or(version)
}

/// Compare two semver versions
/// Returns true if new_version > current_version
fn is_newer_version(current: &str, new: &str) -> bool {
    let current = parse_version(current);
    let new = parse_version(new);

    let parse = |v: &str| -> Vec<u32> {
        v.split('.')
            .filter_map(|s| s.parse().ok())
            .collect()
    };

    let current_parts = parse(current);
    let new_parts = parse(new);

    for i in 0..3 {
        let c = current_parts.get(i).copied().unwrap_or(0);
        let n = new_parts.get(i).copied().unwrap_or(0);
        if n > c {
            return true;
        }
        if n < c {
            return false;
        }
    }
    false
}

/// Fetch the latest release info from GitHub
async fn fetch_latest_release(client: &Client) -> Result<GithubRelease> {
    let response = client
        .get(GITHUB_RELEASES_URL)
        .header("User-Agent", "wake-cli")
        .header("Accept", "application/vnd.github.v3+json")
        .send()
        .await
        .context("Failed to fetch release info")?;

    if !response.status().is_success() {
        bail!(
            "GitHub API returned error: {} {}",
            response.status(),
            response.status().canonical_reason().unwrap_or("Unknown")
        );
    }

    response
        .json()
        .await
        .context("Failed to parse release JSON")
}

/// Find the asset matching our platform
fn find_platform_asset<'a>(release: &'a GithubRelease, target: &str) -> Option<&'a GithubAsset> {
    release
        .assets
        .iter()
        .find(|a| a.name.contains(target) && a.name.ends_with(".tar.gz"))
}

/// Download and extract the update
async fn download_and_extract(
    client: &Client,
    asset: &GithubAsset,
    dest_dir: &PathBuf,
) -> Result<PathBuf> {
    let response = client
        .get(&asset.browser_download_url)
        .header("User-Agent", "wake-cli")
        .send()
        .await
        .context("Failed to download asset")?;

    if !response.status().is_success() {
        bail!("Download failed: HTTP {}", response.status());
    }

    // Create progress bar
    let pb = ProgressBar::new(asset.size);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})")
            .unwrap()
            .progress_chars("#>-"),
    );

    // Download to temp file
    let temp_tar = dest_dir.join("wake-update.tar.gz");
    let mut file = File::create(&temp_tar).context("Failed to create temp file")?;

    let mut downloaded: u64 = 0;
    let mut stream = response.bytes_stream();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.context("Error reading download stream")?;
        file.write_all(&chunk)?;
        downloaded += chunk.len() as u64;
        pb.set_position(downloaded);
    }

    pb.finish_with_message("Download complete");
    drop(file);

    // Extract tarball
    println!("Extracting...");
    let tar_gz = File::open(&temp_tar)?;
    let tar = GzDecoder::new(tar_gz);
    let mut archive = Archive::new(tar);

    // Find the wake binary in the archive
    let extract_dir = dest_dir.join("wake-extract");
    fs::create_dir_all(&extract_dir)?;
    archive.unpack(&extract_dir)?;

    // Find the wake binary (could be in root or in a subdirectory)
    let binary_name = if cfg!(windows) { "wake.exe" } else { "wake" };
    let binary_path = find_binary_in_dir(&extract_dir, binary_name)?;

    // Clean up tarball
    fs::remove_file(&temp_tar).ok();

    Ok(binary_path)
}

/// Recursively find the binary in extracted directory
fn find_binary_in_dir(dir: &PathBuf, name: &str) -> Result<PathBuf> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() && path.file_name().map(|n| n == name).unwrap_or(false) {
            return Ok(path);
        }
        if path.is_dir() {
            if let Ok(found) = find_binary_in_dir(&path, name) {
                return Ok(found);
            }
        }
    }
    bail!("Could not find {} binary in extracted archive", name)
}

/// Perform atomic binary replacement
fn replace_binary(new_binary: &PathBuf, current_binary: &PathBuf) -> Result<()> {
    // Write to temp location first
    let temp_binary = current_binary.with_extension("tmp");

    // Copy new binary to temp
    fs::copy(new_binary, &temp_binary).context("Failed to copy new binary")?;

    // Make executable on Unix
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&temp_binary)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&temp_binary, perms)?;
    }

    // Atomic rename
    fs::rename(&temp_binary, current_binary).context("Failed to replace binary")?;

    Ok(())
}

/// Check for updates without installing
pub async fn check() -> Result<()> {
    let current_version = env!("CARGO_PKG_VERSION");
    println!("Current version: {}", current_version);
    println!("Checking for updates...");

    let client = Client::builder()
        .user_agent("wake-cli")
        .build()?;

    let release = fetch_latest_release(&client).await?;
    let latest_version = parse_version(&release.tag_name);

    if is_newer_version(current_version, latest_version) {
        println!();
        println!("New version available: {} -> {}", current_version, latest_version);
        println!("Run 'wake update' to install");
    } else {
        println!("You're on the latest version ({})", current_version);
    }

    Ok(())
}

/// Update wake to the latest version
pub async fn run() -> Result<()> {
    let current_version = env!("CARGO_PKG_VERSION");
    println!("Current version: {}", current_version);
    println!("Checking for updates...");

    let client = Client::builder()
        .user_agent("wake-cli")
        .build()?;

    let release = fetch_latest_release(&client).await?;
    let latest_version = parse_version(&release.tag_name);

    if !is_newer_version(current_version, latest_version) {
        println!("You're already on the latest version ({})", current_version);
        return Ok(());
    }

    println!();
    println!("New version available: {} -> {}", current_version, latest_version);

    // Get target triple
    let target = get_target_triple()?;
    println!("Platform: {}", target);

    // Find matching asset
    let asset = find_platform_asset(&release, &target)
        .ok_or_else(|| anyhow::anyhow!("No release found for platform: {}", target))?;

    println!("Downloading: {}", asset.name);
    println!();

    // Get current binary path
    let current_binary = env::current_exe().context("Failed to get current executable path")?;

    // Create temp directory for download
    let temp_dir = env::temp_dir().join("wake-update");
    fs::create_dir_all(&temp_dir)?;

    // Download and extract
    let new_binary = download_and_extract(&client, asset, &temp_dir).await?;

    // Replace the binary
    println!("Installing...");
    replace_binary(&new_binary, &current_binary)?;

    // Clean up
    fs::remove_dir_all(&temp_dir).ok();

    println!();
    println!("Successfully updated to version {}", latest_version);
    println!();
    println!("Tip: Run 'wake llm clean' to remove any orphaned models from previous versions.");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_version() {
        assert_eq!(parse_version("v1.2.3"), "1.2.3");
        assert_eq!(parse_version("1.2.3"), "1.2.3");
        assert_eq!(parse_version("v0.5.0"), "0.5.0");
    }

    #[test]
    fn test_is_newer_version() {
        assert!(is_newer_version("0.5.0", "0.6.0"));
        assert!(is_newer_version("0.5.0", "1.0.0"));
        assert!(is_newer_version("0.5.0", "0.5.1"));
        assert!(!is_newer_version("0.6.0", "0.5.0"));
        assert!(!is_newer_version("0.5.0", "0.5.0"));
        assert!(!is_newer_version("1.0.0", "0.9.9"));
    }

    #[test]
    fn test_is_newer_version_with_v_prefix() {
        assert!(is_newer_version("v0.5.0", "v0.6.0"));
        assert!(is_newer_version("0.5.0", "v0.6.0"));
        assert!(is_newer_version("v0.5.0", "0.6.0"));
    }

    #[test]
    fn test_get_target_triple() {
        let result = get_target_triple();
        assert!(result.is_ok());
        let triple = result.unwrap();
        assert!(triple.contains("x86_64") || triple.contains("aarch64"));
    }
}
