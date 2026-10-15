//! Memory: the engine's estimates of a run's peak (re-exported), and what the host has.

pub use qsmxt_engine::memory::*;

/// Get available system memory in bytes.
///
/// On Linux, reads `MemAvailable` from `/proc/meminfo`.
/// Falls back to 8 GB if detection fails or on unsupported platforms.
pub fn available_memory_bytes() -> u64 {
    #[cfg(target_os = "linux")]
    {
        if let Ok(contents) = std::fs::read_to_string("/proc/meminfo") {
            for line in contents.lines() {
                if let Some(rest) = line.strip_prefix("MemAvailable:") {
                    if let Some(kb_str) = rest.split_whitespace().next() {
                        if let Ok(kb) = kb_str.parse::<u64>() {
                            return kb * 1024;
                        }
                    }
                }
            }
        }
    }
    // Fallback: 8 GB
    8 * 1024 * 1024 * 1024
}

/// Get current process resident memory (RSS) in bytes.
///
/// On Linux, reads `VmRSS` from `/proc/self/status`.
/// Returns 0 if detection fails or on unsupported platforms.
pub fn process_rss_bytes() -> u64 {
    #[cfg(target_os = "linux")]
    {
        if let Ok(contents) = std::fs::read_to_string("/proc/self/status") {
            for line in contents.lines() {
                if let Some(rest) = line.strip_prefix("VmRSS:") {
                    if let Some(kb_str) = rest.split_whitespace().next() {
                        if let Ok(kb) = kb_str.parse::<u64>() {
                            return kb * 1024;
                        }
                    }
                }
            }
        }
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_available_memory_positive() {
        let mem = available_memory_bytes();
        assert!(mem > 0, "Available memory should be positive");
    }
}
