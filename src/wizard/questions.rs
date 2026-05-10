use anyhow::Result;
use console::style;
use dialoguer::{theme::ColorfulTheme, Confirm, Input, Select};

use super::state::*;
use crate::os_detect;
use crate::ui::prompt;

/// Ask all wizard questions and return a fully-populated BuildConfig.
pub async fn run() -> Result<BuildConfig> {
    let theme = ColorfulTheme::default();

    // ── Step 1: OS ───────────────────────────────────────────────────────────
    println!("{}", style("┌─ مرحله ۱/۷ — اطلاعات سرور هدف ─────────────────────────┐").bold().blue());
    println!();

    let os_items = vec![
        "Ubuntu",
        "Debian",
        "CentOS",
        "Fedora",
        "AlmaLinux / Rocky Linux / RHEL",
        "Alpine Linux",
        "Arch Linux / Manjaro",
        "OpenSUSE",
    ];
    let os_sel = Select::with_theme(&theme)
        .with_prompt("سیستم‌عامل سرور هدف را انتخاب کنید")
        .items(&os_items)
        .default(0)
        .interact()?;

    let os = match os_sel {
        0 => TargetOs::Ubuntu,
        1 => TargetOs::Debian,
        2 => TargetOs::CentOs,
        3 => TargetOs::Fedora,
        4 => TargetOs::AlmaLinux,
        5 => TargetOs::Alpine,
        6 => TargetOs::Arch,
        7 => TargetOs::OpenSuse,
        _ => TargetOs::Ubuntu,
    };

    let os_version_str: String = Input::with_theme(&theme)
        .with_prompt("نسخه سیستم‌عامل (اختیاری — مثلاً 22.04 یا 9)")
        .allow_empty(true)
        .interact_text()?;
    let os_version = if os_version_str.trim().is_empty() { None } else { Some(os_version_str.trim().to_string()) };

    // ── Architecture ─────────────────────────────────────────────────────────
    let arch_items = vec![
        TargetArch::Amd64.display_name(),
        TargetArch::Arm64.display_name(),
        TargetArch::Armv7.display_name(),
        TargetArch::I386.display_name(),
        TargetArch::S390x.display_name(),
    ];
    let arch_sel = Select::with_theme(&theme)
        .with_prompt("معماری CPU سرور هدف")
        .items(&arch_items)
        .default(0)
        .interact()?;

    let arch = match arch_sel {
        0 => TargetArch::Amd64,
        1 => TargetArch::Arm64,
        2 => TargetArch::Armv7,
        3 => TargetArch::I386,
        4 => TargetArch::S390x,
        _ => TargetArch::Amd64,
    };

    println!();

    // ── Step 2: Package mode ─────────────────────────────────────────────────
    println!("{}", style("┌─ مرحله ۲/۷ — نصب پکیج‌های سیستمی ──────────────────────┐").bold().blue());
    println!();

    let pkg_mode_items = vec![
        "آنلاین — سرور هدف به اینترنت دسترسی دارد",
        "آفلاین — دانلود پکیج‌ها الان و نصب بدون اینترنت",
    ];
    let pkg_mode_sel = Select::with_theme(&theme)
        .with_prompt("حالت نصب پکیج‌ها")
        .items(&pkg_mode_items)
        .default(0)
        .interact()?;

    let package_mode = if pkg_mode_sel == 1 {
        if !os_detect::supports_offline_packages(&os) {
            println!("\n  {} {}", style("⚠️").yellow(), style("این OS از آفلاین کامل پشتیبانی نمی‌کند. آنلاین استفاده می‌شود.").yellow());
            PackageMode::Online
        } else {
            PackageMode::Offline
        }
    } else {
        PackageMode::Online
    };
    println!();

    // ── Step 3: Server IP/Host ───────────────────────────────────────────────
    println!("{}", style("┌─ مرحله ۳/۷ — آدرس سرور هدف ──────────────────────────────┐").bold().blue());
    println!();
    let server_host: String = Input::with_theme(&theme)
        .with_prompt("IP یا دامنه سرور هدف (برای SSL و لینک دسترسی)")
        .interact_text()?;
    let server_host = server_host.trim().to_string();
    println!();

    // ── Step 4: x-ui version ─────────────────────────────────────────────────
    println!("{}", style("┌─ مرحله ۴/۷ — نسخه x-ui ──────────────────────────────────┐").bold().blue());
    println!();
    let ver_items = vec!["آخرین نسخه (GitHub)", "نسخه خاص"];
    let ver_sel = Select::with_theme(&theme).items(&ver_items).default(0).interact()?;
    let xui_version = if ver_sel == 0 { XuiVersion::Latest } else {
        let v: String = Input::with_theme(&theme).with_prompt("نسخه (مثلاً v2.5.1)").interact_text()?;
        XuiVersion::Specific(if v.starts_with('v') { v } else { format!("v{}", v) })
    };
    println!();

    // ── Step 5: Panel settings ───────────────────────────────────────────────
    println!("{}", style("┌─ مرحله ۵/۷ — تنظیمات پنل ────────────────────────────────┐").bold().blue());
    println!();
    let panel_port = prompt::random_port();
    let panel_username = prompt::random_string(8);
    let panel_password = prompt::random_string(10);
    let panel_web_base_path = prompt::random_string(12);

    println!("  {} پورت:      {}", style("→").green(), style(panel_port).yellow().bold());
    println!("  {} نام کاربری: {}", style("→").green(), style(&panel_username).yellow().bold());
    println!("  {} رمز عبور:  {}", style("→").green(), style(&panel_password).yellow().bold());
    println!("  {} آدرس وب:   /{}", style("→").green(), style(&panel_web_base_path).yellow().bold());
    println!("  {} آدرس دسترسی: http://{}:{}/{}", style("→").green(), style(&server_host).cyan(), style(panel_port).cyan(), style(&panel_web_base_path).cyan());
    println!();

    // ── Step 6: SSL ──────────────────────────────────────────────────────────
    println!("{}", style("┌─ مرحله ۶/۷ — تنظیمات SSL ────────────────────────────────┐").bold().blue());
    println!();
    let ssl_items = vec!["بدون SSL", "Custom SSL", "Self-Signed (پیشنهادی)"];
    let ssl_sel = Select::with_theme(&theme).items(&ssl_items).default(2).interact()?;
    let ssl = match ssl_sel {
        0 => SslConfig::None,
        1 => {
            let cert: String = Input::with_theme(&theme).with_prompt("مسیر fullchain").interact_text()?;
            let key:  String = Input::with_theme(&theme).with_prompt("مسیر privkey").interact_text()?;
            SslConfig::Custom { fullchain_path: cert.trim().into(), privkey_path: key.trim().into() }
        }
        _ => SslConfig::SelfSigned { common_name: server_host.clone() },
    };
    println!();

    // ── Step 7: Output Kind ──────────────────────────────────────────────────
    println!("{}", style("┌─ مرحله ۷/۷ — نوع فایل خروجی ─────────────────────────────┐").bold().blue());
    println!();
    let out_items = vec![
        "Self-Extracting (.sh) — یک فایل واحد، انتقال بسیار راحت (پیشنهادی)",
        "پوشه معمولی — شامل تمام فایل‌ها به صورت جداگانه",
    ];
    let out_sel = Select::with_theme(&theme).with_prompt("چگونه بسته را دریافت می‌کنید؟").items(&out_items).default(0).interact()?;
    let output_kind = if out_sel == 0 { OutputKind::Sfx } else { OutputKind::Folder };

    let output_dir: String = Input::with_theme(&theme).with_prompt("مسیر ذخیره‌سازی").default("./xui-bundle".into()).interact_text()?;

    // ── Final Confirm ────────────────────────────────────────────────────────
    println!("\n{}", style("━".repeat(50)).dim());
    let ok = Confirm::with_theme(&theme).with_prompt("آیا از تنظیمات بالا مطمئن هستید؟").default(true).interact()?;
    if !ok { anyhow::bail!("لغو شد."); }

    Ok(BuildConfig {
        os, arch, os_version, package_mode, server_host, xui_version,
        panel_port, panel_username, panel_password, panel_web_base_path,
        ssl, output_dir: output_dir.trim().to_string(), output_kind, proxy: None,
    })
}
