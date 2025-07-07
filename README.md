# duplicate-finder

A fast and reliable duplicate file finder built in Rust, optimized for performance and usability.

## Features

- **Fast scanning**: Utilizes async I/O and parallel processing for maximum performance
- **Memory efficient**: Streams file data for minimal memory footprint even with large files
- **Smart filtering**: Exclude files by size, type, or hidden status
- **Accurate results**: Uses SHA-256 hashing for reliable duplicate detection
- **User-friendly interface**: Clear progress indicators and comprehensive results
- **Multiple output formats**: Human-readable text or machine-readable JSON
- **Detailed statistics**: Size analysis, wasted space calculation, and cleanup recommendations

## Installation

### From Source

```bash
# Clone the repository
git clone https://github.com/0keanix/duplicate-finder
cd duplicate-finder

# Build and install the application
cargo install --path .
```

## Usage

```bash
# Basic usage - scan current directory
duplicate-finder

# Scan a specific directory
duplicate-finder -d /path/to/scan

# Skip small files and export results to JSON
duplicate-finder -s 10240 --output-format json

# Save results to a file
duplicate-finder -o results.json /path/to/scan
```

### Command-line Options

```
OPTIONS:
  -d, --directory <DIRECTORY>    Path to the directory to scan [default: .]
  -s, --min-size <MIN_SIZE>      Minimum file size in bytes [default: 0]
  -S, --max-size <MAX_SIZE>      Maximum file size in bytes (0 for no limit) [default: 0]
  -H, --include-hidden           Include hidden files and directories
  -e, --exclude-empty            Exclude empty files
  -o, --output-format <FORMAT>   Output format: text or json [default: text]
  -o, --output-file <FILE>       Output file path
  -j, --threads <THREADS>        Number of processing threads (0 for auto-detect) [default: 0]
  -v, --verbose                  Verbose output
  -L, --follow-symlinks          Follow symbolic links (can cause infinite loops!)
  -D, --max-depth <MAX_DEPTH>    Maximum directory depth (0 for unlimited) [default: 0]
  -h, --help                     Print help
  -V, --version                  Print version
```

## Example Output

```
📊 SCAN RESULTS
══════════════════════════════════════════════════
📁 Scanned Directory: /home/user/Documents
⏱️  Scan Duration: 3.45s
📄 Total Files: 1258
🔄 Duplicate Files: 56
📦 Duplicate Groups: 18
💾 Wasted Space: 234.5 MB

🔍 DUPLICATE GROUPS (sorted by wasted space)
──────────────────────────────────────────────────
📋 Group #1 (15.7 MB)
   💰 Wasted space: 47.1 MB
   🔐 Hash: 3a7bd3e2ccb4d08c...
   📊 4 duplicate files:
     📌 /home/user/Documents/original.mp4
        📅 Modified: 2023-04-12 14:23:45
     🔄 /home/user/Downloads/same_video.mp4
        📅 Modified: 2023-05-01 09:45:12
     🔄 /home/user/Backup/original.mp4
        📅 Modified: 2023-06-10 16:30:00
     🔄 /home/user/Documents/old/original.mp4
        📅 Modified: 2022-11-22 11:10:34
```

## Performance

The application has been optimized for performance, using several techniques:

- Asynchronous I/O operations with Tokio
- Parallelized file processing with configurable thread count
- Efficient file hashing with optimized buffer sizes
- Incremental hash calculation that doesn't load entire files into memory
- Smart duplicate grouping algorithms

## Use as a Library

`duplicate-finder` can also be used as a library in other Rust projects:

```rust
use duplicate_finder::{Cli, FileScanner};
use std::path::PathBuf;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = Cli {
        directory: PathBuf::from("/path/to/scan"),
        min_size: 1024, // Skip files smaller than 1KB
        ..Default::default()
    };

    let mut scanner = FileScanner::new(config);
    let results = scanner.scan().await?;

    println!("Found {} duplicate groups", results.duplicate_groups.len());
    Ok(())
}
```

## License

This project is licensed under the MIT License - see the LICENSE file for details.

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

---

Built with ❤️ by Ilia Denisov ([iodenisof@gmail.com](mailto:iodenisof@gmail.com))
