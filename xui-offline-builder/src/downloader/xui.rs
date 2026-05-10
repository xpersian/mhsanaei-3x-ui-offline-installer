use anyhow::{Context, Result};
use console::style;
use futures_util::StreamExt;
use indicatif::{ProgressBar, ProgressStyle};
use tokio::fs::File;
use tokio::io::AsyncWriteExt;

use crate::manifest::{
    Manifest, STEP_SERVICE_FILE, STEP_XUI_BINARY, STEP_XUI_SH,
};
use crate::proxy;
use crate::wizard::state::{BuildConfig, TargetOs, XuiVersion};

const GITHUB_API: &str = "https://api.github.com/repos/MHSanaei/3x-ui/releases/latest";
const GITHUB_RELEASE_BASE: &str = "https://github.com/MHSanaei/3x-ui/releases/download";
const XUI_SH_URL: &str = "https://raw.githubusercontent.com/MHSanaei/3x-ui/main/x-ui.sh";

const SERVICE_DEBIAN_URL: &str =
    "https://raw.githubusercontent.com/MHSanaei/3x-ui/main/x-ui.service.debian";
const SERVICE_RHEL_URL: &str =
    "https://raw.githubusercontent.com/MHSanaei/3x-ui/main/x-ui.service.rhel";
const SERVICE_ARCH_URL: &str =
    "https://raw.githubusercontent.com/MHSanaei/3x-ui/main/x-ui.service.arch";
const RC_ALPINE_URL: &str =
    "https://raw.githubusercontent.com/MHSanaei/3x-ui/main/x-ui.rc";

/// Download all x-ui assets: binary tarball, CLI script, service file.
/// Skips files that are already marked Done + valid in the manifest.
pub async fn download(
    config: &BuildConfig,
    out_dir: &str,
    manifest: &mut Manifest,
) -> Result<()> {
    let client = proxy::build_client(&config.proxy)?;

    // ── Resolve version ───────────────────────────────────────────────────────
    let tag = match &config.xui_version {
        XuiVersion::Latest => {
            println!(
                "  {} دریافت آخرین نسخه x-ui از GitHub...",
                style("→").cyan()
            );
            fetch_latest_tag(&client).await?
        }
        XuiVersion::Specific(t) => t.clone(),
    };
    println!(
        "  {} نسخه: {}",
        style("✓").green(),
        style(&tag).yellow().bold()
    );

    // ── Download tarball ──────────────────────────────────────────────────────
    let arch_suffix = config.arch.xui_suffix();
    let tar_name    = format!("x-ui-linux-{}.tar.gz", arch_suffix);
    let tar_dest    = format!("{}/{}", out_dir, tar_name);

    if manifest.step_is_valid(out_dir, STEP_XUI_BINARY) {
        println!("  {} x-ui binary — از قبل موجود است، رد می‌شود.", style("⏭️").dim());
    } else {
        let tar_url = format!("{}/{}/{}", GITHUB_RELEASE_BASE, tag, tar_name);
        download_with_progress(&client, &tar_url, &tar_dest, &format!("x-ui {} ({})", tag, arch_suffix))
            .await
            .context("دانلود باینری x-ui ناموفق بود")?;
        manifest
            .mark_done(out_dir, STEP_XUI_BINARY, vec![tar_name.clone()])
            .context("ذخیره manifest ناموفق بود")?;
    }

    // ── Download x-ui.sh (CLI manager) ───────────────────────────────────────
    let xui_sh_dest = format!("{}/x-ui.sh", out_dir);

    if manifest.step_is_valid(out_dir, STEP_XUI_SH) {
        println!("  {} x-ui.sh — از قبل موجود است، رد می‌شود.", style("⏭️").dim());
    } else {
        download_with_progress(&client, XUI_SH_URL, &xui_sh_dest, "x-ui.sh (CLI manager)")
            .await
            .context("دانلود x-ui.sh ناموفق بود")?;
        manifest
            .mark_done(out_dir, STEP_XUI_SH, vec!["x-ui.sh".to_string()])
            .context("ذخیره manifest ناموفق بود")?;
    }

    // ── Download service file ─────────────────────────────────────────────────
    if manifest.step_is_valid(out_dir, STEP_SERVICE_FILE) {
        println!("  {} service file — از قبل موجود است، رد می‌شود.", style("⏭️").dim());
    } else {
        let (service_url, service_filename) = resolve_service_url(&config.os);
        let service_dest = format!("{}/{}", out_dir, service_filename);
        download_with_progress(&client, service_url, &service_dest, &service_filename)
            .await
            .context("دانلود فایل service ناموفق بود")?;
        manifest
            .mark_done(out_dir, STEP_SERVICE_FILE, vec![service_filename])
            .context("ذخیره manifest ناموفق بود")?;
    }

    Ok(())
}

/// Fetch the latest release tag from GitHub API.
async fn fetch_latest_tag(client: &reqwest::Client) -> Result<String> {
    let resp: serde_json::Value = client
        .get(GITHUB_API)
        .send()
        .await?
        .json()
        .await?;

    resp["tag_name"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| anyhow::anyhow!("GitHub API پاسخ معتبری برنگرداند"))
}

/// Returns (url, filename) for the appropriate service file.
fn resolve_service_url(os: &TargetOs) -> (&'static str, String) {
    match os {
        TargetOs::Ubuntu | TargetOs::Debian => {
            (SERVICE_DEBIAN_URL, "x-ui.service".to_string())
        }
        TargetOs::Arch | TargetOs::Manjaro => {
            (SERVICE_ARCH_URL, "x-ui.service".to_string())
        }
        TargetOs::Alpine => {
            (RC_ALPINE_URL, "x-ui.rc".to_string())
        }
        _ => {
            (SERVICE_RHEL_URL, "x-ui.service".to_string())
        }
    }
}

/// Download a URL to a local file with a progress bar.
pub async fn download_with_progress(
    client: &reqwest::Client,
    url: &str,
    dest: &str,
    label: &str,
) -> Result<()> {
    let resp = client.get(url).send().await?.error_for_status()?;
    let total = resp.content_length().unwrap_or(0);

    let pb = ProgressBar::new(total);
    pb.set_style(
        ProgressStyle::with_template(
            "  {spinner:.cyan} [{elapsed_precise}] [{bar:35.cyan/blue}] {bytes}/{total_bytes} {msg}",
        )?
        .progress_chars("█▓░"),
    );
    pb.set_message(label.to_string());

    let mut file = File::create(dest).await?;
    let mut stream = resp.bytes_stream();
    let mut downloaded: u64 = 0;

    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        file.write_all(&chunk).await?;
        downloaded += chunk.len() as u64;
        pb.set_position(downloaded);
    }

    pb.finish_with_message(format!("✓ {}", label));
    Ok(())
}
