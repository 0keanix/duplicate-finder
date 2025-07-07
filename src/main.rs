//! # Duplicate Finder CLI Application
//!
//! Command-line interface for the duplicate finder library.
//! This binary provides a user-friendly interface to scan directories
//! and find duplicate files.

use anyhow::Result;
use clap::Parser;
use tracing::{info, error};

// Use our library
use duplicate_finder::{Cli, FileScanner, OutputFormatter};

/// Main entry point for the CLI application
///
/// This function coordinates the entire duplicate finding process:
/// 1. Initializes logging system
/// 2. Parses command line arguments
/// 3. Creates and runs the file scanner
/// 4. Formats and displays results
/// 5. Handles errors gracefully
#[tokio::main]
async fn main() -> Result<()> {
    // Initialize a logging system with level based on verbose flag
    // We check for a verbose flag before parsing to set up logging correctly
    let verbose = std::env::args().any(|arg| arg == "-v" || arg == "--verbose");

    let subscriber = tracing_subscriber::fmt()
        .with_max_level(if verbose {
            tracing::Level::DEBUG
        } else {
            tracing::Level::INFO
        })
        .with_target(false) // Don't show module names in logs for cleaner output
        .finish();

    tracing::subscriber::set_global_default(subscriber)
        .expect("Failed to set global logger");

    // Parse command line arguments using clap
    let cli = Cli::parse();

    // Log startup information
    info!("🚀 Starting Duplicate Finder v{}", env!("CARGO_PKG_VERSION"));
    info!("📁 Target directory: {}", cli.directory.display());

    // Display a welcome message to user
    display_welcome_banner(&cli);

    // Create and configure the file scanner
    let mut scanner = FileScanner::new(cli.clone());

    // Execute the main scanning process
    match scanner.scan().await {
        Ok(scan_result) => {
            info!("📊 Scan completed successfully");

            // Create a formatter for displaying results
            let formatter = OutputFormatter::new(&cli);

            // Display results in the requested format
            if let Err(e) = formatter.display_results(&scan_result).await {
                error!("Failed to display results: {}", e);
                std::process::exit(1);
            }

            // Save to a file if requested
            if let Some(output_file) = &cli.output_file {
                match formatter.save_to_file(&scan_result, output_file).await {
                    Ok(()) => {
                        println!("💾 Results saved to: {}", output_file.display());
                        info!("Results saved to file: {}", output_file.display());
                    }
                    Err(e) => {
                        error!("Failed to save results to file: {}", e);
                        eprintln!("❌ Failed to save results: {}", e);
                        std::process::exit(1);
                    }
                }
            }

            // Display final summary
            display_completion_summary(&scan_result);
        }
        Err(e) => {
            error!("Scan failed: {}", e);
            eprintln!("❌ Scan failed: {}", e);

            // Provide helpful error context
            display_error_help(&e);
            std::process::exit(1);
        }
    }

    Ok(())
}

/// Displays a welcome banner with configuration information
fn display_welcome_banner(cli: &Cli) {
    println!("🔍 Duplicate Finder v{}", env!("CARGO_PKG_VERSION"));
    println!("📂 Scanning directory: {}", cli.directory.display());

    if cli.verbose {
        println!();
        println!("🔧 Configuration:");

        if cli.min_size > 0 {
            println!("   📏 Min size: {}", duplicate_finder::format_bytes(cli.min_size));
        }

        if cli.max_size > 0 {
            println!("   📐 Max size: {}", duplicate_finder::format_bytes(cli.max_size));
        }

        println!("   👁️  Include hidden: {}", cli.include_hidden);
        println!("   🚫 Exclude empty: {}", cli.exclude_empty);
        println!("   🔗 Follow symlinks: {}", cli.follow_symlinks);

        if cli.max_depth > 0 {
            println!("   📊 Max depth: {}", cli.max_depth);
        }

        println!("   🧵 Threads: {}",
                 if cli.threads == 0 {
                     "auto".to_string()
                 } else {
                     cli.threads.to_string()
                 }
        );

        println!("   📄 Output format: {:?}", cli.output_format);
    }

    println!();
}

/// Displays a completion summary with key metrics
fn display_completion_summary(scan_result: &duplicate_finder::ScanResult) {
    println!();
    println!("✅ Scan completed!");
    println!("⏱️  Duration: {:?}", scan_result.scan_duration);

    if scan_result.duplicate_groups.is_empty() {
        println!("🎉 No duplicates found - your files are perfectly organized!");
    } else {
        println!("📊 Summary:");
        println!("   📁 Files scanned: {}", scan_result.total_files);
        println!("   🔄 Duplicate files: {}", scan_result.total_duplicates);
        println!("   📦 Duplicate groups: {}", scan_result.duplicate_groups.len());
        println!("   💾 Space wasted: {}", duplicate_finder::format_bytes(scan_result.total_wasted_space));

        // Calculate potential savings percentage
        if scan_result.total_files > 0 {
            let largest_group = scan_result.duplicate_groups
                .iter()
                .max_by_key(|g| g.wasted_space);

            if let Some(group) = largest_group {
                println!("   🏆 Largest group: {} files, {} wasted",
                         group.files.len(),
                         duplicate_finder::format_bytes(group.wasted_space)
                );
            }
        }
    }
}

/// Provides helpful error context and suggestions
fn display_error_help(error: &anyhow::Error) {
    println!();
    println!("💡 Troubleshooting tips:");

    let error_str = error.to_string().to_lowercase();

    if error_str.contains("permission denied") {
        println!("   • Try running with elevated permissions (sudo)");
        println!("   • Check that you have read access to the directory");
        println!("   • Use -H flag to skip hidden directories that might cause permission issues");
    } else if error_str.contains("not found") {
        println!("   • Verify the directory path exists");
        println!("   • Use absolute paths to avoid confusion");
        println!("   • Check for typos in the path");
    } else if error_str.contains("invalid size filter") {
        println!("   • Make sure min-size is less than max-size");
        println!("   • Use 0 for max-size to remove the upper limit");
    } else {
        println!("   • Try running with -v flag for more detailed error information");
        println!("   • Check that the target directory is accessible");
        println!("   • Ensure you have sufficient disk space for temporary operations");
    }

    println!("   • Run 'duplicate-finder --help' for usage information");
}