use anyhow::anyhow;
use reqwest::Client;
use serde::Deserialize;
use std::{
    fs::{self, read_dir, File},
    io::{BufReader, Write},
    path::Path,
    time::{Duration, UNIX_EPOCH},
};
use tar::Archive;
use tokio::time::sleep;
use zstd::Decoder;

const OWNER: &str = "frederik-uni";
const REPO: &str = "crates.io-dump-minfied";
const ASSET_NAME: &str = "data.tar.zst";

const STORAGE_DIR: &str = "./data";
const CURRENT_VERSION_FILE: &str = "./data/current";
static DOWNLOAD_LOCK_FILE: &str = "./data/download.lock";

#[derive(Debug, Deserialize)]
struct Release {
    tag_name: String,
    assets: Vec<Asset>,
}

#[derive(Debug, Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
}

fn check_lock_file() -> bool {
    let p = Path::new(DOWNLOAD_LOCK_FILE);
    if p.exists() {
        if let Ok(v) = p.metadata() {
            if let Ok(v) = v.created() {
                if let Ok(duration_since_epoch) = v.duration_since(UNIX_EPOCH) {
                    if duration_since_epoch > Duration::new(900, 0) {
                        if let Err(e) = fs::remove_file(DOWNLOAD_LOCK_FILE) {
                            eprintln!("Failed to delete lock file: {}", e);
                        }
                        return false;
                    }
                }
            }
        }
        true
    } else {
        false
    }
}

pub async fn download_update() -> Result<bool, anyhow::Error> {
    while check_lock_file() {
        sleep(Duration::from_secs(15 * 60)).await;
    }

    let path = Path::new(DOWNLOAD_LOCK_FILE);
    if let Some(p) = path.parent() {
        fs::create_dir_all(p)?;
    }
    File::create(DOWNLOAD_LOCK_FILE)?;
    let client = Client::new();

    let url = format!(
        "https://api.github.com/repos/{}/{}/releases/latest",
        OWNER, REPO
    );
    let release: Release = client
        .get(&url)
        .header("User-Agent", "rust-github-updater")
        .timeout(Duration::from_secs(15 * 60))
        .send()
        .await?
        .json()
        .await?;

    let latest_version = &release.tag_name;
    let current_version = fs::read_to_string(CURRENT_VERSION_FILE).unwrap_or_default();

    if current_version.trim() == latest_version.trim() {
        fs::remove_file(DOWNLOAD_LOCK_FILE)?;
        return Ok(false);
    }

    let asset = release
        .assets
        .iter()
        .find(|a| a.name == ASSET_NAME)
        .ok_or(anyhow!("Main asset not found"))?;

    let tmp_dir = Path::new(STORAGE_DIR).join("tmp");
    fs::create_dir_all(&tmp_dir)?;

    let asset_path = tmp_dir.join(ASSET_NAME);

    download(&client, &asset.browser_download_url, &asset_path).await?;

    let release_dir = Path::new(STORAGE_DIR).join(&release.tag_name);
    fs::rename(&tmp_dir.join("extracted"), &release_dir)?;

    fs::write(CURRENT_VERSION_FILE, &release.tag_name)?;
    if let Ok(folders) = read_dir(tmp_dir.parent().unwrap()) {
        for folder in folders.filter_map(|v| v.ok()).map(|v| v.path()) {
            if folder.is_dir() && folder != release_dir {
                fs::remove_dir_all(folder)?;
            }
        }
    }

    fs::remove_file(DOWNLOAD_LOCK_FILE)?;

    Ok(true)
}

async fn download(client: &Client, url: &str, out_path: &Path) -> Result<(), anyhow::Error> {
    let response = client
        .get(url)
        .header("User-Agent", "rust-github-updater")
        .timeout(Duration::from_secs(15 * 60))
        .send()
        .await?
        .error_for_status()?;

    let mut out_file = File::create(out_path)?;
    out_file.write_all(&response.bytes().await?.to_vec())?;

    let file = File::open(out_path)?;
    let reader = BufReader::new(file);
    let decoder = Decoder::new(reader)?;
    let mut archive = Archive::new(decoder);

    let extract_dir = out_path.parent().unwrap().join("extracted");
    fs::create_dir_all(&extract_dir)?;

    archive.unpack(&extract_dir)?;

    Ok(())
}
