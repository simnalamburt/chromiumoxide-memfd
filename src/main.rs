use anyhow::{Result, anyhow};
use chromiumoxide::{Browser, BrowserConfig};
use futures::StreamExt;
use tokio::time::{Duration, sleep};

#[cfg(all(
    target_os = "linux",
    any(target_arch = "x86_64", target_arch = "aarch64")
))]
const EMBEDDED_CHROMIUM: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/chromium-cache/chromium"));

#[tokio::main]
async fn main() -> Result<()> {
    #[cfg(all(
        target_os = "linux",
        any(target_arch = "x86_64", target_arch = "aarch64")
    ))]
    install_embedded_chromium()?;

    let path = find_chrome_executable().await?;
    println!("chrome found: {path}");

    let config = BrowserConfig::builder()
        .chrome_executable(path)
        .viewport(None);
    #[cfg(target_os = "macos")]
    let config = config.with_head();
    let config = config.build().map_err(|e| anyhow!("{e}"))?;

    let (mut browser, mut handler) = Browser::launch(config).await?;

    let handle = tokio::spawn(async move {
        while let Some(event) = handler.next().await {
            if event.is_err() {
                break;
            }
        }
    });

    browser.new_page("https://en.wikipedia.org").await?;

    sleep(Duration::from_secs(5)).await;

    browser.close().await?;
    browser.wait().await?;
    handle.await?;

    Ok(())
}

async fn find_chrome_executable() -> Result<String> {
    cfg_select! {
        all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64")) => {
            Ok("./chromium".to_string())
        }
        target_os = "macos" => {
            let candidates = [
                "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
                "/Applications/Chromium.app/Contents/MacOS/Chromium",
            ];
            for path in candidates {
                if tokio::fs::metadata(path).await.is_ok() {
                    return Ok(path.to_string());
                }
            }
            Err(anyhow!("No Chrome or Chromium executable found"))
        }
        _ => {
            Err(anyhow!(
                "No embedded Chrome or Chromium executable is available for this platform"
            ))
        }
    }
}

#[cfg(all(
    target_os = "linux",
    any(target_arch = "x86_64", target_arch = "aarch64")
))]
fn install_embedded_chromium() -> Result<()> {
    use std::{
        fs::{self, OpenOptions, Permissions},
        io::Write,
        os::unix::fs::{OpenOptionsExt, PermissionsExt},
        path::Path,
        time::{SystemTime, UNIX_EPOCH},
    };

    let destination = Path::new("./chromium");
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let temporary = Path::new(".").join(format!(".chromium.{}.{nonce}.tmp", std::process::id()));

    let result = (|| -> Result<()> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o755)
            .open(&temporary)?;
        file.set_permissions(Permissions::from_mode(0o755))?;
        file.write_all(EMBEDDED_CHROMIUM)?;
        file.sync_all()?;
        drop(file);

        fs::rename(&temporary, destination)?;
        Ok(())
    })();

    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}
