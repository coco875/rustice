use std::path::Path;
use std::process::Command;

fn download_file(url: &str, output: &str) -> bool {
    let status = if cfg!(target_os = "windows") || cfg!(target_os = "macos") {
        Command::new("curl")
            .args(&["-L", "-o", output, url])
            .status()
    } else {
        Command::new("wget")
            .args(&["-q", "-O", output, url])
            .status()
    };

    match status {
        Ok(status) if status.success() => true,
        Ok(status) => {
            eprintln!(
                "cargo:error=Download failed with code: {}",
                status.code().unwrap_or(-1)
            );
            false
        }
        Err(e) => {
            eprintln!("cargo:error=Error executing download command: {}", e);
            false
        }
    }
}

fn download_cedev() {
    let cedev = "./CEdev";
    if !Path::new(cedev).exists() {
        // should be improve to support windows
        if std::env::consts::OS == "linux" {
            if !download_file("https://github.com/CE-Programming/toolchain/releases/download/v15.0/CEdev-Linux.tar.gz", "CEdev-Linux.tar.gz") {
                eprintln!("cargo:error=Failed to download CEdev");
                std::process::exit(1);
            }
            let status = std::process::Command::new("tar")
                .args(&["-xzf", "CEdev-Linux.tar.gz"])
                .status()
                .expect("cargo:error=Failed to execute tar");
            if !status.success() {
                eprintln!("cargo:error=Failed to extract CEdev");
                std::process::exit(1);
            }
            let _ = std::fs::remove_file("CEdev-Linux.tar.gz");
        } else if std::env::consts::OS == "macos" {
            if std::env::consts::ARCH == "aarch64" {
                if !download_file("https://github.com/CE-Programming/toolchain/releases/download/v15.0/CEdev-macOS-arm.dmg", "CEdev-macOS.dmg") {
                    eprintln!("cargo:error=Failed to download CEdev");
                    std::process::exit(1);
                }
            } else {
                if !download_file("https://github.com/CE-Programming/toolchain/releases/download/v15.0/CEdev-macOS-intel.dmg", "CEdev-macOS.dmg") {
                    eprintln!("cargo:error=Failed to download CEdev");
                    std::process::exit(1);
                }
            }
            let status = std::process::Command::new("7z")
                .args(&["x", "CEdev-macOS.dmg", "CE Programming Toolchain/CEdev/"])
                .status()
                .expect("cargo:error=Failed to execute 7z");
            if !status.success() {
                eprintln!("cargo:error=Failed to extract CEdev");
                std::process::exit(1);
            }
            let status = std::process::Command::new("mv")
                .args(&["CE Programming Toolchain/CEdev", "."])
                .status()
                .expect("cargo:error=Failed to execute mv");
            if !status.success() {
                eprintln!("cargo:error=Failed to move CEdev");
                std::process::exit(1);
            }
            let status = std::process::Command::new("chmod")
                .args(&["-R", "+x", "CEdev/bin"])
                .status()
                .expect("cargo:error=Failed to execute chmod");
            if !status.success() {
                eprintln!("cargo:error=Failed to chmod CEdev");
                std::process::exit(1);
            }

            let _ = std::fs::remove_file("CEdev-macOS.dmg");
            let _ = std::fs::remove_dir_all("CE Programming Toolchain");
        } else if std::env::consts::OS == "windows" {
            if !download_file("https://github.com/CE-Programming/toolchain/releases/download/v15.0/CEdev-Windows.zip", "CEdev-Windows.zip") {
                eprintln!("cargo:error=Failed to download CEdev");
                std::process::exit(1);
            }
            let status = std::process::Command::new("tar")
                .args(&["-xf", "CEdev-Windows.zip"])
                .status()
                .expect("cargo:error=Failed to execute tar");
            if !status.success() {
                eprintln!("cargo:error=Failed to extract CEdev");
                std::process::exit(1);
            }
        } else {
            eprintln!("cargo:error=CEdev not found and automatic download is only supported on Linux, Macos and Windows.");
            std::process::exit(1);
        }
    }
}

fn require_current_cedev() {
    let executable = if cfg!(windows) {
        "CEdev/bin/cedev-config.exe"
    } else {
        "CEdev/bin/cedev-config"
    };
    let output = Command::new(executable)
        .arg("--version")
        .output()
        .expect("cargo:error=Failed to query the CEdev version");
    let version = String::from_utf8_lossy(&output.stdout);
    let major = version
        .trim()
        .trim_start_matches('v')
        .split('.')
        .next()
        .and_then(|value| value.parse::<u32>().ok());
    if !output.status.success() || !matches!(major, Some(15..)) {
        panic!(
            "cargo:error=CEdev 15 or newer is required, but found {}. Remove the CEdev directory and rebuild to download the current release.",
            version.trim()
        );
    }
}

fn main() {
    // check if we compile for tice
    let target = std::env::var("TARGET").unwrap();
    if target.contains("tice") {
        println!("cargo:warning=we are building for tice");
        download_cedev();
        require_current_cedev();
        let status = Command::new("rustc")
            .args(&["ti83-fake-linker.rs", "-o", "ti83-fake-linker"])
            .status()
            .expect("cargo:error=Failed to execute rustc");
        if !status.success() {
            panic!("cargo:error=Failed to build ti83-fake-linker");
        }
        println!("cargo:rerun-if-changed=ti83-fake-linker.rs");
        println!("cargo:rerun-if-changed=src/tice/wrapper.c");
        println!("cargo:rustc-link-arg-bins=src/tice/wrapper.c");
    }
}
