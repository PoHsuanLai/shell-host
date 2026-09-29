//! The process's resident set size.

/// The `VmRSS` line of a `/proc/<pid>/status` text, in KiB.
pub fn vm_rss_kib(status: &str) -> Option<u64> {
    status
        .lines()
        .find_map(|line| line.strip_prefix("VmRSS:"))
        .and_then(|value| value.trim().strip_suffix("kB"))
        .and_then(|value| value.trim().parse().ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vm_rss_is_read_from_proc_status() {
        let status = "Name:\tdemo_bar\nVmPeak:\t  999 kB\nVmRSS:\t  172800 kB\nThreads:\t9\n";
        assert_eq!(vm_rss_kib(status), Some(172_800));
        assert_eq!(vm_rss_kib("Name:\tx\n"), None);
    }
}
