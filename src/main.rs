use anyhow::{Result, anyhow};
use chromiumoxide::{Browser, BrowserConfig};
use futures::StreamExt;

#[cfg(embedded_chromium)]
const EMBEDDED_CHROMIUM: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/chromium-cache/chromium"));

#[cfg(embedded_chromium)]
const CHROMIUM_MEMFD_PATH_ENV: &str = "CHROMIUM_MEMFD_PATH";

#[tokio::main]
async fn main() -> Result<()> {
    #[cfg(embedded_chromium)]
    exec_chromium_from_memfd()?;

    #[cfg(embedded_chromium)]
    let chromium_memfd = create_chromium_memfd()?;

    let path = find_chrome_executable(
        #[cfg(embedded_chromium)]
        &chromium_memfd,
    )
    .await?;
    println!("chrome found: {path}");

    let config = BrowserConfig::builder();
    #[cfg(embedded_chromium)]
    let chromium_subprocess_path = {
        use std::os::fd::AsRawFd;

        format!(
            "/proc/{}/fd/{}",
            std::process::id(),
            chromium_memfd.as_raw_fd()
        )
    };
    #[cfg(embedded_chromium)]
    let config = config
        .chrome_executable(std::env::current_exe()?)
        .env(CHROMIUM_MEMFD_PATH_ENV, &path)
        .arg(("browser-subprocess-path", chromium_subprocess_path.as_str()))
        .arg("no-zygote")
        .no_sandbox();
    #[cfg(not(embedded_chromium))]
    let config = config.chrome_executable(path);
    #[cfg(target_os = "macos")]
    let config = config.viewport(None).with_head();
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

async fn find_chrome_executable(
    #[cfg(embedded_chromium)] chromium_memfd: &std::fs::File,
) -> Result<String> {
    cfg_select! {
        embedded_chromium => {
            use std::os::fd::AsRawFd;

            Ok(format!("/proc/self/fd/{}", chromium_memfd.as_raw_fd()))
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

#[cfg(embedded_chromium)]
fn create_chromium_memfd() -> Result<std::fs::File> {
    use rustix::{
        fs::{MemfdFlags, Mode, SealFlags, fchmod, fcntl_add_seals, memfd_create},
        io::Errno,
    };
    use std::{fs::File, io::Write};

    const BASE_FLAGS: MemfdFlags = MemfdFlags::ALLOW_SEALING;
    let fd = match memfd_create("chromium", BASE_FLAGS | MemfdFlags::EXEC) {
        Ok(fd) => fd,
        Err(Errno::INVAL) => memfd_create("chromium", BASE_FLAGS)?,
        Err(error) => return Err(error.into()),
    };

    let mut file = File::from(fd);
    file.write_all(EMBEDDED_CHROMIUM)?;
    fchmod(&file, Mode::RWXU)?;
    fcntl_add_seals(
        &file,
        SealFlags::GROW | SealFlags::SHRINK | SealFlags::WRITE | SealFlags::SEAL,
    )?;

    Ok(file)
}

#[cfg(embedded_chromium)]
fn exec_chromium_from_memfd() -> Result<()> {
    use std::{os::unix::process::CommandExt, process::Command};

    let Some(path) = std::env::var_os(CHROMIUM_MEMFD_PATH_ENV) else {
        return Ok(());
    };

    let error = Command::new(path).args(std::env::args_os().skip(1)).exec();
    Err(error.into())
}
