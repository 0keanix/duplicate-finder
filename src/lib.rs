//! # Duplicate Finder Library
//!
//! A fast and reliable library for finding duplicate files in directories.
//! This library provides both a CLI interface and programmatic API for
//! detecting file duplicates using SHA-256 hashing.
//!
//! ## Features
//!
//! - **Fast scanning**: Async/await for I/O operations
//! - **Memory efficient**: Streaming file processing
//! - **Configurable**: Size filters, hidden files, thread control
//! - **Multiple output formats**: Text and JSON
//! - **Production ready**: Comprehensive error handling and logging
//!
//! ## Example
//!
//! ```rust,no_run
//! use duplicate_finder::{Cli, FileScanner};
//! use std::path::PathBuf;
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     let config = Cli {
//!         directory: PathBuf::from("/path/to/scan"),
//!         min_size: 1024, // Skip files smaller than 1KB
//!         ..Default::default()
//!     };
//!
//!     let mut scanner = FileScanner::new(config);
//!     let results = scanner.scan().await?;
//!
//!     println!("Found {} duplicate groups", results.duplicate_groups.len());
//!     Ok(())
//! }
//! ```

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use anyhow::Result;
use clap::Parser;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::fs;

// Публичные модули - доступны для внешнего использования
pub mod scanner;
pub mod output;

// Re-export основных типов для удобства использования библиотеки
pub use scanner::FileScanner;
pub use output::OutputFormatter;

/// CLI interface - structure for parsing command line arguments
///
/// This structure defines all possible parameters that a user
/// can pass to the application via command line
#[derive(Parser, Debug, Clone)]
#[command(name = "duplicate-finder")]
#[command(about = "A blazingly fast duplicate file finder")]
#[command(version = "0.1.0")]
#[command(author = "Ilia Denisov <iodenisof@gmail.com>")]
#[command(long_about = "
Duplicate Finder recursively scans directories to identify duplicate files
using SHA-256 hashing. It supports various filtering options and output formats
to help you clean up your file system efficiently.

Examples:
  duplicate-finder /home/user/Documents
  duplicate-finder -s 1024 -e --output-format json
  duplicate-finder -o results.json /path/to/scan
")]
pub struct Cli {
    /// Directory to scan for duplicates (defaults to current directory)
    #[arg(short, long, default_value = ".")]
    #[arg(help = "Path to the directory to scan")]
    pub directory: PathBuf,

    /// Minimum file size in bytes (files smaller than this will be ignored)
    #[arg(short = 's', long, default_value = "0")]
    #[arg(help = "Minimum file size in bytes")]
    pub min_size: u64,

    /// Maximum file size in bytes (0 = no limit)
    #[arg(short = 'S', long, default_value = "0")]
    #[arg(help = "Maximum file size in bytes (0 for no limit)")]
    pub max_size: u64,

    /// Include hidden files and directories in the scan
    #[arg(short = 'H', long)]
    #[arg(help = "Include hidden files and directories")]
    pub include_hidden: bool,

    /// Exclude empty files from the scan
    #[arg(short = 'e', long)]
    #[arg(help = "Exclude empty files")]
    pub exclude_empty: bool,

    /// Output format: text or json
    #[arg(short, long, default_value = "text")]
    #[arg(help = "Output format")]
    pub output_format: OutputFormat,

    /// Save results to a file instead of printing to stdout
    #[arg(short = 'o', long)]
    #[arg(help = "Output file path")]
    pub output_file: Option<PathBuf>,

    /// Number of threads for file processing (0 = automatic)
    #[arg(short = 'j', long, default_value = "0")]
    #[arg(help = "Number of processing threads (0 for auto-detect)")]
    pub threads: usize,

    /// Enable verbose output with detailed progress information
    #[arg(short, long)]
    #[arg(help = "Verbose output")]
    pub verbose: bool,

    /// Follow symbolic links (be careful with this option!)
    #[arg(short = 'L', long)]
    #[arg(help = "Follow symbolic links (can cause infinite loops!)")]
    pub follow_symlinks: bool,

    /// Maximum depth for directory recursion (0 = unlimited)
    #[arg(short = 'd', long, default_value = "0")]
    #[arg(help = "Maximum directory depth (0 for unlimited)")]
    pub max_depth: usize,
}

/// Default implementation for Cli - useful for testing and programmatic usage
impl Default for Cli {
    fn default() -> Self {
        Self {
            directory: PathBuf::from("."),
            min_size: 0,
            max_size: 0,
            include_hidden: false,
            exclude_empty: false,
            output_format: OutputFormat::Text,
            output_file: None,
            threads: 0,
            verbose: false,
            follow_symlinks: false,
            max_depth: 0,
        }
    }
}

/// Output format
///
/// Supported formats for scan result presentation
#[derive(Debug, Clone, clap::ValueEnum, Serialize, Deserialize)]
pub enum OutputFormat {
    /// Human-readable text format with Unicode characters
    Text,
    /// Machine-readable JSON format
    Json,
}

/// File metadata
///
/// Contains all necessary information about a file for duplicate detection
/// and result presentation to the user
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FileInfo {
    /// Full path to the file
    pub path: PathBuf,

    /// File size in bytes
    pub size: u64,

    /// SHA-256 hash of file contents
    pub hash: String,

    /// Last modification time
    pub modified: SystemTime,

    /// Creation time (if available on the filesystem)
    pub created: Option<SystemTime>,
}

/// Group of duplicate files
///
/// Represents a set of files with identical contents (same hash)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DuplicateGroup {
    /// SHA-256 hash that identifies this group
    pub hash: String,

    /// Size of each file in the group (all files have the same size)
    pub size: u64,

    /// List of duplicate files
    pub files: Vec<FileInfo>,

    /// Total space occupied by all files in the group
    pub total_size: u64,

    /// Space that could be saved by removing duplicates
    pub wasted_space: u64,
}

/// Full scan result
///
/// Contains aggregated statistics and all found duplicate groups
#[derive(Debug, Serialize, Deserialize)]
pub struct ScanResult {
    /// Total number of files processed
    pub total_files: usize,

    /// Groups of duplicate files
    pub duplicate_groups: Vec<DuplicateGroup>,

    /// Total number of duplicate files across all groups
    pub total_duplicates: usize,

    /// Total amount of wasted space in bytes
    pub total_wasted_space: u64,

    /// Time taken to complete the scan
    pub scan_duration: std::time::Duration,

    /// Root directory that was scanned
    pub scanned_directory: PathBuf,
}

/// Application custom errors
///
/// Structured errors with contextual information for better diagnostics
#[derive(thiserror::Error, Debug)]
pub enum DuplicateFinderError {
    /// Standard I/O error wrapper
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Permission denied for specific path
    #[error("Permission denied for path: {path}")]
    PermissionDenied { path: PathBuf },

    /// Path does not exist
    #[error("Path does not exist: {path}")]
    PathNotFound { path: PathBuf },

    /// Invalid configuration: min_size > max_size
    #[error("Invalid size filter: min_size ({min}) > max_size ({max})")]
    InvalidSizeFilter { min: u64, max: u64 },

    /// Error calculating file hash
    #[error("Failed to calculate file hash for: {path}")]
    HashCalculationError { path: PathBuf },

    /// Symbolic link loop detected
    #[error("Symbolic link loop detected at: {path}")]
    SymlinkLoop { path: PathBuf },

    /// Maximum recursion depth exceeded
    #[error("Maximum directory depth ({max_depth}) exceeded at: {path}")]
    MaxDepthExceeded { path: PathBuf, max_depth: usize },
}

impl FileInfo {
    /// Creates a new FileInfo from a file path
    ///
    /// This is the core function that extracts all necessary metadata
    /// and calculates the SHA-256 hash of the file contents
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the file to process
    ///
    /// # Returns
    ///
    /// * `Ok(FileInfo)` - Successfully processed file
    /// * `Err(DuplicateFinderError)` - Error accessing file or calculating hash
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use duplicate_finder::FileInfo;
    /// use std::path::Path;
    ///
    /// #[tokio::main]
    /// async fn main() -> anyhow::Result<()> {
    ///     let file_info = FileInfo::from_path(Path::new("example.txt")).await?;
    ///     println!("File hash: {}", file_info.hash);
    ///     Ok(())
    /// }
    /// ```
    pub async fn from_path(path: &Path) -> Result<Self, DuplicateFinderError> {
        // Get file metadata with detailed error mapping
        let metadata = fs::metadata(path)
            .await
            .map_err(|e| match e.kind() {
                std::io::ErrorKind::PermissionDenied => {
                    DuplicateFinderError::PermissionDenied {
                        path: path.to_path_buf(),
                    }
                }
                std::io::ErrorKind::NotFound => {
                    DuplicateFinderError::PathNotFound {
                        path: path.to_path_buf(),
                    }
                }
                _ => DuplicateFinderError::Io(e),
            })?;

        let size = metadata.len();
        let modified = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);
        let created = metadata.created().ok();

        // Calculate file hash - this is the most expensive operation
        let hash = Self::calculate_file_hash(path).await
            .map_err(|_| DuplicateFinderError::HashCalculationError {
                path: path.to_path_buf(),
            })?;

        Ok(FileInfo {
            path: path.to_path_buf(),
            size,
            hash,
            modified,
            created,
        })
    }

    /// Calculates SHA-256 hash of file contents using streaming
    ///
    /// This function reads the file in chunks to handle large files efficiently
    /// without loading the entire file into memory
    async fn calculate_file_hash(path: &Path) -> Result<String, std::io::Error> {
        use tokio::io::AsyncReadExt;

        let mut file = fs::File::open(path).await?;
        let mut hasher = Sha256::new();
        let mut buffer = vec![0u8; 65536]; // 64KB buffer for optimal performance

        loop {
            let bytes_read = file.read(&mut buffer).await?;
            if bytes_read == 0 {
                break; // End of file reached
            }
            hasher.update(&buffer[..bytes_read]);
        }

        // Convert hash to hexadecimal string
        Ok(format!("{:x}", hasher.finalize()))
    }
}

// Utility functions for the library

/// Formats byte count into human-readable string
///
/// Converts large numbers into appropriate units (B, KB, MB, GB, TB)
///
/// # Examples
///
/// ```rust
/// use duplicate_finder::format_bytes;
///
/// assert_eq!(format_bytes(1024), "1.00 KB");
/// assert_eq!(format_bytes(1536), "1.50 KB");
/// ```
pub fn format_bytes(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    const THRESHOLD: f64 = 1024.0;

    if bytes == 0 {
        return "0 B".to_string();
    }

    let bytes_f = bytes as f64;
    let unit_index = (bytes_f.ln() / THRESHOLD.ln()).floor() as usize;
    let unit_index = unit_index.min(UNITS.len() - 1);

    let value = bytes_f / THRESHOLD.powi(unit_index as i32);

    if unit_index == 0 {
        format!("{} {}", bytes, UNITS[unit_index])
    } else {
        format!("{:.2} {}", value, UNITS[unit_index])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use tokio::io::AsyncWriteExt;

    #[tokio::test]
    async fn test_file_info_creation() {
        let temp_dir = tempdir().expect("Failed to create temp dir");
        let file_path = temp_dir.path().join("test_file.txt");

        // Create a test file
        let mut file = fs::File::create(&file_path).await.expect("Failed to create test file");
        file.write_all(b"Hello, World!").await.expect("Failed to write to test file");
        file.flush().await.expect("Failed to flush test file");

        // Test FileInfo creation
        let file_info = FileInfo::from_path(&file_path).await.expect("Failed to create FileInfo");

        assert_eq!(file_info.path, file_path);
        assert_eq!(file_info.size, 13); // "Hello, World!" is 13 bytes
        assert!(!file_info.hash.is_empty());

        // Verify hash consistency
        let file_info2 = FileInfo::from_path(&file_path).await.expect("Failed to create FileInfo");
        assert_eq!(file_info.hash, file_info2.hash);
    }

    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(1024), "1.00 KB");
        assert_eq!(format_bytes(1536), "1.50 KB");
        assert_eq!(format_bytes(1024 * 1024), "1.00 MB");
        assert_eq!(format_bytes(1024 * 1024 * 1024), "1.00 GB");
    }

    #[test]
    fn test_cli_default() {
        let cli = Cli::default();
        assert_eq!(cli.directory, PathBuf::from("."));
        assert_eq!(cli.min_size, 0);
        assert_eq!(cli.threads, 0);
        assert!(!cli.verbose);
    }
}