use anyhow::{Context, Result};
use clap::Parser;
use indicatif::{ProgressBar, ProgressStyle};
use rayon::prelude::*;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    /// Path to the archive directory
    #[clap(short, long)]
    archive_path: PathBuf,

    /// Archive name to use in manifest paths (defaults to directory name)
    #[clap(long)]
    archive_name: Option<String>,

    /// Output file for the manifest
    #[clap(short, long, default_value = "manifest.txt")]
    output: PathBuf,

    /// Number of worker threads (defaults to number of CPU cores)
    #[clap(short, long)]
    threads: Option<usize>,

    /// Buffer size for reading files (in bytes)
    #[clap(short, long, default_value = "1048576")]
    buffer_size: usize,

    /// Show progress bar
    #[clap(short, long)]
    progress: bool,

    /// Validate existing manifest file
    #[clap(short, long)]
    validate: bool,

    /// Update manifest for new or changed files only
    #[clap(short, long)]
    update: bool,
}

struct FileInfo {
    path: PathBuf,
    size: u64,
}

fn hash_file(file_info: &FileInfo, archive_path: &Path, archive_name: &str, buffer_size: usize) -> Result<String> {
    let hash = get_file_hash(file_info, buffer_size)?;
    
    // Get relative path from archive root
    let relative_path = file_info.path
        .strip_prefix(archive_path)
        .unwrap_or(&file_info.path)
        .to_string_lossy();
    
    // Combine archive name with relative path
    let full_relative_path = if relative_path.is_empty() {
        archive_name.to_string()
    } else {
        format!("{}/{}", archive_name, relative_path)
    };
    
    Ok(format!("{} {}", hash, full_relative_path))
}

fn collect_files(archive_path: &Path) -> Result<Vec<FileInfo>> {
    let mut files = Vec::new();
    
    for entry in WalkDir::new(archive_path)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if entry.file_type().is_file() {
            // Skip macOS metadata files
            let filename = entry.file_name().to_string_lossy();
            if filename.starts_with("._") {
                continue;
            }
            
            // Try to get metadata, skip files that can't be accessed
            match entry.metadata() {
                Ok(metadata) => {
                    files.push(FileInfo {
                        path: entry.path().to_path_buf(),
                        size: metadata.len(),
                    });
                }
                Err(e) => {
                    eprintln!("Warning: Skipping file {}: {}", entry.path().display(), e);
                    continue;
                }
            }
        }
    }
    
    Ok(files)
}

fn load_existing_manifest(manifest_path: &Path) -> Result<HashMap<PathBuf, String>> {
    let mut manifest = HashMap::new();
    
    if !manifest_path.exists() {
        return Ok(manifest);
    }
    
    let file = fs::File::open(manifest_path)
        .with_context(|| format!("Failed to open manifest file: {}", manifest_path.display()))?;
    let reader = BufReader::new(file);
    
    for (line_num, line) in reader.lines().enumerate() {
        let line = line.with_context(|| format!("Failed to read line {} in manifest", line_num + 1))?;
        let line = line.trim();
        
        if line.is_empty() {
            continue;
        }
        
        // Parse line: <hash> <path>
        let parts: Vec<&str> = line.splitn(2, ' ').collect();
        if parts.len() != 2 {
            eprintln!("Warning: Invalid line {} in manifest: {}", line_num + 1, line);
            continue;
        }
        
        let hash = parts[0].to_string();
        let path = PathBuf::from(parts[1]);
        
        manifest.insert(path, hash);
    }
    
    Ok(manifest)
}

fn get_file_hash(file_info: &FileInfo, buffer_size: usize) -> Result<String> {
    let mut hasher = Sha256::new();
    let mut buffer = vec![0; buffer_size];
    
    let mut file = fs::File::open(&file_info.path)
        .with_context(|| format!("Failed to open file: {}", file_info.path.display()))?;
    
    loop {
        let bytes_read = std::io::Read::read(&mut file, &mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }
    
    let hash = hasher.finalize();
    Ok(format!("{:x}", hash))
}

fn validate_manifest(archive_path: &Path, manifest_path: &Path, archive_name: &str, args: &Args) -> Result<()> {
    println!("Validating manifest: {}", manifest_path.display());
    
    let existing_manifest = load_existing_manifest(manifest_path)?;
    let files = collect_files(archive_path)?;
    
    if files.is_empty() {
        println!("No files found in archive");
        return Ok(());
    }
    
    let progress_bar = if args.progress {
        let pb = ProgressBar::new(files.len() as u64);
        pb.set_style(
            ProgressStyle::default_bar()
                .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({eta})")
                .progress_chars("#>-"),
        );
        Some(pb)
    } else {
        None
    };
    
    let mut valid_count = 0;
    let mut invalid_count = 0;
    let mut missing_count = 0;
    let mut new_count = 0;
    
    for file_info in &files {
        // Get the archive folder name
        let archive_name = archive_path
            .file_name()
            .unwrap_or_else(|| archive_path.as_os_str())
            .to_string_lossy();
        
        // Get relative path for comparison
        let relative_path = file_info.path
            .strip_prefix(archive_path)
            .unwrap_or(&file_info.path);
        
        // Create the full relative path with archive name
        let full_relative_path = if relative_path.to_string_lossy().is_empty() {
            PathBuf::from(&*archive_name)
        } else {
            PathBuf::from(format!("{}/{}", archive_name, relative_path.to_string_lossy()))
        };
        
        let expected_hash = existing_manifest.get(&full_relative_path);
        
        if let Some(expected) = expected_hash {
            let actual_hash = get_file_hash(file_info, args.buffer_size)?;
            
            if actual_hash == *expected {
                valid_count += 1;
            } else {
                invalid_count += 1;
                println!("Hash mismatch for {}: expected {}, got {}", 
                    relative_path.display(), expected, actual_hash);
            }
        } else {
            new_count += 1;
            println!("New file found: {}", relative_path.display());
        }
        
        if let Some(ref pb) = progress_bar {
            pb.inc(1);
        }
    }
    
    // Check for missing files
    for relative_path in existing_manifest.keys() {
        let full_path = archive_path.join(relative_path);
        if !full_path.exists() {
            missing_count += 1;
            println!("Missing file: {}", relative_path.display());
        }
    }
    
    if let Some(pb) = progress_bar {
        pb.finish_with_message("Validation complete");
    }
    
    println!("Validation results:");
    println!("  Valid files: {}", valid_count);
    println!("  Invalid files: {}", invalid_count);
    println!("  New files: {}", new_count);
    println!("  Missing files: {}", missing_count);
    
    if invalid_count > 0 || missing_count > 0 {
        anyhow::bail!("Validation failed: {} invalid files, {} missing files", invalid_count, missing_count);
    }
    
    println!("Validation successful!");
    Ok(())
}

fn update_manifest(archive_path: &Path, manifest_path: &Path, archive_name: &str, args: &Args) -> Result<()> {
    println!("Updating manifest: {}", manifest_path.display());
    
    let mut existing_manifest = load_existing_manifest(manifest_path)?;
    let files = collect_files(archive_path)?;
    
    if files.is_empty() {
        println!("No files found in archive");
        return Ok(());
    }
    
    let progress_bar = if args.progress {
        let pb = ProgressBar::new(files.len() as u64);
        pb.set_style(
            ProgressStyle::default_bar()
                .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({eta})")
                .progress_chars("#>-"),
        );
        Some(pb)
    } else {
        None
    };
    
    let mut updated_count = 0;
    let mut unchanged_count = 0;
    let mut new_count = 0;
    
    for file_info in &files {
        // Get the archive folder name
        let archive_name = archive_path
            .file_name()
            .unwrap_or_else(|| archive_path.as_os_str())
            .to_string_lossy();
        
        // Get relative path for comparison
        let relative_path = file_info.path
            .strip_prefix(archive_path)
            .unwrap_or(&file_info.path);
        
        // Create the full relative path with archive name
        let full_relative_path = if relative_path.to_string_lossy().is_empty() {
            PathBuf::from(&*archive_name)
        } else {
            PathBuf::from(format!("{}/{}", archive_name, relative_path.to_string_lossy()))
        };
        
        let expected_hash = existing_manifest.get(&full_relative_path);
        let actual_hash = get_file_hash(file_info, args.buffer_size)?;
        
        if let Some(expected) = expected_hash {
            if actual_hash == *expected {
                unchanged_count += 1;
            } else {
                existing_manifest.insert(full_relative_path.clone(), actual_hash);
                updated_count += 1;
                println!("Updated hash for: {}", full_relative_path.display());
            }
        } else {
            existing_manifest.insert(full_relative_path.clone(), actual_hash);
            new_count += 1;
            println!("Added new file: {}", full_relative_path.display());
        }
        
        if let Some(ref pb) = progress_bar {
            pb.inc(1);
        }
    }
    
    // Remove entries for files that no longer exist
    let mut removed_count = 0;
    existing_manifest.retain(|relative_path, _| {
        let full_path = archive_path.join(relative_path);
        if full_path.exists() {
            true
        } else {
            removed_count += 1;
            println!("Removed missing file: {}", relative_path.display());
            false
        }
    });
    
    // Write updated manifest
    let mut output_file = fs::File::create(manifest_path)
        .with_context(|| format!("Failed to create output file: {}", manifest_path.display()))?;
    
    for (path, hash) in existing_manifest {
        writeln!(output_file, "{} {}", hash, path.display())?;
    }
    
    if let Some(pb) = progress_bar {
        pb.finish_with_message("Update complete");
    }
    
    println!("Update results:");
    println!("  Unchanged files: {}", unchanged_count);
    println!("  Updated files: {}", updated_count);
    println!("  New files: {}", new_count);
    println!("  Removed files: {}", removed_count);
    
    Ok(())
}

fn main() -> Result<()> {
    let args = Args::parse();
    
    // Validate archive path
    if !args.archive_path.exists() {
        anyhow::bail!("Archive path does not exist: {}", args.archive_path.display());
    }
    if !args.archive_path.is_dir() {
        anyhow::bail!("Archive path is not a directory: {}", args.archive_path.display());
    }
    
    // Determine archive name
    let archive_name = args.archive_name.clone().unwrap_or_else(|| {
        args.archive_path
            .file_name()
            .unwrap_or_else(|| args.archive_path.as_os_str())
            .to_string_lossy()
            .to_string()
    });
    
    // Handle different modes
    if args.validate {
        validate_manifest(&args.archive_path, &args.output, &archive_name, &args)?;
        return Ok(());
    }
    
    if args.update {
        update_manifest(&args.archive_path, &args.output, &archive_name, &args)?;
        return Ok(());
    }
    
    // Default mode: generate new manifest
    println!("Scanning archive: {}", args.archive_path.display());
    let files = collect_files(&args.archive_path)?;
    println!("Found {} files", files.len());
    
    if files.is_empty() {
        println!("No files found in archive");
        return Ok(());
    }
    
    // Calculate total size for progress tracking
    let total_size: u64 = files.iter().map(|f| f.size).sum();
    println!("Total size: {} bytes ({:.2} GB)", total_size, total_size as f64 / 1024.0 / 1024.0 / 1024.0);
    
    // Setup progress bar if requested
    let progress_bar = if args.progress {
        let pb = ProgressBar::new(files.len() as u64);
        pb.set_style(
            ProgressStyle::default_bar()
                .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({eta})")
                .progress_chars("#>-"),
        );
        Some(pb)
    } else {
        None
    };
    
    // Setup thread pool
    let thread_count = args.threads.unwrap_or_else(|| {
        std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1)
    });
    
    println!("Using {} threads with {} byte buffer", thread_count, args.buffer_size);
    
    // Process files in parallel
    let start_time = std::time::Instant::now();
    
    let results: Vec<Result<String>> = files
        .par_iter()
        .map(|file_info| {
            let result = hash_file(file_info, &args.archive_path, &archive_name, args.buffer_size);
            if let Some(ref pb) = progress_bar {
                pb.inc(1);
            }
            result
        })
        .collect();
    
    // Write results to output file
    println!("Writing manifest to: {}", args.output.display());
    let mut output_file = fs::File::create(&args.output)
        .with_context(|| format!("Failed to create output file: {}", args.output.display()))?;
    
    let mut success_count = 0;
    let mut error_count = 0;
    
    for result in results {
        match result {
            Ok(line) => {
                writeln!(output_file, "{}", line)?;
                success_count += 1;
            }
            Err(e) => {
                eprintln!("Error processing file: {}", e);
                error_count += 1;
            }
        }
    }
    
    if let Some(pb) = progress_bar {
        pb.finish_with_message("Complete");
    }
    
    let elapsed = start_time.elapsed();
    println!(
        "Manifest generation complete in {:.2?}",
        elapsed
    );
    println!("Successfully processed: {} files", success_count);
    if error_count > 0 {
        println!("Errors: {} files", error_count);
    }
    
    Ok(())
}
