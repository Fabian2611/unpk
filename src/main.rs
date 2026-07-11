pub mod execution;
pub mod logging;

use std::io::Write;
use std::path::{Path, PathBuf};

enum SpecialExecutionFlag {
    None,
    Dry,
    List,
}

fn parse_ensure_exists(path: String) -> PathBuf {
    let p = Path::new(&path).to_owned();
    if !p.exists() {
        error!("{} does not exist", p.display());
        std::process::exit(1);
    }
    p
}

fn parse_file_path(path: String) -> PathBuf {
    let p = parse_ensure_exists(path);
    if !p.is_file() {
        error!("{} is not a file", p.display());
        std::process::exit(1);
    }
    p.canonicalize().unwrap_or_else(|e| {
        error!("failed to resolve path: {}", e);
        std::process::exit(1);
    })
}

fn parse_optional_dir_path(path: String) -> PathBuf {
    let p = PathBuf::from(path);
    if p.exists() && !p.is_dir() {
        error!("{} is not a directory", p.display());
        std::process::exit(1);
    }
    p
}

fn get_parent_path(path: &Path) -> &Path {
    match path.parent() {
        Some(p) => p,
        None => {
            error!(
                "{} has no parent directory. Please manually specify an output directory.",
                path.display()
            );
            std::process::exit(1);
        }
    }
}

fn archive_stem(path: &Path) -> String {
    let name = path.file_name().unwrap().to_string_lossy();
    for suffix in [".tar.gz", ".tar.bz2", ".tar.xz", ".tar.zst"] {
        if let Some(stripped) = name.strip_suffix(suffix) {
            return stripped.to_string();
        }
    }
    path.file_stem().unwrap().to_string_lossy().to_string()
}

fn err_usage() -> ! {
    eprintln!("Usage: unpack <file> [OPTIONS]");
    eprintln!();
    eprintln!("Options:");
    eprintln!("  -o, --output <dir>   Extract into a specific directory");
    eprintln!("      --here           Extract into current directory");
    eprintln!("      --dry-run        Show what would happen without writing anything");
    eprintln!("      --list           List archive contents without extracting");
    eprintln!("  -h, --help           Show this help message");
    std::process::exit(1);
}

fn main() {
    let mut args = std::env::args().skip(1).peekable();
    let mut input: Option<PathBuf> = None;
    let mut output: Option<PathBuf> = None;
    let mut here = false;
    let mut dry = false;
    let mut list = false;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "-?" | "--help" => err_usage(),
            "--here" => here = true,
            "--dry-run" => dry = true,
            "--list" => list = true,
            "-o" | "--output" => {
                let Some(v) = args.next() else {
                    error!("{}: no output directory specified", arg);
                    std::process::exit(1);
                };
                output = Some(parse_optional_dir_path(v));
            }
            _ if arg.starts_with('-') && arg != "-" => {
                error!("unknown flag '{}'", arg);
                err_usage();
            }
            _ => {
                if input.is_some() {
                    error!("extra argument '{}'", arg);
                    err_usage();
                }
                input = Some(parse_file_path(arg));
            }
        }
    }

    if input.is_none() {
        error!("missing input file");
        err_usage();
    }
    let input = input.unwrap();

    if here && output.is_some() {
        error!("cannot specify both --here and --output");
        err_usage();
    }

    let spex = if dry {
        if list {
            error!("cannot both --list and --dry-run");
            err_usage();
        }
        SpecialExecutionFlag::Dry
    } else if list {
        SpecialExecutionFlag::List
    } else {
        SpecialExecutionFlag::None
    };

    execute(&input, output, here, spex);
}

fn err_stdin(e: std::io::Error) -> ! {
    error!("could not read stdin: {}", e);
    std::process::exit(1);
}

fn dedupe(dir: &Path) {
    if !dir.exists() {
        return;
    }
    if dir
        .read_dir()
        .map(|mut rd| rd.next().is_none())
        .unwrap_or(false)
    {
        return;
    }

    loop {
        warn!(
            "Directory '{}' already exists. Proceed anyway? [y/N] ",
            dir.display()
        );
        std::io::stdout().flush().unwrap();
        let mut prompt = String::new();
        std::io::stdin()
            .read_line(&mut prompt)
            .unwrap_or_else(|e| err_stdin(e));
        match prompt.trim().to_lowercase().as_str() {
            "y" => return,
            "n" | "" => {
                error!("user aborted extraction");
                std::process::exit(1);
            }
            _ => continue,
        }
    }
}

fn root_entry_name(entries: &[String]) -> Option<String> {
    let first = |e: &String| e.split('/').next().unwrap_or("").to_string();
    let root = first(entries.first()?);
    if root.is_empty() {
        return None;
    }
    let all_under_root = entries.iter().all(|e| first(e) == root);
    let has_nesting = entries.iter().any(|e| e.contains('/'));
    (all_under_root && has_nesting).then_some(root)
}

fn resolve_dest(root: &Option<String>, input: &Path, output: Option<PathBuf>, here: bool) -> PathBuf {
    if here {
        return get_parent_path(input).to_path_buf();
    }
    if let Some(explicit) = output {
        return explicit;
    }
    let parent = get_parent_path(input).to_path_buf();
    let name = root.clone().unwrap_or_else(|| archive_stem(input));
    parent.join(name)
}

fn execute(file: &Path, output: Option<PathBuf>, here: bool, spex: SpecialExecutionFlag) {
    let fmt = execution::detect_format(file);
    let backend = execution::backend_for(&fmt);
    let entries = backend.list(file);
    let root = root_entry_name(&entries);
    let strip_root = root.is_some();

    match spex {
        SpecialExecutionFlag::List => {
            for e in &entries {
                println!("{e}");
            }
        }
        SpecialExecutionFlag::Dry => {
            let dest = resolve_dest(&root, file, output, here);
            let exists = if dest.exists() { " (exists)" } else { "" };
            let strip_note = if strip_root { ", stripping common root" } else { "" };
            println!(
                "Would extract {} entries into {}{}{}",
                entries.len(),
                dest.display(),
                exists,
                strip_note
            );
        }
        SpecialExecutionFlag::None => {
            let dest = resolve_dest(&root, file, output, here);
            if !here {
                dedupe(&dest);
            }
            std::fs::create_dir_all(&dest).unwrap_or_else(|e| {
                error!("failed to create {}: {}", dest.display(), e);
                std::process::exit(1);
            });
            backend.extract(file, &dest, strip_root);
        }
    }
}
