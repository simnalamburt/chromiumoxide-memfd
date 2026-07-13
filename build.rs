use std::{
    env,
    error::Error,
    fs::{self, File},
    io::{self, BufReader, BufWriter, Write},
    path::{Path, PathBuf},
};

struct ChromiumRelease {
    archive_url: &'static str,
    archive_hash: &'static str,
    chromium_hash: &'static str,
}

const CHROMIUM_X86_64: ChromiumRelease = ChromiumRelease {
    archive_url: "https://github.com/Sparticuz/chromium/releases/download/v149.0.0/chromium-v149.0.0-pack.x64.tar",
    archive_hash: "ff85af566f6150222c1b596481566a362a67e0f6991042552b8afd3a2f894a55",
    chromium_hash: "6208d1b64c52b93c6700c243e24905851644eaa5a67216bb8e5cfb7487506986",
};

const CHROMIUM_AARCH64: ChromiumRelease = ChromiumRelease {
    archive_url: "https://github.com/Sparticuz/chromium/releases/download/v149.0.0/chromium-v149.0.0-pack.arm64.tar",
    archive_hash: "b7e812b057a72ed2d71ee338b3ffc87841fb61f419f56a49450ec723a5761b8a",
    chromium_hash: "c2b237aac7aed620e4dd6e914b68e54176c493ef9108e7cf8706bba93c2d314a",
};

fn main() -> Result<(), Box<dyn Error>> {
    println!("cargo::rerun-if-changed=build.rs");

    if env::var("CARGO_CFG_TARGET_OS")? != "linux" {
        return Ok(());
    }
    let release = &match env::var("CARGO_CFG_TARGET_ARCH")?.as_str() {
        "x86_64" => CHROMIUM_X86_64,
        "aarch64" => CHROMIUM_AARCH64,
        _ => return Ok(()),
    };

    let out_dir = PathBuf::from(env::var_os("OUT_DIR").ok_or("OUT_DIR is not set")?);
    let cache_dir = &out_dir.join("chromium-cache");
    let archive_path = &cache_dir.join("pack.tar");
    let chromium_path = &cache_dir.join("chromium");
    fs::create_dir_all(cache_dir)?;
    println!("cargo::rerun-if-changed={}", archive_path.display());
    println!("cargo::rerun-if-changed={}", chromium_path.display());

    if !has_expected_hash(archive_path, release.archive_hash)? {
        download_archive(release, archive_path)?;
    }
    if !has_expected_hash(chromium_path, release.chromium_hash)? {
        extract_chromium(archive_path, chromium_path, release.chromium_hash)?;
    }

    Ok(())
}

fn download_archive(release: &ChromiumRelease, archive_path: &Path) -> Result<(), Box<dyn Error>> {
    println!(
        "cargo::warning=downloading Chromium from {}",
        release.archive_url
    );

    let temporary_path = temporary_path(archive_path, "download");
    let result = (|| -> Result<(), Box<dyn Error>> {
        let response = ureq::get(release.archive_url).call()?;
        let (_, body) = response.into_parts();
        let mut reader = body.into_reader();
        let mut writer = BufWriter::new(File::create(&temporary_path)?);
        io::copy(&mut reader, &mut writer)?;
        writer.flush()?;
        drop(writer);

        verify_hash(
            &temporary_path,
            release.archive_hash,
            "downloaded Chromium archive",
        )?;
        fs::rename(&temporary_path, archive_path)?;
        Ok(())
    })();

    if result.is_err() {
        let _ = fs::remove_file(&temporary_path);
    }
    result
}

fn extract_chromium(
    archive_path: &Path,
    chromium_path: &Path,
    expected_hash: &str,
) -> Result<(), Box<dyn Error>> {
    let temporary_path = temporary_path(chromium_path, "extract");
    let result = (|| -> Result<(), Box<dyn Error>> {
        let archive_file = File::open(archive_path)?;
        let mut archive = tar::Archive::new(BufReader::new(archive_file));
        let mut found_chromium = false;

        for entry in archive.entries()? {
            let mut entry = entry?;
            let path = entry.path()?;
            let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };

            if name == "chromium.br" {
                let output = File::create(&temporary_path)?;
                let mut writer = BufWriter::new(output);
                let mut decoder = brotli::Decompressor::new(&mut entry, 128 * 1024);
                io::copy(&mut decoder, &mut writer)?;
                writer.flush()?;
                found_chromium = true;
                break;
            }
        }

        if !found_chromium {
            return Err("Chromium archive is missing chromium.br".into());
        }

        verify_hash(&temporary_path, expected_hash, "extracted Chromium binary")?;
        fs::rename(&temporary_path, chromium_path)?;
        Ok(())
    })();

    if result.is_err() {
        let _ = fs::remove_file(&temporary_path);
    }
    result
}

fn has_expected_hash(path: &Path, expected: &str) -> Result<bool, Box<dyn Error>> {
    if !path.is_file() {
        return Ok(false);
    }
    Ok(blake3_hash(path)? == expected)
}

fn verify_hash(path: &Path, expected: &str, description: &str) -> Result<(), Box<dyn Error>> {
    let actual = blake3_hash(path)?;
    if actual != expected {
        return Err(format!(
            "BLAKE3 mismatch for {description}: expected {expected}, got {actual}"
        )
        .into());
    }
    Ok(())
}

fn blake3_hash(path: &Path) -> Result<String, Box<dyn Error>> {
    let mut reader = BufReader::new(File::open(path)?);
    let mut hasher = blake3::Hasher::new();
    io::copy(&mut reader, &mut hasher)?;
    Ok(hasher.finalize().to_hex().to_string())
}

fn temporary_path(destination: &Path, operation: &str) -> PathBuf {
    let file_name = destination
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("chromium");
    destination.with_file_name(format!(
        ".{file_name}.{operation}.{}.tmp",
        std::process::id()
    ))
}
