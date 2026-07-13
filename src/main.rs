use anyhow::{Result, anyhow};
use chromiumoxide::{Browser, BrowserConfig};
use futures::StreamExt;
use tokio::time::{Duration, sleep};

#[tokio::main]
async fn main() -> Result<()> {
    let path = find_chrome_executable().await?;
    println!("chrome found: {path}");

    let config = BrowserConfig::builder()
        .chrome_executable(path)
        .with_head()
        .viewport(None)
        .build()
        .map_err(|e| anyhow!("{e}"))?;

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
        target_os = "linux" => {
            find_or_install_sparticuz_chromium().await
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
            Err(anyhow!("No Chrome or Chromium executable found"))
        }
    }
}
