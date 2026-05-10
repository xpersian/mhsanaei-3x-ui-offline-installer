use anyhow::Result;
use console::style;
use std::fs;

use crate::os_detect::{self, PkgFormat};
use crate::wizard::state::{BuildConfig, PackageMode, SslConfig, TargetOs};

/// Render the offline install.sh from the config and write it to the bundle.
pub fn render(config: &BuildConfig) -> Result<()> {
    let script = build_script(config);
    let dest = format!("{}/install.sh", config.output_dir);
    fs::write(&dest, &script)?;

    // Make it executable on Unix
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&dest)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&dest, perms)?;
    }

    println!(
        "  {} install.sh تولید شد → {}",
        style("✓").green(),
        style(&dest).yellow().bold()
    );
    Ok(())
}

// ─── Helpers ─────────────────────────────────────────────────────────────────


fn build_script(c: &BuildConfig) -> String {
    let arch_suffix = c.arch.xui_suffix();

    // ── Package section ───────────────────────────────────────────────────────
    let pkg_section = build_pkg_section(c);
    let install_call = match c.package_mode {
        PackageMode::Online  => "install_base_online",
        PackageMode::Offline => "install_base_offline",
    };

    // ── SSL section ───────────────────────────────────────────────────────────
    let ssl_section = build_ssl_section(c);
    let ssl_call = match &c.ssl {
        SslConfig::None => "# SSL غیرفعال است".to_string(),
        _               => "setup_ssl".to_string(),
    };

    // ── Service management (Alpine vs systemd) ────────────────────────────────
    let is_alpine = c.os == TargetOs::Alpine;
    let (service_stop, service_install) = build_service_section(is_alpine);

    // ── CentOS VERSION_ID ─────────────────────────────────────────────────────
    let version_id_line = if c.os == TargetOs::CentOs {
        format!("VERSION_ID=\"{}\"", c.os_version.as_deref().unwrap_or(""))
    } else {
        String::new()
    };

    // ── Assemble final script ─────────────────────────────────────────────────
    let mut s = String::new();

    s.push_str("#!/bin/bash\n");
    s.push_str("# ============================================================\n");
    s.push_str(&format!("# 3x-ui Offline Installer — سفارشی‌شده توسط xui-offline-builder\n"));
    s.push_str(&format!("# سیستم‌عامل هدف : {}\n", c.os.display_name()));
    s.push_str(&format!("# معماری          : {}\n", arch_suffix));
    s.push_str("# ============================================================\n");
    s.push_str("set -e\n\n");

    s.push_str("red='\\033[0;31m'\n");
    s.push_str("green='\\033[0;32m'\n");
    s.push_str("blue='\\033[0;34m'\n");
    s.push_str("yellow='\\033[0;33m'\n");
    s.push_str("plain='\\033[0m'\n\n");

    s.push_str("# مسیر bundle\n");
    s.push_str("BUNDLE_DIR=\"$(cd \"$(dirname \"${BASH_SOURCE[0]}\")\" && pwd)\"\n\n");

    s.push_str("xui_folder=\"/usr/local/x-ui\"\n");
    s.push_str("xui_service=\"/etc/systemd/system\"\n");

    if !version_id_line.is_empty() {
        s.push('\n');
        s.push_str(&version_id_line);
        s.push('\n');
    }

    s.push('\n');
    s.push_str("# ── بررسی root ──────────────────────────────────────────────\n");
    s.push_str("[[ $EUID -ne 0 ]] && echo -e \"${red}خطا: این اسکریپت باید با دسترسی root اجرا شود.${plain}\" && exit 1\n\n");

    s.push_str("echo -e \"${green}شروع نصب 3x-ui (نسخه آفلاین)...${plain}\"\n\n");

    // Package functions
    s.push_str("# ── نصب پکیج‌های سیستمی ──────────────────────────────────────\n");
    s.push_str(&pkg_section);
    s.push_str("\n\n");
    s.push_str(&format!("{}\n\n", install_call));

    // Stop old service
    s.push_str("# ── متوقف کردن سرویس قبلی ────────────────────────────────────\n");
    s.push_str(&service_stop);
    s.push_str("\n");
    s.push_str("rm -rf \"$xui_folder\" 2>/dev/null || true\n\n");

    // Extract binary
    s.push_str("# ── استخراج باینری x-ui ─────────────────────────────────────\n");
    s.push_str("echo -e \"${green}نصب باینری x-ui...${plain}\"\n");
    s.push_str("mkdir -p \"$(dirname \"$xui_folder\")\"\n");
    s.push_str(&format!(
        "tar zxf \"$BUNDLE_DIR/x-ui-linux-{}.tar.gz\" -C \"$(dirname \"$xui_folder\")\"\n",
        arch_suffix
    ));
    s.push_str("mv \"$(dirname \"$xui_folder\")/x-ui\" \"$xui_folder\" 2>/dev/null || true\n");
    s.push_str("chmod +x \"$xui_folder/x-ui\"\n");
    s.push_str("chmod +x \"$xui_folder/x-ui.sh\"\n");
    if arch_suffix == "armv7" {
        s.push_str("mv \"$xui_folder/bin/xray-linux-armv7\" \"$xui_folder/bin/xray-linux-arm\" 2>/dev/null || true\n");
    }
    s.push_str("chmod +x \"$xui_folder/bin/\"* 2>/dev/null || true\n\n");

    // CLI manager
    s.push_str("# ── نصب CLI manager ──────────────────────────────────────────\n");
    s.push_str("cp \"$BUNDLE_DIR/x-ui.sh\" /usr/bin/x-ui\n");
    s.push_str("chmod +x /usr/bin/x-ui\n");
    s.push_str("mkdir -p /var/log/x-ui\n\n");

    // Panel config
    s.push_str("# ── پیکربندی پنل ─────────────────────────────────────────────\n");
    s.push_str("echo -e \"${green}پیکربندی تنظیمات پنل...${plain}\"\n");
    s.push_str(&format!(
        "\"$xui_folder/x-ui\" setting -username \"{}\" -password \"{}\" -port \"{}\" -webBasePath \"{}\" > /dev/null 2>&1\n\n",
        c.panel_username, c.panel_password, c.panel_port, c.panel_web_base_path
    ));

    // SSL
    s.push_str("# ── تنظیم SSL ──────────────────────────────────────────────\n");
    s.push_str(&ssl_section);
    s.push('\n');
    s.push_str(&ssl_call);
    s.push_str("\n\n");

    // Service install
    s.push_str("# ── نصب و فعال‌سازی سرویس ────────────────────────────────────\n");
    s.push_str("echo -e \"${green}فعال‌سازی سرویس x-ui...${plain}\"\n");
    s.push_str(&service_install);
    s.push_str("\n\n");

    // etckeeper
    s.push_str("# etckeeper compatibility\n");
    s.push_str("if [ -d \"/etc/.git\" ]; then\n");
    s.push_str("    echo \"x-ui/x-ui.db\" >> /etc/.gitignore 2>/dev/null || true\n");
    s.push_str("fi\n\n");

    // Final output
    s.push_str("echo \"\"\n");
    s.push_str("echo -e \"${green}═══════════════════════════════════════════${plain}\"\n");
    s.push_str("echo -e \"${green}        نصب 3x-ui با موفقیت انجام شد!      ${plain}\"\n");
    s.push_str("echo -e \"${green}═══════════════════════════════════════════${plain}\"\n");
    s.push_str(&format!("echo -e \"${{green}}نام کاربری: {}${{plain}}\"\n", c.panel_username));
    s.push_str(&format!("echo -e \"${{green}}رمز عبور:  {}${{plain}}\"\n", c.panel_password));
    s.push_str(&format!("echo -e \"${{green}}پورت:      {}${{plain}}\"\n", c.panel_port));
    s.push_str(&format!("echo -e \"${{green}}WebPath:   {}${{plain}}\"\n", c.panel_web_base_path));
    
    let protocol = match c.ssl { SslConfig::None => "http", _ => "https" };
    s.push_str(&format!(
        "echo -e \"${{green}}لینک دسترسی: {}://{}:{}/{}${{plain}}\"\n",
        protocol, c.server_host, c.panel_port, c.panel_web_base_path
    ));

    s.push_str("echo -e \"${yellow}⚠ این اطلاعات را ایمن نگه دارید!${plain}\"\n");

    s.push_str("echo -e \"${green}═══════════════════════════════════════════${plain}\"\n");
    s.push_str("echo \"\"\n");
    s.push_str("echo -e \"دستورات مدیریت:\"\n");
    s.push_str("echo -e \"  x-ui start / stop / restart / status / log\"\n");

    s
}

fn build_pkg_section(c: &BuildConfig) -> String {
    let online_cmd = os_detect::install_command_online(&c.os);
    match c.package_mode {
        PackageMode::Online => {
            format!("install_base_online() {{\n    {}\n}}", online_cmd)
        }
        PackageMode::Offline => {
            let fmt = os_detect::mirror_info(&c.os)
                .map(|m| m.format)
                .unwrap_or(PkgFormat::Deb);
            let offline_cmd = os_detect::install_command_offline(&c.os, &fmt);
            let mut s = String::new();
            s.push_str("install_base_offline() {\n");
            s.push_str("    echo \"نصب پکیج‌ها از bundle آفلاین...\"\n");
            s.push_str("    cd \"$BUNDLE_DIR\"\n");
            for line in offline_cmd.lines() {
                s.push_str(&format!("    {}\n", line));
            }
            s.push_str("}\n");
            s.push_str("install_base_online() {\n");
            s.push_str("    echo \"fallback آنلاین برای پکیج‌های جاافتاده...\"\n");
            s.push_str(&format!("    {} || true\n", online_cmd));
            s.push_str("}");
            s
        }
    }
}

fn build_ssl_section(c: &BuildConfig) -> String {
    match &c.ssl {
        SslConfig::None => {
            "# SSL غیرفعال — پنل روی HTTP اجرا می‌شود\n".to_string()
        }
        SslConfig::Custom { .. } | SslConfig::SelfSigned { .. } => {
            let mut s = String::new();
            s.push_str("setup_ssl() {\n");
            s.push_str("    local cert_dest=\"/root/cert/bundle\"\n");
            s.push_str("    mkdir -p \"$cert_dest\"\n");
            s.push_str("    cp \"$BUNDLE_DIR/ssl/fullchain.pem\" \"$cert_dest/fullchain.pem\"\n");
            s.push_str("    cp \"$BUNDLE_DIR/ssl/privkey.pem\"   \"$cert_dest/privkey.pem\"\n");
            s.push_str("    chmod 644 \"$cert_dest/fullchain.pem\"\n");
            s.push_str("    chmod 600 \"$cert_dest/privkey.pem\"\n");
            s.push_str("    /usr/local/x-ui/x-ui cert \\\n");
            s.push_str("        -webCert \"$cert_dest/fullchain.pem\" \\\n");
            s.push_str("        -webCertKey \"$cert_dest/privkey.pem\" > /dev/null 2>&1 || true\n");
            s.push_str("    echo \"  SSL گواهی نصب شد\"\n");
            s.push_str("}\n");
            s
        }
    }
}

fn build_service_section(is_alpine: bool) -> (String, String) {
    if is_alpine {
        let stop = "rc-service x-ui stop 2>/dev/null || true".to_string();
        let install = concat!(
            "cp \"$BUNDLE_DIR/x-ui.rc\" /etc/init.d/x-ui\n",
            "chmod +x /etc/init.d/x-ui\n",
            "rc-update add x-ui\n",
            "rc-service x-ui start"
        ).to_string();
        (stop, install)
    } else {
        let stop = "systemctl stop x-ui 2>/dev/null || true".to_string();
        let install = concat!(
            "cp \"$BUNDLE_DIR/x-ui.service\" /etc/systemd/system/x-ui.service\n",
            "chown root:root /etc/systemd/system/x-ui.service\n",
            "chmod 644 /etc/systemd/system/x-ui.service\n",
            "systemctl daemon-reload\n",
            "systemctl enable x-ui\n",
            "systemctl start x-ui"
        ).to_string();
        (stop, install)
    }
}
