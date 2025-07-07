use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use anyhow::Result;
use indicatif::{ProgressBar, ProgressStyle};
use tokio::fs;
use tokio::sync::Semaphore;
use tracing::{debug, error, info, warn};

use crate::{Cli, DuplicateFinderError, DuplicateGroup, FileInfo, ScanResult};

/// Main file scanner
///
/// This structure encapsulates all scanning logic and contains
/// configuration that affects the duplicate search process
pub struct FileScanner {
    /// Configuration from CLI
    config: Cli,

    /// Semaphore to limit the number of concurrent operations
    /// This prevents file descriptor exhaustion
    semaphore: Arc<Semaphore>,

    /// Progress bar for user interface
    progress_bar: Option<ProgressBar>,
}

impl FileScanner {
    /// Creates a new scanner instance
    pub fn new(config: Cli) -> Self {
        // Определяем количество потоков
        let thread_count = if config.threads == 0 {
            num_cpus::get() * 2 // По умолчанию: количество ядер * 2
        } else {
            config.threads
        };

        info!("Using {} threads for file processing", thread_count);

        Self {
            config,
            semaphore: Arc::new(Semaphore::new(thread_count)),
            progress_bar: None,
        }
    }

    /// Main entry point for scanning
    ///
    /// This function coordinates the entire process:
    /// 1. Input validation
    /// 2. Filesystem scanning
    /// 3. Hash calculation
    /// 4. Duplicate grouping
    /// 5. Result formation
    pub async fn scan(&mut self) -> Result<ScanResult> {
        let start_time = Instant::now();

        info!("Starting file system scan");

        // Configuration validation
        self.validate_config()?;

        // Create a progress bar if not running in quiet mode
        if !self.config.verbose {
            self.setup_progress_bar();
        }

        // Phase 1: File system scanning
        info!("Phase 1: Discovering files");
        let file_paths = self.discover_files().await?;

        info!("Found {} files to process", file_paths.len());

        if let Some(pb) = &self.progress_bar {
            pb.set_length(file_paths.len() as u64);
            pb.set_message("Processing files...");
        }

        // Phase 2: Process files and calculate hashes
        info!("Phase 2: Processing files and calculating hashes");
        let file_infos = self.process_files(file_paths).await?;

        if let Some(pb) = &self.progress_bar {
            pb.finish_with_message("File processing complete!");
        }

        // Phase 3: Grouping duplicates
        info!("Phase 3: Grouping duplicates");
        let duplicate_groups = self.group_duplicates(file_infos);

        let scan_duration = start_time.elapsed();

        // Form the final result
        let result = ScanResult {
            total_files: duplicate_groups.iter().map(|g| g.files.len()).sum(),
            total_duplicates: duplicate_groups.iter()
                .map(|g| if g.files.len() > 1 { g.files.len() - 1 } else { 0 })
                .sum(),
            total_wasted_space: duplicate_groups.iter()
                .map(|g| g.wasted_space)
                .sum(),
            duplicate_groups: duplicate_groups.into_iter()
                .filter(|g| g.files.len() > 1) // Только реальные дубликаты
                .collect(),
            scan_duration,
            scanned_directory: self.config.directory.clone(),
        };

        info!("Scan completed in {:?}", scan_duration);
        info!("Found {} duplicate groups", result.duplicate_groups.len());
        info!("Total wasted space: {} bytes", result.total_wasted_space);

        Ok(result)
    }

    /// Configuration validation before starting the scan
    fn validate_config(&self) -> Result<(), DuplicateFinderError> {
        // Verify that the directory exists
        if !self.config.directory.exists() {
            return Err(DuplicateFinderError::PathNotFound {
                path: self.config.directory.clone(),
            });
        }

        // Check file size filters
        if self.config.max_size > 0 && self.config.min_size > self.config.max_size {
            return Err(DuplicateFinderError::InvalidSizeFilter {
                min: self.config.min_size,
                max: self.config.max_size,
            });
        }

        Ok(())
    }

    /// Configure a progress bar for visual feedback
    fn setup_progress_bar(&mut self) {
        let pb = ProgressBar::new(0);
        pb.set_style(
            ProgressStyle::default_bar()
                .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} {msg}")
                .expect("Invalid progress bar template")
                .progress_chars("#>-"),
        );
        self.progress_bar = Some(pb);
    }

    /// Recursive file discovery in a directory
    ///
    /// Uses an iterative approach instead of recursion to avoid
    /// async recursion issues and better stack usage control
    async fn discover_files(&self) -> Result<Vec<PathBuf>> {
        let mut file_paths = Vec::new();

        // Use a stack to imitate recursion
        // Each element contains (directory_path, current_depth)
        let mut dir_stack = vec![(self.config.directory.clone(), 0)];

        // Debug and monitoring statistics
        let mut directories_processed = 0;
        let mut max_stack_size = 0;

        while let Some((current_dir, current_depth)) = dir_stack.pop() {
            directories_processed += 1;
            max_stack_size = max_stack_size.max(dir_stack.len());

            // Protection against accidental infinite recursion
            const MAX_PENDING_DIRS: usize = 10000;
            if dir_stack.len() > MAX_PENDING_DIRS {
                warn!(
                    "Too many pending directories ({}), possible deep directory structure. Limiting scan.",
                    dir_stack.len()
                );
                break;
            }

            // Check depth limit
            if self.config.max_depth > 0 && current_depth >= self.config.max_depth {
                debug!("Max depth {} reached at {}", self.config.max_depth, current_dir.display());
                continue;
            }

            debug!("Scanning directory: {} (depth: {}, stack: {})",
                current_dir.display(), current_depth, dir_stack.len());

            // Try to read directory contents
            let mut read_dir = match fs::read_dir(&current_dir).await {
                Ok(rd) => rd,
                Err(e) => {
                    warn!("Cannot read directory {}: {}", current_dir.display(), e);
                    continue; // Continue with other directories
                }
            };

            // Buffer new directories before adding them to the stack
            // This helps with performance when dealing with a large number of subdirectories
            let mut new_directories = Vec::new();

            // Process each entry in the directory
            while let Some(entry_result) = read_dir.next_entry().await.transpose() {
                let entry = match entry_result {
                    Ok(entry) => entry,
                    Err(e) => {
                        warn!("Error reading directory entry in {}: {}", current_dir.display(), e);
                        continue;
                    }
                };

                let path = entry.path();

                // Check if hidden files should be skipped
                if !self.config.include_hidden && self.is_hidden(&path) {
                    debug!("Skipping hidden path: {}", path.display());
                    continue;
                }

                // Check symbolic links
                if path.is_symlink() && !self.config.follow_symlinks {
                    debug!("Skipping symlink: {}", path.display());
                    continue;
                }

                let metadata = match entry.metadata().await {
                    Ok(metadata) => metadata,
                    Err(e) => {
                        warn!("Cannot read metadata for {}: {}", path.display(), e);
                        continue;
                    }
                };

                if metadata.is_dir() {
                    // Buffer the directory for later scanning
                    new_directories.push((path, current_depth + 1));
                } else if metadata.is_file() {
                    // Check file size filters
                    if self.file_passes_size_filter(metadata.len()) {
                        file_paths.push(path);
                    }
                }
            }

            // Add new directories to the stack
            // Reverse order for breadth-first traversal
            for dir_entry in new_directories.into_iter().rev() {
                dir_stack.push(dir_entry);
            }
        }

        info!(
            "Directory scan completed: {} directories processed, {} files found, max stack size: {}",
            directories_processed,
            file_paths.len(),
            max_stack_size
        );

        Ok(file_paths)
    }

    /// Checks if a path is hidden
    fn is_hidden(&self, path: &Path) -> bool {
        path.file_name()
            .and_then(|name| name.to_str())
            .map(|name| name.starts_with('.'))
            .unwrap_or(false)
    }

    /// Checks if a file passes a size filter
    fn file_passes_size_filter(&self, size: u64) -> bool {
        // Check empty files exclusion
        if self.config.exclude_empty && size == 0 {
            return false;
        }

        // Check minimum size
        if size < self.config.min_size {
            return false;
        }

        // Check maximum size (0 means no limit)
        if self.config.max_size > 0 && size > self.config.max_size {
            return false;
        }

        true
    }

    /// Process files and calculate their hashes
    ///
    /// Uses controlled parallelism through semaphore for efficient
    /// processing of large numbers of files without a system overload
    async fn process_files(&self, file_paths: Vec<PathBuf>) -> Result<Vec<FileInfo>> {
        let mut file_infos = Vec::new();
        let mut tasks = Vec::new();

        // Process files in batches to manage memory
        const BATCH_SIZE: usize = 100;

        for batch in file_paths.chunks(BATCH_SIZE) {
            // Create tasks for the current batch
            for path in batch {
                let path_owned = path.clone();
                let semaphore = Arc::clone(&self.semaphore);
                let progress_bar = self.progress_bar.clone();

                let task = tokio::spawn(async move {
                    // Acquire permission from the semaphore
                    let _permit = semaphore.acquire().await
                        .expect("Semaphore should not be closed");

                    let result = FileInfo::from_path(&path_owned).await;

                    // Update the progress bar
                    if let Some(pb) = &progress_bar {
                        pb.inc(1);
                    }

                    match result {
                        Ok(file_info) => {
                            debug!("Processed file: {}", path_owned.display());
                            Some(file_info)
                        }
                        Err(e) => {
                            error!("Failed to process file {}: {}", path_owned.display(), e);
                            None
                        }
                    }
                });

                tasks.push(task);
            }

            // Wait for all tasks in the current batch to complete
            for task in tasks.drain(..) {
                match task.await {
                    Ok(Some(file_info)) => file_infos.push(file_info),
                    Ok(None) => {} // Файл не удалось обработать, пропускаем
                    Err(e) => error!("Task panicked: {}", e),
                }
            }
        }

        Ok(file_infos)
    }

    /// Groups files by their hashes to find duplicates
    fn group_duplicates(&self, file_infos: Vec<FileInfo>) -> Vec<DuplicateGroup> {
        let mut groups: HashMap<String, Vec<FileInfo>> = HashMap::new();

        // Group files by hash
        for file_info in file_infos {
            groups.entry(file_info.hash.clone())
                .or_insert_with(Vec::new)
                .push(file_info);
        }

        // Convert to DuplicateGroup
        groups.into_iter()
            .map(|(hash, mut files)| {
                // Сортируем файлы по времени модификации (самые старые первыми)
                files.sort_by_key(|f| f.modified);

                let size = files.first().map(|f| f.size).unwrap_or(0);
                let total_size = size * files.len() as u64;
                let wasted_space = if files.len() > 1 {
                    size * (files.len() as u64 - 1)
                } else {
                    0
                };

                DuplicateGroup {
                    hash,
                    size,
                    files,
                    total_size,
                    wasted_space,
                }
            })
            .collect()
    }
}