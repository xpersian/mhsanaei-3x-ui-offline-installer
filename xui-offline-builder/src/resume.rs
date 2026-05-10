use anyhow::Result;
use console::style;
use dialoguer::{theme::ColorfulTheme, Confirm, Input, Select};
use std::fs;
use std::path::Path;

use crate::manifest::{
    Manifest, StepStatus, MANIFEST_FILE,
    STEP_INSTALL_SH, STEP_PACKAGES, STEP_SERVICE_FILE,
    STEP_SSL, STEP_XUI_BINARY, STEP_XUI_SH,
};
use crate::wizard::state::{BuildConfig, SslConfig};
use crate::generator;

// ─── Public API ──────────────────────────────────────────────────────────────

/// What the user chose to do with an existing bundle.
#[derive(Debug)]
pub enum ResumeAction {
    Continue(Manifest),
    Edited(Manifest, bool /* needs_redownload */),
    Restart,
    Exit,
}

/// Check whether a bundle already exists in `dir`.
pub fn detect_existing_bundle(dir: &str) -> Option<Manifest> {
    let manifest_path = format!("{}/{}", dir, MANIFEST_FILE);
    if !Path::new(&manifest_path).exists() {
        return None;
    }
    Manifest::load(dir).ok()
}

/// Show the resume UI and return what the user wants to do.
pub fn run_resume_mode(dir: &str, manifest: Manifest) -> Result<ResumeAction> {
    let theme = ColorfulTheme::default();

    // ── Header ───────────────────────────────────────────────────────────────
    println!("{}", style("━".repeat(54)).yellow());
    println!("{}", style("  📦  Bundle موجود پیدا شد!").yellow().bold());
    println!("{}", style("━".repeat(54)).yellow());

    if let Some(cfg) = manifest.config.as_object() {
        let get = |k: &str| cfg.get(k).and_then(|v| v.as_str()).unwrap_or("?");
        println!("  {:20} {}", style("سیستم‌عامل:").dim(),  style(get("os")).cyan());
        println!("  {:20} {}", style("معماری:").dim(),      style(get("arch")).cyan());
        println!("  {:20} {}", style("نسخه x-ui:").dim(),  style(get("xui_version")).cyan());
        println!("  {:20} {}", style("پورت پنل:").dim(),   style(get("panel_port")).yellow());
        println!("  {:20} {}", style("نام کاربری:").dim(), style(get("panel_username")).yellow());
        println!("  {:20} {}", style("SSL:").dim(),         style(get("ssl_kind")).cyan());
        println!(
            "  {:20} {}",
            style("تاریخ ساخت:").dim(),
            style(manifest.created_at.format("%Y-%m-%d %H:%M UTC").to_string()).dim()
        );
    }
    println!();

    // ── Verify ───────────────────────────────────────────────────────────────
    println!("{}", style("  🔍  تأیید صحت bundle...").bold());
    println!("{}", style("  ─────────────────────────────────────────").dim());

    let (all_ok, any_missing) = show_verify_report(dir, &manifest);

    println!("{}", style("  ─────────────────────────────────────────").dim());
    println!();

    if all_ok {
        println!("  {} همه فایل‌ها سالم هستند.", style("✅").green());
    } else {
        println!(
            "  {} برخی فایل‌ها ناموجود یا خراب هستند.",
            style("⚠️").yellow()
        );
    }
    println!();

    // ── Resume menu ───────────────────────────────────────────────────────────
    let mut menu_items: Vec<&str> = Vec::new();
    if any_missing {
        menu_items.push("ادامه دانلود — تکمیل موارد ناقص/گمشده");
    }
    menu_items.push("ویرایش تنظیمات — پورت / نام کاربری / رمز عبور / SSL");
    menu_items.push("شروع مجدد کامل — حذف bundle و شروع از ابتدا");
    menu_items.push("خروج");

    let sel = Select::with_theme(&theme)
        .with_prompt("چه کاری انجام شود؟")
        .items(&menu_items)
        .default(0)
        .interact()?;

    let mut idx = sel;
    if any_missing {
        if idx == 0 {
            return Ok(ResumeAction::Continue(manifest));
        }
        idx -= 1;
    }

    match idx {
        0 => {
            println!();
            let (updated, needs_dl) = edit_settings(dir, manifest)?;
            Ok(ResumeAction::Edited(updated, needs_dl))
        }
        1 => {
            println!();
            let ok = Confirm::with_theme(&theme)
                .with_prompt(&format!(
                    "آیا مطمئنید؟ پوشه {} کاملاً حذف خواهد شد.",
                    style(dir).yellow()
                ))
                .default(false)
                .interact()?;

            if ok {
                fs::remove_dir_all(dir)?;
                println!("  {} bundle قبلی حذف شد.", style("✓").green());
                Ok(ResumeAction::Restart)
            } else {
                Ok(ResumeAction::Exit)
            }
        }
        _ => Ok(ResumeAction::Exit),
    }
}

// ─── Verify report ────────────────────────────────────────────────────────────

pub fn show_verify_report(dir: &str, manifest: &Manifest) -> (bool, bool) {
    let steps = [
        (STEP_XUI_BINARY,   "x-ui binary (.tar.gz) "),
        (STEP_XUI_SH,       "x-ui.sh (CLI manager) "),
        (STEP_SERVICE_FILE, "فایل service/rc       "),
        (STEP_PACKAGES,     "پکیج‌های آفلاین        "),
        (STEP_SSL,          "SSL (cert/key)         "),
        (STEP_INSTALL_SH,   "install.sh             "),
    ];

    let mut all_ok      = true;
    let mut any_missing = false;

    for (key, label) in &steps {
        let step = manifest.steps.get(*key);

        // Detect "skipped" steps (Done with no files = online mode / no-ssl)
        if let Some(s) = step {
            if s.status == StepStatus::Done && s.files.is_empty() {
                let reason = if *key == STEP_PACKAGES { "حالت آنلاین" } else { "بدون SSL" };
                println!(
                    "  {} {}  {}",
                    style("⏭️").dim(),
                    style(label).dim(),
                    style(format!("رد شد ({})", reason)).dim()
                );
                continue;
            }
        }

        let valid = manifest.step_is_valid(dir, key);

        match step.map(|s| &s.status) {
            Some(StepStatus::Done) if valid => {
                println!(
                    "  {} {}  {}",
                    style("✅").green(),
                    style(label).bold(),
                    style(step.unwrap().files.join(", ")).dim()
                );
            }
            Some(StepStatus::Done) => {
                println!(
                    "  {} {}  {}",
                    style("❌").red(),
                    style(label).bold(),
                    style("فایل ناموجود یا خراب").red()
                );
                all_ok      = false;
                any_missing = true;
            }
            Some(StepStatus::Partial) => {
                let note = step.and_then(|s| s.note.as_deref()).unwrap_or("");
                println!(
                    "  {} {}  {}",
                    style("⚠️ ").yellow(),
                    style(label).bold(),
                    style(format!("ناقص — {}", note)).yellow()
                );
                all_ok      = false;
                any_missing = true;
            }
            Some(StepStatus::Failed) => {
                let note = step.and_then(|s| s.note.as_deref()).unwrap_or("ناشناخته");
                println!(
                    "  {} {}  {}",
                    style("❌").red(),
                    style(label).bold(),
                    style(format!("ناموفق — {}", note)).red()
                );
                all_ok      = false;
                any_missing = true;
            }
            _ => {
                println!(
                    "  {} {}  {}",
                    style("🔲").dim(),
                    style(label).dim(),
                    style("انجام نشده").dim()
                );
                any_missing = true;
            }
        }
    }

    (all_ok, any_missing)
}

// ─── Edit Mode ────────────────────────────────────────────────────────────────

fn edit_settings(dir: &str, mut manifest: Manifest) -> Result<(Manifest, bool)> {
    let theme = ColorfulTheme::default();

    println!("{}", style("┌─ ویرایش تنظیمات ──────────────────────────────────────────┐").bold().blue());
    println!();
    println!("  {}", style("نکته: تغییر پورت/کاربری/رمز/SSL فقط install.sh را بازسازی می‌کند.").dim());
    println!();

    let mut needs_redownload = false;

    loop {
        let edit_items = vec![
            "تغییر پورت پنل",
            "تغییر نام کاربری",
            "تغییر رمز عبور",
            "تغییر SSL",
            "بازسازی install.sh و ذخیره",
            "بازگشت",
        ];

        let sel = Select::with_theme(&theme)
            .with_prompt("چه تنظیماتی؟")
            .items(&edit_items)
            .default(4)
            .interact()?;

        match sel {
            0 => {
                let p: String = Input::with_theme(&theme)
                    .with_prompt("پورت جدید (1024-65535)")
                    .interact_text()?;
                if let Ok(n) = p.trim().parse::<u16>() {
                    if n >= 1024 {
                        if let Some(obj) = manifest.config.as_object_mut() {
                            obj.insert("panel_port".to_string(), serde_json::json!(n));
                        }
                        println!("  {} پورت → {}", style("✓").green(), n);
                    }
                }
            }
            1 => {
                let u: String = Input::with_theme(&theme)
                    .with_prompt("نام کاربری جدید")
                    .interact_text()?;
                if let Some(obj) = manifest.config.as_object_mut() {
                    obj.insert("panel_username".to_string(), serde_json::json!(u.trim()));
                }
                println!("  {} نام کاربری به‌روز شد.", style("✓").green());
            }
            2 => {
                let p: String = Input::with_theme(&theme)
                    .with_prompt("رمز عبور جدید")
                    .interact_text()?;
                if let Some(obj) = manifest.config.as_object_mut() {
                    obj.insert("panel_password".to_string(), serde_json::json!(p.trim()));
                }
                println!("  {} رمز عبور به‌روز شد.", style("✓").green());
            }
            3 => {
                let ssl_items = vec!["بدون SSL", "Self-Signed", "Custom (فایل‌های موجود)"];
                let ss = Select::with_theme(&theme)
                    .with_prompt("نوع SSL جدید")
                    .items(&ssl_items)
                    .default(0)
                    .interact()?;

                let ssl_kind = match ss {
                    0 => "none".to_string(),
                    1 => {
                        let cn: String = Input::with_theme(&theme)
                            .with_prompt("IP یا دامنه برای Self-Signed")
                            .interact_text()?;
                        needs_redownload = true;
                        format!("self-signed({})", cn.trim())
                    }
                    _ => {
                        let cert: String = Input::with_theme(&theme).with_prompt("مسیر fullchain.pem").interact_text()?;
                        let key:  String = Input::with_theme(&theme).with_prompt("مسیر privkey.pem").interact_text()?;
                        let ssl_dir = format!("{}/ssl", dir);
                        fs::create_dir_all(&ssl_dir)?;
                        fs::copy(cert.trim(), format!("{}/fullchain.pem", ssl_dir))?;
                        fs::copy(key.trim(),  format!("{}/privkey.pem",   ssl_dir))?;
                        println!("  {} فایل‌های SSL کپی شدند.", style("✓").green());
                        "custom".to_string()
                    }
                };
                if let Some(obj) = manifest.config.as_object_mut() {
                    obj.insert("ssl_kind".to_string(), serde_json::json!(ssl_kind));
                }
                if let Some(s) = manifest.steps.get_mut(STEP_SSL) {
                    s.status = StepStatus::Pending;
                    s.files.clear();
                    s.sha256.clear();
                }
                println!("  {} SSL به‌روز شد.", style("✓").green());
            }
            4 => {
                // Regenerate install.sh
                match config_from_manifest(&manifest, dir) {
                    Ok(cfg) => {
                        let rt = tokio::runtime::Handle::current();
                        rt.block_on(generator::build(&cfg))?;
                        if let Some(s) = manifest.steps.get_mut(STEP_INSTALL_SH) {
                            s.status = StepStatus::Done;
                            s.files  = vec!["install.sh".to_string()];
                        }
                        manifest.save(dir)?;
                        println!("  {} install.sh بازسازی شد.", style("✓").green());
                    }
                    Err(e) => {
                        println!("  {} خطا در بازسازی: {}", style("✗").red(), e);
                    }
                }
                break;
            }
            _ => break,
        }
    }

    manifest.save(dir)?;
    Ok((manifest, needs_redownload))
}

// ─── Config reconstruction from manifest ─────────────────────────────────────

/// Reconstruct a BuildConfig from the JSON snapshot stored in manifest.config.
pub fn config_from_manifest(manifest: &Manifest, dir: &str) -> Result<BuildConfig> {
    use crate::wizard::state::*;

    let cfg = manifest.config.as_object()
        .ok_or_else(|| anyhow::anyhow!("manifest.config ساختار نامعتبری دارد"))?;

    let get_str = |k: &str| -> String {
        cfg.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string()
    };
    let get_u64 = |k: &str| -> u64 {
        cfg.get(k).and_then(|v| v.as_u64()).unwrap_or(0)
    };

    let os = match get_str("os").as_str() {
        "Ubuntu"       => TargetOs::Ubuntu,
        "Debian"       => TargetOs::Debian,
        "CentOS"       => TargetOs::CentOs,
        "Fedora"       => TargetOs::Fedora,
        "AlmaLinux"    => TargetOs::AlmaLinux,
        "Rocky Linux"  => TargetOs::Rocky,
        "RHEL"         => TargetOs::Rhel,
        "Alpine Linux" => TargetOs::Alpine,
        "Arch Linux"   => TargetOs::Arch,
        "Manjaro"      => TargetOs::Manjaro,
        "OpenSUSE"     => TargetOs::OpenSuse,
        _              => TargetOs::Ubuntu,
    };

    let arch = match get_str("arch").as_str() {
        "arm64" => TargetArch::Arm64,
        "armv7" => TargetArch::Armv7,
        "386"   => TargetArch::I386,
        "s390x" => TargetArch::S390x,
        _       => TargetArch::Amd64,
    };

    let package_mode = if get_str("package_mode") == "offline" {
        PackageMode::Offline
    } else {
        PackageMode::Online
    };

    let xui_version = match get_str("xui_version").as_str() {
        "latest" => XuiVersion::Latest,
        v        => XuiVersion::Specific(v.to_string()),
    };

    let ssl_str = get_str("ssl_kind");
    let ssl = if ssl_str == "none" {
        SslConfig::None
    } else if ssl_str == "custom" {
        SslConfig::Custom {
            fullchain_path: format!("{}/ssl/fullchain.pem", dir),
            privkey_path:   format!("{}/ssl/privkey.pem",   dir),
        }
    } else if ssl_str.starts_with("self-signed(") {
        let cn = ssl_str
            .trim_start_matches("self-signed(")
            .trim_end_matches(')')
            .to_string();
        SslConfig::SelfSigned { common_name: cn }
    } else {
        SslConfig::None
    };

    Ok(BuildConfig {
        os,
        arch,
        os_version:          cfg.get("os_version").and_then(|v| v.as_str()).map(|s| s.to_string()),
        package_mode,
        xui_version,
        panel_port:          get_u64("panel_port") as u16,
        panel_username:      get_str("panel_username"),
        panel_password:      get_str("panel_password"),
        panel_web_base_path: get_str("panel_web_base_path"),
        ssl,
        server_host:         get_str("server_host"),
        proxy:               None,
        output_dir:          dir.to_string(),
        output_kind:         if get_str("output_kind") == "sfx" { OutputKind::Sfx } else { OutputKind::Folder },
    })
}
