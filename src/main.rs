use std::fs::File;

use anyhow::{Result, anyhow};
use chromiumoxide::{Browser, BrowserConfig};
use futures::StreamExt;

#[tokio::main]
async fn main() -> Result<()> {
    let config = BrowserConfig::builder();

    let (config, _file) = cfg_select! {
        target_os = "macos" => {{
            async fn find_chrome_macos() -> Result<String> {
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

            let config = config
                .chrome_executable(find_chrome_macos().await?)
                .viewport(None)
                .with_head();

            (config, None::<File>)
        }}
        embedded_chromium => {{
            const EMBEDDED: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/chromium-cache/chromium"));

            use std::{io::Write, os::unix::io::AsRawFd};

            use rustix::fs::{MemfdFlags, Mode, SealFlags, fchmod, fcntl_add_seals, memfd_create};
            use rustix::io::Errno;

            let fd = match memfd_create("chromium", MemfdFlags::ALLOW_SEALING | MemfdFlags::EXEC) {
                Ok(fd) => fd,
                // Fallback required for Linux < 6.3 which doesn't support MFD_EXEC
                Err(Errno::INVAL) => memfd_create("chromium", MemfdFlags::ALLOW_SEALING)?,
                Err(error) => return Err(error.into()),
            };
            let mut file = File::from(fd);
            let path = format!("/proc/{}/fd/{}", std::process::id(), file.as_raw_fd());
            let seals = SealFlags::GROW | SealFlags::SHRINK | SealFlags::WRITE | SealFlags::SEAL;

            file.write_all(EMBEDDED)?;
            fchmod(&file, Mode::RWXU)?;
            fcntl_add_seals(&file, seals)?;

            let config = config
                .chrome_executable(&path)
                .arg(("browser-subprocess-path", &path[..]))
                .arg("no-zygote")
                .no_sandbox();

            (config, Some(file))
        }}
        _ => compile_error!("Unsupported platform"),
    };

    let config = config.build().map_err(|e| anyhow!("{e}"))?;

    let (mut browser, mut handler) = Browser::launch(config).await?;

    let handle = tokio::spawn(async move {
        while let Some(event) = handler.next().await {
            if let Err(error) = event {
                eprintln!("browser handler error: {error}");
                break;
            }
        }
    });

    let page = browser
        .new_page("https://en.wikipedia.org/wiki/Main_Page")
        .await?;
    let featured_article = page.find_element("#mp-tfa > p").await?;
    let featured_article_text = featured_article
        .string_property("textContent")
        .await?
        .map(|text| text.trim().to_owned())
        .filter(|text| !text.is_empty())
        .ok_or_else(|| anyhow!("Wikipedia's featured article block has no text"))?;
    println!("\nFrom today's featured article:\n\n{featured_article_text}");

    browser.close().await?;
    browser.wait().await?;
    handle.await?;

    Ok(())
}
