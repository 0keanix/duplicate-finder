use std::path::Path;

use anyhow::Result;
use chrono::{DateTime, Local};
use serde_json;
use tokio::fs;

use crate::{Cli, DuplicateGroup, OutputFormat, ScanResult};

/// Output formatter for scan results
///
/// This component is responsible for presenting results in various formats
/// and providing a convenient user interface
pub struct OutputFormatter<'a> {
    config: &'a Cli,
}

impl<'a> OutputFormatter<'a> {
    /// Creates a new formatter instance
    pub fn new(config: &'a Cli) -> Self {
        Self { config }
    }

    /// Main function for displaying results
    ///
    /// Selects an output format based on configuration and displays results
    pub async fn display_results(&self, scan_result: &ScanResult) -> Result<()> {
        match self.config.output_format {
            OutputFormat::Text => self.display_text_format(scan_result).await,
            OutputFormat::Json => self.display_json_format(scan_result).await,
        }
    }

    /// Saves results to a file
    pub async fn save_to_file(&self, scan_result: &ScanResult, output_path: &Path) -> Result<()> {
        let content = match self.config.output_format {
            OutputFormat::Text => self.format_as_text(scan_result),
            OutputFormat::Json => serde_json::to_string_pretty(scan_result)?,
        };

        fs::write(output_path, content).await?;
        Ok(())
    }

    /// Display results in text format
    ///
    /// Creates a beautiful, human-readable report using Unicode symbols
    /// for better visual perception 
    async fn display_text_format(&self, scan_result: &ScanResult) -> Result<()> {
        println!("{}", self.format_as_text(scan_result));
        Ok(())
    }

    /// Formats results as text
    fn format_as_text(&self, scan_result: &ScanResult) -> String {
        let mut output = String::new();

        // Report header
        output.push_str("📊 SCAN RESULTS\n");
        output.push_str(&"═".repeat(50));
        output.push('\n');

        // General statistics
        output.push_str(&format!("📁 Scanned Directory: {}\n", scan_result.scanned_directory.display()));
        output.push_str(&format!("⏱️  Scan Duration: {:?}\n", scan_result.scan_duration));
        output.push_str(&format!("📄 Total Files: {}\n", scan_result.total_files));
        output.push_str(&format!("🔄 Duplicate Files: {}\n", scan_result.total_duplicates));
        output.push_str(&format!("📦 Duplicate Groups: {}\n", scan_result.duplicate_groups.len()));
        output.push_str(&format!("💾 Wasted Space: {}\n", format_bytes(scan_result.total_wasted_space)));
        output.push('\n');

        if scan_result.duplicate_groups.is_empty() {
            output.push_str("🎉 No duplicates found! Your file system is clean.\n");
            return output;
        }

        // Sort duplicate groups by wasted space size (descending)
        let mut sorted_groups = scan_result.duplicate_groups.clone();
        sorted_groups.sort_by(|a, b| b.wasted_space.cmp(&a.wasted_space));

        // Detailed information about duplicate groups
        output.push_str("🔍 DUPLICATE GROUPS (sorted by wasted space)\n");
        output.push_str(&"─".repeat(50));
        output.push('\n');

        for (index, group) in sorted_groups.iter().enumerate() {
            output.push_str(&self.format_duplicate_group(group, index + 1));
            output.push('\n');
        }

        // Final recommendations
        output.push_str(&self.generate_recommendations(scan_result));

        output
    }

    /// Formats one duplicate group
    fn format_duplicate_group(&self, group: &DuplicateGroup, group_number: usize) -> String {
        let mut output = String::new();

        output.push_str(&format!("📋 Group #{} ({})\n", group_number, format_bytes(group.size)));
        output.push_str(&format!("   💰 Wasted space: {}\n", format_bytes(group.wasted_space)));
        output.push_str(&format!("   🔐 Hash: {}...\n", &group.hash[..16])); // Show the first 16 characters of hash
        output.push_str(&format!("   📊 {} duplicate files:\n", group.files.len()));

        for (file_index, file) in group.files.iter().enumerate() {
            let file_marker = if file_index == 0 { "📌" } else { "🔄" }; // The first file is considered original
            let modified_time = format_system_time(file.modified);

            output.push_str(&format!("     {} {}\n", file_marker, file.path.display()));
            output.push_str(&format!("        📅 Modified: {}\n", modified_time));

            if let Some(created) = file.created {
                let created_time = format_system_time(created);
                output.push_str(&format!("        🆕 Created: {}\n", created_time));
            }
        }

        output
    }

    /// Generates recommendations based on scan results
    fn generate_recommendations(&self, scan_result: &ScanResult) -> String {
        let mut recommendations = String::new();

        recommendations.push_str("💡 RECOMMENDATIONS\n");
        recommendations.push_str(&"─".repeat(50));
        recommendations.push('\n');

        if scan_result.total_wasted_space == 0 {
            recommendations.push_str("✨ Your file system is perfectly organized! No cleanup needed.\n");
            return recommendations;
        }

        // Cleanup recommendations
        let savings_gb = scan_result.total_wasted_space as f64 / (1024.0 * 1024.0 * 1024.0);

        if savings_gb > 1.0 {
            recommendations.push_str(&format!("🚨 High Impact: You can save {:.2} GB by removing duplicates!\n", savings_gb));
        } else if scan_result.total_wasted_space > 100 * 1024 * 1024 { // > 100 MB
            recommendations.push_str("⚠️  Medium Impact: Consider cleaning up duplicate files.\n");
        } else {
            recommendations.push_str("ℹ️  Low Impact: Duplicates present but space savings are minimal.\n");
        }

        recommendations.push('\n');
        recommendations.push_str("🛠️  Cleanup Strategy:\n");
        recommendations.push_str("   1. Review each group carefully before deletion\n");
        recommendations.push_str("   2. Keep the oldest file (marked with 📌) as the original\n");
        recommendations.push_str("   3. Consider using hard links instead of deletion for safety\n");
        recommendations.push_str("   4. Always backup important data before cleanup\n");

        // Statistics by file types (if extensions exist)
        let file_extensions = self.analyze_file_extensions(scan_result);
        if !file_extensions.is_empty() {
            recommendations.push('\n');
            recommendations.push_str("📈 File Types Analysis:\n");
            for (extension, count) in file_extensions.iter().take(5) { // Top 5 file extensions
                recommendations.push_str(&format!("   {} files: {}\n", extension, count));
            }
        }

        recommendations
    }

    /// Analyzes file extensions for statistics
    fn analyze_file_extensions(&self, scan_result: &ScanResult) -> Vec<(String, usize)> {
        use std::collections::HashMap;

        let mut extension_counts: HashMap<String, usize> = HashMap::new();

        for group in &scan_result.duplicate_groups {
            for file in &group.files {
                let extension = file.path
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .unwrap_or("(no extension)")
                    .to_lowercase();

                *extension_counts.entry(extension).or_insert(0) += 1;
            }
        }

        let mut extensions: Vec<(String, usize)> = extension_counts.into_iter().collect();
        extensions.sort_by(|a, b| b.1.cmp(&a.1)); // Sort by count (descending)

        extensions
    }

    /// Display results in JSON format
    async fn display_json_format(&self, scan_result: &ScanResult) -> Result<()> {
        let json_output = serde_json::to_string_pretty(scan_result)?;
        println!("{}", json_output);
        Ok(())
    }
}

/// Formats size in bytes into a human-readable format
///
/// Converts large numbers into convenient units (KB, MB, GB, TB)
fn format_bytes(bytes: u64) -> String {
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

/// Formats SystemTime into a readable string
fn format_system_time(time: std::time::SystemTime) -> String {
    match time.duration_since(std::time::UNIX_EPOCH) {
        Ok(duration) => {
            let datetime = DateTime::from_timestamp(duration.as_secs() as i64, 0)
                .unwrap_or_default();
            let local_datetime: DateTime<Local> = datetime.into();
            local_datetime.format("%Y-%m-%d %H:%M:%S").to_string()
        }
        Err(_) => "Unknown".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn test_format_system_time() {
        let unix_epoch = std::time::UNIX_EPOCH;
        let formatted = format_system_time(unix_epoch);
        // Check that the function doesn't panic and returns a string
        assert!(!formatted.is_empty());
    }
}