use anyhow::Result;
use console::style;
use dialoguer::{theme::ColorfulTheme, Confirm, Input, Select};
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Proxy configuration provided by the user.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyConfig {
    pub kind: ProxyKind,
    /// Full proxy URL, e.g. "socks5://127.0.0.1:1080" or "http://user:pass@proxy:3128"
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ProxyKind {
    None,
    Socks5,
    Http,
}

// ─── Wizard question ──────────────────────────────────────────────────────────

/// Ask the user whether they need a proxy, and if so, which kind and address.
/// Returns None if no proxy is needed.
pub fn ask_proxy() -> Result<Option<ProxyConfig>> {
    let theme = ColorfulTheme::default();

    println!("{}", style("┌─ تنظیمات پروکسی (اختیاری) ───────────────────────────────┐").bold().blue());
    println!();
    println!(
        "  {}",
        style("اگر این سیستم برای دانلود به پروکسی نیاز دارد، اینجا تنظیم کنید.").dim()
    );
    println!(
        "  {}",
        style("راهنما: SOCKS5 برای کلاینت‌های VPN مثل Clash/V2Ray / HTTP برای Squid و مشابه.").dim()
    );
    println!();

    let needs_proxy = Confirm::with_theme(&theme)
        .with_prompt("آیا برای دانلود به پروکسی نیاز دارید؟")
        .default(false)
        .interact()?;

    if !needs_proxy {
        println!(
            "  {} بدون پروکسی — اتصال مستقیم",
            style("→").dim()
        );
        println!();
        return Ok(None);
    }

    let kind_items = vec![
        "SOCKS5  (مثال: socks5://127.0.0.1:1080)",
        "HTTP    (مثال: http://127.0.0.1:8080)",
    ];
    let kind_sel = Select::with_theme(&theme)
        .with_prompt("نوع پروکسی")
        .items(&kind_items)
        .default(0)
        .interact()?;

    let kind = if kind_sel == 0 { ProxyKind::Socks5 } else { ProxyKind::Http };
    let default_url = if kind == ProxyKind::Socks5 {
        "socks5://127.0.0.1:1080"
    } else {
        "http://127.0.0.1:8080"
    };

    let url: String = loop {
        let raw: String = Input::with_theme(&theme)
            .with_prompt("آدرس پروکسی")
            .default(default_url.to_string())
            .interact_text()?;
        let raw = raw.trim().to_string();

        // Basic validation
        if raw.starts_with("socks5://") || raw.starts_with("socks4://")
            || raw.starts_with("http://")  || raw.starts_with("https://")
        {
            break raw;
        }
        println!(
            "  {} آدرس نامعتبر است. باید با socks5:// یا http:// شروع شود.",
            style("✗").red()
        );
    };

    let cfg = ProxyConfig { kind, url };

    // Test the connection
    println!();
    println!(
        "  {} در حال تست اتصال از طریق پروکسی...",
        style("🔗").cyan()
    );

    match test_proxy(&cfg).await_or_run() {
        Ok(ms) => {
            println!(
                "  {} اتصال موفق! تأخیر: {}ms",
                style("✓").green().bold(),
                style(ms).yellow()
            );
        }
        Err(e) => {
            println!("  {} پروکسی پاسخ نداد: {}", style("✗").red(), e);
            println!();

            let choice_items = vec![
                "تلاش مجدد با همان پروکسی",
                "وارد کردن آدرس جدید",
                "ادامه بدون تأیید پروکسی (ممکن است دانلود شکست بخورد)",
                "ادامه بدون پروکسی",
            ];
            let choice = Select::with_theme(&theme)
                .with_prompt("چه کاری انجام شود؟")
                .items(&choice_items)
                .default(0)
                .interact()?;

            match choice {
                3 => {
                    println!("  {} بدون پروکسی ادامه می‌دهیم.", style("→").dim());
                    println!();
                    return Ok(None);
                }
                2 => {
                    println!(
                        "  {} ادامه با پروکسی بدون تأیید.",
                        style("⚠️").yellow()
                    );
                }
                _ => {
                    // Recurse for retry / new address
                    println!();
                    return ask_proxy();
                }
            }
        }
    }

    println!();
    Ok(Some(cfg))
}

// ─── Connection test (sync wrapper for use in the wizard) ────────────────────

struct SyncResult(Result<u64>);

/// Synchronously test the proxy by making a GET to GitHub API.
fn test_proxy(cfg: &ProxyConfig) -> SyncResult {
    let url = cfg.url.clone();
    SyncResult(std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        rt.block_on(async {
            let client = build_client_inner(&url, Duration::from_secs(8))?;
            let start = std::time::Instant::now();
            client
                .get("https://api.github.com")
                .header("User-Agent", "xui-offline-builder/0.1")
                .send()
                .await?
                .error_for_status()?;
            Ok(start.elapsed().as_millis() as u64)
        })
    })
    .join()
    .unwrap_or_else(|_| Err(anyhow::anyhow!("thread panicked"))))
}

impl SyncResult {
    fn await_or_run(self) -> Result<u64> {
        self.0
    }
}

// ─── Client builder (used by all downloaders) ────────────────────────────────

/// Build a reqwest Client, optionally routing through the given proxy.
pub fn build_client(proxy: &Option<ProxyConfig>) -> Result<reqwest::Client> {
    let proxy_url = proxy.as_ref().map(|p| p.url.as_str());
    build_client_inner(
        proxy_url.unwrap_or(""),
        Duration::from_secs(30),
    )
}

fn build_client_inner(proxy_url: &str, timeout: Duration) -> Result<reqwest::Client> {
    let mut builder = reqwest::Client::builder()
        .user_agent("xui-offline-builder/0.1")
        .timeout(timeout);

    if !proxy_url.is_empty() {
        let proxy = reqwest::Proxy::all(proxy_url)
            .map_err(|e| anyhow::anyhow!("پروکسی نامعتبر: {}", e))?;
        builder = builder.proxy(proxy);
    }

    Ok(builder.build()?)
}
