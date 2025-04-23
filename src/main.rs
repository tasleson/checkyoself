use std::{
    collections::HashMap,
    env,
    fs::{self, File},
    io::{BufReader, Read},
    path::{Path, PathBuf},
    process::exit,
    sync::{Arc, Mutex},
};

use colored::*;
use indicatif::{ProgressBar, ProgressStyle};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use walkdir::WalkDir;

#[derive(Serialize, Deserialize, Debug, Clone)]
struct FileMeta {
    hash: String,
    modified: u64, // UNIX timestamp (secs since epoch),
    size: i64,
}

#[derive(Serialize, Deserialize, Debug)]
struct FileHashMap(HashMap<String, FileMeta>);

fn file_metadata(path: &Path) -> std::io::Result<(u64, i64)> {
    let metadata = fs::metadata(path)?;

    let modified_secs = metadata
        .modified()
        .unwrap()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let file_size = metadata.len() as i64;

    Ok((modified_secs, file_size))
}

fn calculate_blake3(path: &Path) -> std::io::Result<FileMeta> {
    let (modified, size) = file_metadata(path)?;

    let file = File::open(path)?;
    let mut reader = BufReader::new(file);
    let mut hasher = blake3::Hasher::new();
    let mut buffer = [0u8; 8192];

    loop {
        let n = reader.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }

    Ok(FileMeta {
        hash: hasher.finalize().to_hex().to_string(),
        modified,
        size,
    })
}

fn walk_files(dir: &Path, skip_dirs: &[String]) -> Vec<PathBuf> {
    WalkDir::new(dir)
        .into_iter()
        .filter_entry(|entry| {
            // Skip directory if its name matches one of the skip_dirs
            if entry.file_type().is_dir() {
                if let Some(name) = entry.file_name().to_str() {
                    !skip_dirs.iter().any(|skip| name == skip)
                } else {
                    true
                }
            } else {
                true
            }
        })
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
        .map(|e| e.path().to_path_buf())
        .collect()
}

fn hash_files_parallel(paths: Vec<PathBuf>, show_progress: bool) -> HashMap<String, FileMeta> {
    let map = Arc::new(Mutex::new(HashMap::new()));

    let progress = if show_progress {
        let bar = ProgressBar::new(paths.len() as u64);
        bar.set_style(
            ProgressStyle::with_template(
                "[{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} {msg}",
            )
            .unwrap()
            .progress_chars("##-"),
        );
        Some(bar)
    } else {
        None
    };

    paths.par_iter().for_each(|path| {
        if let Ok(meta) = calculate_blake3(path) {
            let mut map_lock = map.lock().unwrap();
            map_lock.insert(path.to_string_lossy().to_string(), meta);
        }

        if let Some(pb) = &progress {
            pb.inc(1);
        }
    });

    if let Some(pb) = progress {
        pb.finish_with_message("Hashing complete");
    }

    Arc::try_unwrap(map).unwrap().into_inner().unwrap()
}

fn get_reference_by_hash(reference: &HashMap<String, FileMeta>) -> HashMap<String, Vec<String>> {
    let mut reference_by_hash: HashMap<String, Vec<String>> = HashMap::new();
    for (path, meta) in reference {
        reference_by_hash
            .entry(meta.hash.to_string())
            .or_default()
            .push(path.clone());
    }
    reference_by_hash
}

fn verify_and_update(
    current: &HashMap<String, FileMeta>,
    reference: &mut HashMap<String, FileMeta>,
    reference_file: &Path,
    update: bool,
    quiet: bool,
) -> bool {
    let mut matched = 0;
    let mut moved = 0;
    let mut mismatched = 0;
    let mut extra = 0;

    let reference_by_hash = get_reference_by_hash(reference);

    for (path, current_meta) in current {
        let item = reference.get(path);

        match item {
            Some(expected_meta) => {
                if current_meta.hash == expected_meta.hash {
                    //println!("{} {}", "✅ MATCHED".green(), path);
                    matched += 1;
                } else if current_meta.modified == expected_meta.modified {
                    println!(
                        "{} {}\n  expected: {}\n  found:    {}",
                        "❌ MISMATCH".red(),
                        path,
                        expected_meta.hash,
                        current_meta.hash
                    );
                    mismatched += 1;
                } else {
                    if !quiet {
                        println!(
                            "{} {} (modified time differs, hash ignored)",
                            "ℹ️ SKIPPED".blue(),
                            path
                        );
                    }
                    if update {
                        if !quiet {
                            println!("{} Added to reference list", "➕".cyan());
                        }
                        reference.insert(path.clone(), current_meta.clone());
                    }
                }
            }
            None => {
                if let Some(prev_paths) = reference_by_hash.get(&current_meta.hash) {
                    // Files of zero size have same hash ...
                    if current_meta.size != 0 {
                        let c_paths: Vec<String> =
                            prev_paths.iter().map(|p| p.to_string()).collect();

                        if !quiet && prev_paths.len() < 3 {
                            println!(
                                "{} {}\n  previously: {}",
                                "🔀 MOVED".yellow(),
                                path,
                                c_paths.join(", ")
                            );
                        }
                        moved += 1;

                        if update {
                            reference.insert(path.clone(), current_meta.clone());
                        }
                    }
                } else {
                    if !quiet {
                        println!("{} {}", "⚠️ EXTRA".blue(), path);
                    }

                    extra += 1;
                    if update {
                        if !quiet {
                            println!("{} Added to reference list", "➕".cyan());
                        }
                        reference.insert(path.clone(), current_meta.clone());
                    }
                }
            }
        }
    }

    if !quiet {
        println!("\n=== {} ===", "SUMMARY".bold().underline());
        println!("{} {}", "✅ Verified:".green(), matched);
        println!("{} {}", "🔀 Moved:".yellow(), moved);
        println!("{} {}", "❌ Mismatched:".red(), mismatched);
        println!("{} {}", "⚠️ Extra:".blue(), extra);
    }

    if update {
        if !quiet {
            println!(
                "\n{} Updating reference file: {}",
                "💾".bold(),
                reference_file.display()
            );
        }
        let json = serde_json::to_string_pretty(&FileHashMap(current.clone()))
            .expect("Serialization failed");
        fs::write(reference_file, json).expect("Failed to write updated reference");
    }

    mismatched > 0
}

fn main() -> std::io::Result<()> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 3 {
        eprintln!("Usage:");
        eprintln!(
            "  {} <directory> <output.json> [--progress] [--skip <dir>...] [--q]",
            args[0]
        );
        eprintln!(
            "  {} <directory> --verify <ref.json> [--update] [--progress] [--skip <dir>...] [--q]",
            args[0]
        );
        std::process::exit(1);
    }

    let dir = PathBuf::from(&args[1]);
    let show_progress = args.contains(&"--progress".to_string());
    let update = args.contains(&"--update".to_string());
    let verify_mode = args.contains(&"--verify".to_string());
    let quiet = args.contains(&"--q".to_string());

    let verify_file = args
        .iter()
        .position(|x| x == "--verify")
        .and_then(|i| args.get(i + 1))
        .map(PathBuf::from);
    let output_file = if !verify_mode {
        Some(PathBuf::from(&args[2]))
    } else {
        None
    };

    // Parse skipped dirs
    let skip_dirs: Vec<String> = args
        .windows(2)
        .filter(|w| w[0] == "--skip")
        .map(|w| w[1].clone())
        .collect();

    if !dir.is_dir() {
        eprintln!("Error: {} is not a directory", dir.display());
        std::process::exit(1);
    }

    let files = walk_files(&dir, &skip_dirs);
    let current_hashes = hash_files_parallel(files, show_progress);

    if verify_mode {
        let verify_file = verify_file.expect("Missing argument for --verify");
        let data = fs::read_to_string(&verify_file)?;
        let FileHashMap(mut reference_hashes) = serde_json::from_str(&data)?;
        let had_mismatches = verify_and_update(
            &current_hashes,
            &mut reference_hashes,
            &verify_file,
            update,
            quiet,
        );
        if had_mismatches {
            eprintln!("{}", "❌ One or more mismatches found!".red().bold());
            exit(2);
        }
    } else if let Some(output_file) = output_file {
        let file_map = FileHashMap(current_hashes);
        let json = serde_json::to_string_pretty(&file_map).expect("Serialization failed");
        fs::write(&output_file, json)?;

        if !quiet {
            println!("Hash table written to {}", output_file.display());
        }
    }

    Ok(())
}
