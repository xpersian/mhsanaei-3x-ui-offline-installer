use anyhow::Result;
use console::style;
use rcgen::{generate_simple_self_signed, CertifiedKey};
use std::fs;

/// Copy user-provided cert files to the bundle ssl/ directory.
pub fn copy_custom(fullchain_src: &str, privkey_src: &str, out_dir: &str) -> Result<()> {
    let ssl_dir = format!("{}/ssl", out_dir);
    fs::create_dir_all(&ssl_dir)?;

    fs::copy(fullchain_src, format!("{}/fullchain.pem", ssl_dir))?;
    fs::copy(privkey_src, format!("{}/privkey.pem", ssl_dir))?;

    println!(
        "  {} فایل‌های SSL کپی شدند → {}",
        style("✓").green(),
        style(&ssl_dir).yellow()
    );
    Ok(())
}

/// Generate a self-signed certificate for the given IP or domain.
pub fn generate_self_signed(common_name: &str, out_dir: &str) -> Result<()> {
    println!(
        "  {} در حال ساخت گواهی self-signed برای {}...",
        style("→").cyan(),
        style(common_name).yellow()
    );

    let ssl_dir = format!("{}/ssl", out_dir);
    fs::create_dir_all(&ssl_dir)?;

    // Build Subject Alternative Names — support both IP and domain
    let subject_alt_names = vec![common_name.to_string()];

    let CertifiedKey { cert, key_pair } = generate_simple_self_signed(subject_alt_names)
        .map_err(|e| anyhow::anyhow!("ساخت گواهی self-signed ناموفق: {}", e))?;

    let cert_pem = cert.pem();
    let key_pem  = key_pair.serialize_pem();

    fs::write(format!("{}/fullchain.pem", ssl_dir), &cert_pem)?;
    fs::write(format!("{}/privkey.pem",   ssl_dir), &key_pem)?;

    // Secure key permissions (best-effort on Linux)
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(format!("{}/privkey.pem", ssl_dir))?.permissions();
        perms.set_mode(0o600);
        fs::set_permissions(format!("{}/privkey.pem", ssl_dir), perms)?;
    }

    println!(
        "  {} گواهی self-signed ساخته شد → {}/ssl/",
        style("✓").green(),
        out_dir
    );
    println!();
    println!("  {}", style("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━").yellow());
    println!("  {} {}", style("ℹ️  راهنمای Self-Signed Certificate:").bold(), "");
    println!("  {}", style("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━").yellow());
    println!("  • این گواهی برای استفاده شخصی مناسب است.");
    println!("  • مرورگرها هنگام باز کردن پنل هشدار امنیتی نشان می‌دهند.");
    println!("  • برای رد کردن هشدار در Chrome: روی صفحه کلیک کنید و");
    println!("    تایپ کنید: {}", style("thisisunsafe").bold().cyan());
    println!("  • در Firefox: Advanced → Accept Risk and Continue");
    println!("  {}", style("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━").yellow());
    println!();

    Ok(())
}
