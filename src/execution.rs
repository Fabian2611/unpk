use crate::error;
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::process::Command;

pub enum Format {
    Zip,
    Gzip,
    Bzip2,
    Xz,
    Tar,
    SevenZip,
    Rar,
}

fn file_err(err: std::io::Error) -> ! {
    error!("cannot open file: {}", err);
    std::process::exit(1);
}

pub fn detect_format(path: &Path) -> Format {
    let mut f = File::open(path).map_err(file_err).unwrap();
    let mut buf = [0u8; 8];
    f.read(&mut buf).map_err(file_err).unwrap();

    match buf {
        [0x50, 0x4B, 0x03, 0x04, ..] | [0x50, 0x4B, 0x05, 0x06, ..] => Format::Zip,
        [0x1F, 0x8B, ..] => Format::Gzip,
        [0x42, 0x5A, 0x68, ..] => Format::Bzip2,
        [0xFD, b'7', b'z', b'X', b'Z', 0x00, ..] => Format::Xz,
        [0x37, 0x7A, 0xBC, 0xAF, 0x27, 0x1C, ..] => Format::SevenZip,
        [0x52, 0x61, 0x72, 0x21, 0x1A, 0x07, ..] => Format::Rar,
        _ => {
            let mut tar_check = [0u8; 5];
            f.seek(SeekFrom::Start(257)).ok();
            if f.read(&mut tar_check).is_ok() && &tar_check == b"ustar" {
                return Format::Tar;
            }
            error!("unrecognized archive format: {}", path.display());
            std::process::exit(1);
        }
    }
}

fn run_err(cmd: &str, e: std::io::Error) -> ! {
    if e.kind() == std::io::ErrorKind::NotFound {
        error!("'{}' not found on PATH", cmd);
    } else {
        error!("failed to run {}: {}", cmd, e);
    }
    std::process::exit(1);
}

fn run(cmd: &str, args: &[&str]) {
    let output = Command::new(cmd)
        .args(args)
        .output()
        .unwrap_or_else(|e| run_err(cmd, e));

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        error!("{} failed: {}", cmd, stderr.trim());
        std::process::exit(1);
    }
}

fn run_capture(cmd: &str, args: &[&str]) -> Vec<String> {
    let output = Command::new(cmd)
        .args(args)
        .output()
        .unwrap_or_else(|e| run_err(cmd, e));

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        error!("{} failed: {}", cmd, stderr.trim());
        std::process::exit(1);
    }

    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|l| l.to_string().trim_end_matches('/').to_owned())
        .filter(|l| !l.is_empty())
        .collect()
}

fn check_binary(name: &str) {
    match Command::new(name).arg("--help").output() {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            error!("'{}' was not found on PATH", name);
            std::process::exit(1);
        }
        Err(e) => {
            error!("failed to check for '{}': {}", name, e);
            std::process::exit(1);
        }
    }
}

fn extract_stripped(dest: &Path, run_extract: impl FnOnce(&Path)) {
    let tmp = dest.with_file_name(format!(
        ".unpack-tmp-{}-{}",
        dest.file_name().unwrap_or_default().to_string_lossy(),
        std::process::id()
    ));
    fs::create_dir_all(&tmp).unwrap_or_else(|e| {
        error!("failed to create temp dir {}: {}", tmp.display(), e);
        std::process::exit(1);
    });

    run_extract(&tmp);

    let root_entries: Vec<PathBuf> = fs::read_dir(&tmp)
        .unwrap_or_else(|e| {
            error!("failed to read temp dir {}: {}", tmp.display(), e);
            std::process::exit(1);
        })
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .collect();

    let source_dir = if root_entries.len() == 1 && root_entries[0].is_dir() {
        root_entries.into_iter().next().unwrap()
    } else {
        tmp.clone()
    };

    for entry in fs::read_dir(&source_dir).unwrap_or_else(|e| {
        error!("failed to read {}: {}", source_dir.display(), e);
        std::process::exit(1);
    }) {
        let entry = entry.unwrap_or_else(|e| {
            error!("failed to read entry: {}", e);
            std::process::exit(1);
        });
        let target = dest.join(entry.file_name());
        fs::rename(entry.path(), &target).unwrap_or_else(|e| {
            error!(
                "failed to move {} to {}: {}",
                entry.path().display(),
                target.display(),
                e
            );
            std::process::exit(1);
        });
    }

    fs::remove_dir_all(&tmp).ok();
}

pub trait Extractor {
    fn list(&self, archive: &Path) -> Vec<String>;
    fn extract(&self, archive: &Path, dest: &Path, strip_root: bool);
}

pub struct TarBackend;
impl Extractor for TarBackend {
    fn list(&self, archive: &Path) -> Vec<String> {
        run_capture("tar", &["-tf", &archive.to_string_lossy()])
    }
    fn extract(&self, archive: &Path, dest: &Path, strip_root: bool) {
        let archive_str = archive.to_string_lossy().to_string();
        let dest_str = dest.to_string_lossy().to_string();
        let mut args: Vec<&str> = vec!["-xf", &archive_str, "-C", &dest_str];
        if strip_root {
            args.push("--strip-components=1");
        }
        run("tar", &args);
    }
}

pub struct ZipBackend;
impl Extractor for ZipBackend {
    fn list(&self, archive: &Path) -> Vec<String> {
        run_capture("unzip", &["-Z1", &archive.to_string_lossy()])
    }
    fn extract(&self, archive: &Path, dest: &Path, strip_root: bool) {
        let archive_str = archive.to_string_lossy().to_string();
        if strip_root {
            extract_stripped(dest, |tmp| {
                run("unzip", &["-q", &archive_str, "-d", &tmp.to_string_lossy()]);
            });
        } else {
            run(
                "unzip",
                &["-q", &archive_str, "-d", &dest.to_string_lossy()],
            );
        }
    }
}

pub struct SevenZipBackend;
impl Extractor for SevenZipBackend {
    fn list(&self, archive: &Path) -> Vec<String> {
        let lines = run_capture("7z", &["l", "-slt", &archive.to_string_lossy()]);
        let mut entries = Vec::new();
        let mut past_separator = false;
        for line in lines {
            if line.trim() == "----------" {
                past_separator = true;
                continue;
            }
            if past_separator && let Some(rest) = line.strip_prefix("Path = ") {
                entries.push(rest.to_string());
            }
        }
        entries
    }

    fn extract(&self, archive: &Path, dest: &Path, strip_root: bool) {
        let archive_str = archive.to_string_lossy().to_string();
        if strip_root {
            extract_stripped(dest, |tmp| {
                let dest_arg = format!("-o{}", tmp.to_string_lossy());
                run("7z", &["x", "-y", &dest_arg, &archive_str]);
            });
        } else {
            let dest_arg = format!("-o{}", dest.to_string_lossy());
            run("7z", &["x", "-y", &dest_arg, &archive_str]);
        }
    }
}

pub struct RarBackend;
impl Extractor for RarBackend {
    fn list(&self, archive: &Path) -> Vec<String> {
        run_capture("unrar", &["lb", "-y", &archive.to_string_lossy()])
    }

    fn extract(&self, archive: &Path, dest: &Path, strip_root: bool) {
        let archive_str = archive.to_string_lossy().to_string();
        if strip_root {
            extract_stripped(dest, |tmp| {
                let tmp_str = format!("{}/", tmp.to_string_lossy());
                run("unrar", &["x", "-y", &archive_str, &tmp_str]);
            });
        } else {
            let dest_str = format!("{}/", dest.to_string_lossy().trim_end_matches('/'));
            run("unrar", &["x", "-y", &archive_str, &dest_str]);
        }
    }
}

pub fn backend_for(format: &Format) -> Box<dyn Extractor> {
    match format {
        Format::Tar | Format::Gzip | Format::Bzip2 | Format::Xz => Box::new(TarBackend),
        Format::Zip => Box::new(ZipBackend),
        Format::SevenZip => {
            check_binary("7z");
            Box::new(SevenZipBackend)
        }
        Format::Rar => {
            check_binary("unrar");
            Box::new(RarBackend)
        }
    }
}
