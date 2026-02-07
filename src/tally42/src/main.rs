mod model;

use model::Statement;
use std::env;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

fn main() {
    let mut verbose = false;
    for arg in env::args().skip(1) {
        match arg.as_str() {
            "-v" | "--verbose" => verbose = true,
            _ => {
                eprintln!("error: unknown argument: {arg}");
                std::process::exit(1);
            }
        }
    }

    let workdir = match env::var("TALLY42_WORKDIR") {
        Ok(val) => PathBuf::from(val),
        Err(_) => env::current_dir().unwrap_or_else(|err| {
            eprintln!("error: failed to get current dir: {err}");
            std::process::exit(1);
        }),
    };

    if !workdir.is_dir() {
        eprintln!("error: workdir is not a directory: {}", workdir.display());
        std::process::exit(1);
    }

    let mut loaded = 0usize;
    let mut errors = 0usize;

    for entry in WalkDir::new(&workdir)
        .into_iter()
        .filter_entry(|e| !should_skip_dir(e))
        .filter_map(|e| e.ok())
    {
        if !is_toml_file(entry.path()) {
            continue;
        }

        let path = entry.path();
        let content = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(err) => {
                eprintln!("warning: failed to read {}: {err}", path.display());
                errors += 1;
                continue;
            }
        };

        match toml::from_str::<Statement>(&content) {
            Ok(stmt) => {
                loaded += 1;
                if verbose {
                    eprintln!(
                        "loaded: {} (account={}, closing-date={})",
                        path.display(),
                        stmt.account,
                        stmt.closing_date
                    );
                }
            }
            Err(err) => {
                eprintln!("warning: failed to parse {}: {err}", path.display());
                errors += 1;
            }
        }
    }

    if verbose {
        eprintln!("loaded {loaded} statements with {errors} warnings");
    }
}

fn is_toml_file(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.eq_ignore_ascii_case("toml"))
        .unwrap_or(false)
}

fn should_skip_dir(entry: &walkdir::DirEntry) -> bool {
    if !entry.file_type().is_dir() {
        return false;
    }
    matches!(entry.file_name().to_str(), Some(".git" | "bld" | "bin"))
}
