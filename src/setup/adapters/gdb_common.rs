//! Shared utilities for GDB-based adapters (GDB and CUDA-GDB)

/// Extracts version string from GDB --version output
///
/// Returns first token starting with digit (handles varying GDB output formats)
pub fn parse_gdb_version(output: &str) -> Option<String> {
    output
        .lines()
        .next()
        .and_then(|line| {
            line.split_whitespace()
                .find(|token| token.chars().next().map_or(false, |c| c.is_ascii_digit()))
        })
        .map(|s| s.to_string())
}

/// Checks if GDB version meets DAP support requirement (≥14.1)
///
/// Returns false on parse failure to prevent launching incompatible GDB
pub fn is_gdb_version_sufficient(version: &str) -> bool {
    let parts: Vec<&str> = version.split('.').collect();
    let Some(major_str) = parts.get(0) else {
        return false;
    };
    let Some(minor_str) = parts.get(1) else {
        return false;
    };
    let Ok(major) = major_str.parse::<u32>() else {
        return false;
    };
    let Ok(minor) = minor_str.parse::<u32>() else {
        return false;
    };

    major > 14 || (major == 14 && minor >= 1)
}

/// Retrieves GDB version by executing --version flag
///
/// Returns None on exec failure or unparseable output
pub async fn get_gdb_version(path: &std::path::PathBuf) -> Option<String> {
    let output = tokio::process::Command::new(path)
        .arg("--version")
        .output()
        .await
        .ok()?;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        parse_gdb_version(&stdout)
    } else {
        None
    }
}
