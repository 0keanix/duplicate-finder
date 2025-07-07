use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use std::path::Path;
use tempfile::NamedTempFile;
use tokio::io::AsyncWriteExt;

use duplicate_finder::FileInfo;

/// Creates a temporary file with the specified size for testing
async fn create_test_file(size: usize) -> NamedTempFile {
    let temp_file = NamedTempFile::new().expect("Failed to create temp file");

    // Fill the file with data
    let data = vec![0u8; size];
    let mut async_file = tokio::fs::File::create(temp_file.path()).await
        .expect("Failed to create async file");

    async_file.write_all(&data).await.expect("Failed to write test data");
    async_file.flush().await.expect("Failed to flush file");

    temp_file
}

/// File size constants for benchmarking (in bytes)
const FILE_SIZE_1KB: usize = 1024;
const FILE_SIZE_10KB: usize = 10 * FILE_SIZE_1KB;
const FILE_SIZE_100KB: usize = 100 * FILE_SIZE_1KB;
const FILE_SIZE_1MB: usize = 1024 * FILE_SIZE_1KB;
const FILE_SIZE_10MB: usize = 10 * FILE_SIZE_1MB;

/// File hashing benchmark for different file sizes
///
/// This benchmark measures the performance of our hashing algorithm
/// on files of various sizes to understand scalability
fn bench_file_hashing_by_size(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    // Testing different file sizes using named constants
    let file_sizes = vec![
        FILE_SIZE_1KB,
        FILE_SIZE_10KB,
        FILE_SIZE_100KB,
        FILE_SIZE_1MB,
        FILE_SIZE_10MB,
    ];

    let mut group = c.benchmark_group("file_hashing_by_size");

    for size in file_sizes {
        // Configure throughput for better metrics
        group.throughput(Throughput::Bytes(size as u64));
        group.bench_with_input(
            BenchmarkId::new("hash_file", format_size(size)),
            &size,
            |b, &size| {
                b.iter(|| {
                    rt.block_on(async {
                        // Create a file for each iteration (to avoid caching)
                        let temp_file = create_test_file(size).await;
                        // Measure the hashing time
                        let result = FileInfo::from_path(temp_file.path()).await;
                        // black_box prevents compiler optimization
                        black_box(result.expect("Failed to hash file"));
                    })
                });
            },
        );
    }
    group.finish();
}

/// Buffer size optimization benchmark
///
/// This test helps find the optimal buffer size for IO operations
fn bench_buffer_sizes(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    // Testing different buffer sizes
    let buffer_sizes = vec![
        4 * FILE_SIZE_1KB,      // 4 KB
        8 * FILE_SIZE_1KB,      // 8 KB (current size)
        16 * FILE_SIZE_1KB,     // 16 KB
        32 * FILE_SIZE_1KB,     // 32 KB
        64 * FILE_SIZE_1KB,     // 64 KB
        128 * FILE_SIZE_1KB,    // 128 KB
    ];

    let mut group = c.benchmark_group("buffer_size_optimization");

    // Use a medium-sized file for testing
    let test_file_size = 1024 * 1024; // 1 MB

    for buffer_size in buffer_sizes {
        group.bench_with_input(
            BenchmarkId::new("buffer", format_size(buffer_size)),
            &buffer_size,
            |b, &buffer_size| {
                b.iter(|| {
                    rt.block_on(async {
                        let temp_file = create_test_file(test_file_size).await;

                        // Test hashing with a specific buffer size
                        let result = hash_file_with_buffer_size(temp_file.path(), buffer_size).await;

                        black_box(result.expect("Failed to hash file"));
                    })
                });
            },
        );
    }

    group.finish();
}

/// Alternative implementation of hashing with configurable buffer size
///
/// This function is used only for benchmarks to test
/// different buffer sizes
async fn hash_file_with_buffer_size(path: &Path, buffer_size: usize) -> Result<String, std::io::Error> {
    use sha2::{Digest, Sha256};
    use tokio::io::AsyncReadExt;
    use tokio::fs;

    let mut file = fs::File::open(path).await?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; buffer_size];

    loop {
        let bytes_read = file.read(&mut buffer).await?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }

    Ok(format!("{:x}", hasher.finalize()))
}

/// Parallel file hashing benchmark
///
/// Tests how our system handles concurrent processing
/// of multiple files of different sizes
fn bench_parallel_hashing(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    let file_counts = vec![1, 2, 4, 8, 16, 32];
    let file_size = 100 * 1024; // 100 KB per file

    let mut group = c.benchmark_group("parallel_hashing");

    for count in file_counts {
        group.bench_with_input(
            BenchmarkId::new("files", count),
            &count,
            |b, &count| {
                b.iter(|| {
                    rt.block_on(async {
                        // Create multiple files
                        let mut temp_files = Vec::new();
                        for _ in 0..count {
                            temp_files.push(create_test_file(file_size).await);
                        }

                        // Hash all files in parallel
                        let tasks: Vec<_> = temp_files.iter()
                            .map(|f| FileInfo::from_path(f.path()))
                            .collect();

                        let results = futures::future::join_all(tasks).await;

                        // Verify that all hashes are computed successfully
                        for result in results {
                            black_box(result.expect("Failed to hash file"));
                        }
                    })
                });
            },
        );
    }

    group.finish();
}

/// Comparative benchmark of different hashing algorithms
///
/// Comparing SHA-256 with other algorithms to understand trade-offs
fn bench_hash_algorithms(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    let test_size = 1024 * 1024; // 1 MB test file

    let mut group = c.benchmark_group("hash_algorithms");
    group.throughput(Throughput::Bytes(test_size as u64));

    // SHA-256 (current algorithm)
    group.bench_function("sha256", |b| {
        b.iter(|| {
            rt.block_on(async {
                let temp_file = create_test_file(test_size).await;
                let result = hash_with_sha256(temp_file.path()).await;
                black_box(result.expect("SHA-256 failed"))
            })
        });
    });

    // SHA-1 (faster but less secure)
    group.bench_function("sha1", |b| {
        b.iter(|| {
            rt.block_on(async {
                let temp_file = create_test_file(test_size).await;
                let result = hash_with_sha1(temp_file.path()).await;
                black_box(result.expect("SHA-1 failed"))
            })
        });
    });

    // BLAKE3 (modern and very fast)
    group.bench_function("blake3", |b| {
        b.iter(|| {
            rt.block_on(async {
                let temp_file = create_test_file(test_size).await;
                let result = hash_with_blake3(temp_file.path()).await;
                black_box(result.expect("BLAKE3 failed"))
            })
        });
    });

    group.finish();
}

// Implementations of different hashing algorithms for comparison
async fn hash_with_sha256(path: &Path) -> Result<String, std::io::Error> {
    use sha2::{Digest, Sha256};
    use tokio::io::AsyncReadExt;
    use tokio::fs;

    let mut file = fs::File::open(path).await?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 65536];

    loop {
        let bytes_read = file.read(&mut buffer).await?;
        if bytes_read == 0 { break; }
        hasher.update(&buffer[..bytes_read]);
    }

    Ok(format!("{:x}", hasher.finalize()))
}

async fn hash_with_sha1(path: &Path) -> Result<String, std::io::Error> {
    use sha1::{Digest, Sha1};
    use tokio::io::AsyncReadExt;
    use tokio::fs;

    let mut file = fs::File::open(path).await?;
    let mut hasher = Sha1::new();
    let mut buffer = vec![0u8; 65536];

    loop {
        let bytes_read = file.read(&mut buffer).await?;
        if bytes_read == 0 { break; }
        hasher.update(&buffer[..bytes_read]);
    }

    Ok(format!("{:x}", hasher.finalize()))
}

async fn hash_with_blake3(path: &Path) -> Result<String, std::io::Error> {
    use tokio::io::AsyncReadExt;
    use tokio::fs;

    let mut file = fs::File::open(path).await?;
    let mut hasher = blake3::Hasher::new();
    let mut buffer = vec![0u8; 65536];

    loop {
        let bytes_read = file.read(&mut buffer).await?;
        if bytes_read == 0 { break; }
        hasher.update(&buffer[..bytes_read]);
    }

    Ok(hasher.finalize().to_hex().to_string())
}

/// Helper function for size formatting
fn format_size(size: usize) -> String {
    if size >= 1024 * 1024 {
        format!("{}MB", size / (1024 * 1024))
    } else if size >= 1024 {
        format!("{}KB", size / 1024)
    } else {
        format!("{}B", size)
    }
}

// Configure Criterion and group benchmarks
criterion_group!(
    benches,
    bench_file_hashing_by_size,
    bench_buffer_sizes,
    bench_parallel_hashing,
    bench_hash_algorithms
);

criterion_main!(benches);