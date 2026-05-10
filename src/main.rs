mod wizard;
mod os_detect;
mod downloader;
mod generator;
mod manifest;
mod proxy;
mod resume;
mod ui;

use anyhow::Result;
use console::style;

use manifest::{Manifest, STEP_INSTALL_SH};
use resume::{ResumeAction, detect_existing_bundle, run_resume_mode};

#[tokio::main]
async fn main() -> Result<()> {
    print_banner();

    // Default output directory (may change after wizard)
    let default_out = "./xui-bundle";

    // ── Check for existing bundle ─────────────────────────────────────────────
    if let Some(existing_manifest) = detect_existing_bundle(default_out) {
        match run_resume_mode(default_out, existing_manifest)? {
            ResumeAction::Continue(manifest) => {
                // Resume: download missing steps then regenerate install.sh
                run_download_and_generate_from_manifest(manifest, default_out).await?;
            }
            ResumeAction::Edited(manifest, needs_redownload) => {
                if needs_redownload {
                    println!(
                        "  {} Regenerating SSL files...",
                        style("→").cyan()
                    );
                    run_download_and_generate_from_manifest(manifest, default_out).await?;
                } else {
                    println!(
                        "  {} install.sh was already regenerated.",
                        style("→").green()
                    );
                    print_done(default_out);
                }
                return Ok(());
            }
            ResumeAction::Restart => {
                println!(
                    "\n  {} Restarting — full wizard starting...\n",
                    style("→").cyan()
                );
                run_full_wizard().await?;
                return Ok(());
            }
            ResumeAction::Exit => {
                println!("  {} Exiting.", style("→").dim());
                return Ok(());
            }
        }
        return Ok(());
    }

    // ── No existing bundle → full wizard ─────────────────────────────────────
    run_full_wizard().await
}

// ─── Full Wizard Flow ─────────────────────────────────────────────────────────

async fn run_full_wizard() -> Result<()> {
    // Phase 1: Collect settings
    let mut config = wizard::run().await?;

    // Phase 2: Proxy (before any download)
    println!("{}", style("━".repeat(54)).cyan());
    let proxy_cfg = proxy::ask_proxy()?;
    config.proxy = proxy_cfg;

    let out = config.output_dir.clone();
    std::fs::create_dir_all(&out)?;

    // Phase 3: Create fresh manifest
    let mut manifest = Manifest::new(&config);
    manifest.save(&out)?;

    run_download_and_generate(&config, &mut manifest, &out).await
}

// ─── Download + Generate (shared by full wizard and resume) ──────────────────

async fn run_download_and_generate(
    config: &crate::wizard::state::BuildConfig,
    manifest: &mut Manifest,
    out: &str,
) -> Result<()> {
    println!("\n{}", style("━".repeat(54)).cyan());
    println!("{}", style("  📦  Starting download of required files...").cyan().bold());
    println!("{}\n", style("━".repeat(54)).cyan());

    downloader::download_all(config, manifest).await?;

    println!("\n{}", style("━".repeat(54)).cyan());
    println!("{}", style("  ⚙️   Building offline install.sh...").cyan().bold());
    println!("{}\n", style("━".repeat(54)).cyan());

    generator::build(config).await?;

    // Mark install_sh as done
    manifest.mark_done(out, STEP_INSTALL_SH, vec!["install.sh".to_string()])?;

    print_done(out);
    Ok(())
}

/// Resume: reconstruct config from manifest and run download+generate for missing steps.
async fn run_download_and_generate_from_manifest(
    manifest: Manifest,
    out: &str,
) -> Result<()> {
    // Reconstruct a minimal config from manifest snapshot
    let config = resume::config_from_manifest(&manifest, out)?;
    let mut manifest = manifest;
    run_download_and_generate(&config, &mut manifest, out).await
}

// ─── Banner / Done ────────────────────────────────────────────────────────────

fn print_banner() {
    let version = env!("CARGO_PKG_VERSION");
    println!();
    println!("{}", style("╔══════════════════════════════════════════════════╗").cyan());
    println!("{}", style("║                                                  ║").cyan());
    println!("{}", style(format!("║      3x-ui Offline Bundle Builder - V{: <9} ║", version)).cyan().bold());
    println!("{}", style("║          Build 3x-ui Offline Bundle              ║").cyan());
    println!("{}", style("║                                                  ║").cyan());
    println!("{}", style("╚══════════════════════════════════════════════════╝").cyan());
    println!();
    println!(
        "  {}",
        style("This tool builds a complete offline installation bundle for the 3x-ui panel.").dim()
    );
    println!();
}

fn print_done(out_dir: &str) {
    println!("\n{}", style("━".repeat(54)).green());
    println!("{}", style("  ✅  Offline Bundle built successfully!").green().bold());
    println!("{}", style("━".repeat(54)).green());
    println!();
    println!(
        "  {}  {}",
        style("📁 Output Directory:").bold(),
        style(out_dir).yellow().bold()
    );
    println!(
        "  {}",
        style("To install, transfer the folder to the target server and run:").dim()
    );
    println!(
        "    {}",
        style("chmod +x install.sh && sudo bash install.sh").white().bold()
    );
    println!();
}
