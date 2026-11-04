//! Memory: the engine's estimates of a run's peak (re-exported), and what the host has.

use std::path::{Path, PathBuf};

pub use qsmxt_engine::memory::*;

/// Memory this process can use, and what limited it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Available {
    pub bytes: u64,
    pub limit: Limit,
}

/// What [`Available::bytes`] came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Limit {
    /// The system's `MemAvailable` (`/proc/meminfo`).
    System,
    /// A cgroup memory limit (a Slurm job's `--mem`, a container's memory limit) of this many
    /// bytes, less what the cgroup already uses, below the system's `MemAvailable`.
    Cgroup(u64),
    /// Nothing could be read: an assumed 8 GB.
    Assumed,
}

impl Available {
    /// What limited the figure, for the "Memory: … available" log line: empty for the system's
    /// own figure.
    pub fn describe(&self) -> String {
        match self.limit {
            Limit::System => String::new(),
            Limit::Cgroup(limit) => format!(" (within this job's or container's limit of {})", format_bytes(limit)),
            Limit::Assumed => " (assumed: the system's memory could not be read)".to_string(),
        }
    }
}

const ASSUMED_BYTES: u64 = 8 * 1024 * 1024 * 1024;

/// Get available memory in bytes ([`available_memory`]).
pub fn available_memory_bytes() -> u64 {
    available_memory().bytes
}

/// Memory this process can use: the system's `MemAvailable`, or less when the process runs in a
/// cgroup with a memory limit (a Slurm job, a container), whose limit less its current usage is
/// what is left to it. Every cgroup from the process's own up to the root counts, since a Slurm
/// job sets the limit on the job's cgroup and runs its tasks in cgroups below it. 8 GB when
/// nothing can be read (and on other systems than Linux).
pub fn available_memory() -> Available {
    #[cfg(target_os = "linux")]
    {
        let read = |p: &Path| std::fs::read_to_string(p).ok();
        let system = read(Path::new("/proc/meminfo")).as_deref().and_then(parse_meminfo_available);
        let cgroup = read(Path::new("/proc/self/cgroup"))
            .and_then(|own| cgroup_headroom(&own, Path::new("/sys/fs/cgroup"), read));
        combine(system, cgroup)
    }
    #[cfg(not(target_os = "linux"))]
    {
        combine(None, None)
    }
}

/// The smaller of the system's available memory and a cgroup's headroom `(left, limit)`.
fn combine(system: Option<u64>, cgroup: Option<(u64, u64)>) -> Available {
    match (system, cgroup) {
        (Some(s), Some((left, limit))) if left < s => Available { bytes: left, limit: Limit::Cgroup(limit) },
        (Some(s), _) => Available { bytes: s, limit: Limit::System },
        (None, Some((left, limit))) => Available { bytes: left, limit: Limit::Cgroup(limit) },
        (None, None) => Available { bytes: ASSUMED_BYTES, limit: Limit::Assumed },
    }
}

/// `MemAvailable` in a `/proc/meminfo`, in bytes.
fn parse_meminfo_available(meminfo: &str) -> Option<u64> {
    meminfo.lines()
        .find_map(|l| l.strip_prefix("MemAvailable:"))
        .and_then(|rest| rest.split_whitespace().next())
        .and_then(|kb| kb.parse::<u64>().ok())
        .map(|kb| kb * 1024)
}

/// A cgroup memory limit file's value (`memory.max`, `memory.limit_in_bytes`): `None` when
/// unlimited (`max`, or cgroup v1's page-rounded `i64::MAX`), or unreadable.
fn parse_cgroup_limit(contents: &str) -> Option<u64> {
    let v = contents.trim();
    if v == "max" {
        return None;
    }
    // cgroup v1 writes "unlimited" as i64::MAX rounded down to a page; anything past 2^60 bytes
    // (an exabyte) is no limit in practice.
    v.parse::<u64>().ok().filter(|&b| b < 1 << 60)
}

/// The memory cgroup directories of the process whose `/proc/self/cgroup` is `own`, under the
/// cgroup mount `root`, from its own up to the root: cgroup v2's unified hierarchy (`0::/path`,
/// `memory.max` and `memory.current`), else cgroup v1's memory controller
/// (`N:...memory...:/path` mounted at `root/memory`, `memory.limit_in_bytes` and
/// `memory.usage_in_bytes`). Returns the directories, and the limit and usage file names.
fn cgroup_dirs(own: &str, root: &Path) -> Vec<(PathBuf, &'static str, &'static str)> {
    let mut out = Vec::new();
    let mut push = |base: PathBuf, rel: &str, limit: &'static str, usage: &'static str| {
        // From the process's own cgroup up to the mount's root. Inside a container the path
        // can name a cgroup of the host that is not mounted there; those are skipped by the
        // reader, and the root (the container's own cgroup) still counts.
        let mut dir = base.join(rel.trim_start_matches('/'));
        loop {
            out.push((dir.clone(), limit, usage));
            if dir == base || !dir.pop() {
                break;
            }
        }
    };
    for line in own.lines() {
        let mut parts = line.splitn(3, ':');
        let (Some(_id), Some(controllers), Some(path)) = (parts.next(), parts.next(), parts.next()) else { continue };
        if controllers.is_empty() {
            push(root.to_path_buf(), path, "memory.max", "memory.current");
        } else if controllers.split(',').any(|c| c == "memory") {
            push(root.join("memory"), path, "memory.limit_in_bytes", "memory.usage_in_bytes");
        }
    }
    out
}

/// The tightest cgroup memory limit on the process whose `/proc/self/cgroup` is `own`, as
/// `(limit − usage, limit)` in bytes, reading files through `read`; `None` when no cgroup on
/// the way to the root has a limit.
fn cgroup_headroom(own: &str, root: &Path, read: impl Fn(&Path) -> Option<String>) -> Option<(u64, u64)> {
    cgroup_dirs(own, root).into_iter()
        .filter_map(|(dir, limit, usage)| {
            let limit = parse_cgroup_limit(&read(&dir.join(limit))?)?;
            let used = read(&dir.join(usage)).and_then(|u| u.trim().parse::<u64>().ok()).unwrap_or(0);
            Some((limit.saturating_sub(used), limit))
        })
        .min_by_key(|&(left, _)| left)
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

    const GB: u64 = 1024 * 1024 * 1024;

    #[test]
    fn meminfo_available_is_read_in_bytes() {
        let meminfo = "MemTotal:       32000000 kB\nMemFree:  100 kB\nMemAvailable:   1000 kB\n";
        assert_eq!(parse_meminfo_available(meminfo), Some(1000 * 1024));
        assert_eq!(parse_meminfo_available("MemTotal: 5 kB\n"), None);
    }

    #[test]
    fn unlimited_cgroup_values_are_no_limit() {
        assert_eq!(parse_cgroup_limit("max\n"), None);
        assert_eq!(parse_cgroup_limit("9223372036854771712\n"), None, "cgroup v1's unlimited");
        assert_eq!(parse_cgroup_limit("51539607552\n"), Some(48 * GB));
        assert_eq!(parse_cgroup_limit(""), None);
    }

    /// A fake /sys/fs/cgroup: file path -> contents.
    fn files(entries: &[(&str, &str)]) -> impl Fn(&Path) -> Option<String> {
        let map: std::collections::HashMap<PathBuf, String> =
            entries.iter().map(|(p, c)| (PathBuf::from(p), c.to_string())).collect();
        move |p: &Path| map.get(p).cloned()
    }

    #[test]
    fn a_slurm_jobs_limit_is_found_above_the_tasks_cgroup() {
        // cgroup v2 as Slurm lays it out: the limit on the job, the task below it unlimited.
        let own = "0::/system.slice/slurmstepd.scope/job_42/step_0/user/task_0\n";
        let read = files(&[
            ("/sys/fs/cgroup/system.slice/slurmstepd.scope/job_42/step_0/user/task_0/memory.max", "max"),
            ("/sys/fs/cgroup/system.slice/slurmstepd.scope/job_42/step_0/user/task_0/memory.current", "1000"),
            ("/sys/fs/cgroup/system.slice/slurmstepd.scope/job_42/memory.max", "51539607552"),
            ("/sys/fs/cgroup/system.slice/slurmstepd.scope/job_42/memory.current", "4294967296"),
        ]);
        assert_eq!(cgroup_headroom(own, Path::new("/sys/fs/cgroup"), read), Some((44 * GB, 48 * GB)));
    }

    #[test]
    fn a_container_limit_at_the_cgroup_root_counts() {
        // A container's cgroup namespace: its own cgroup is the root.
        let read = files(&[("/sys/fs/cgroup/memory.max", "17179869184"), ("/sys/fs/cgroup/memory.current", "0")]);
        assert_eq!(cgroup_headroom("0::/\n", Path::new("/sys/fs/cgroup"), read), Some((16 * GB, 16 * GB)));
    }

    #[test]
    fn cgroup_v1_memory_controller_is_read() {
        let own = "12:cpu,cpuacct:/slurm/uid_1/job_7\n4:memory:/slurm/uid_1/job_7/step_batch\n";
        let read = files(&[
            ("/sys/fs/cgroup/memory/slurm/uid_1/job_7/step_batch/memory.limit_in_bytes", "9223372036854771712"),
            ("/sys/fs/cgroup/memory/slurm/uid_1/job_7/memory.limit_in_bytes", "17179869184"),
            ("/sys/fs/cgroup/memory/slurm/uid_1/job_7/memory.usage_in_bytes", "1073741824"),
            ("/sys/fs/cgroup/memory/memory.limit_in_bytes", "9223372036854771712"),
        ]);
        assert_eq!(cgroup_headroom(own, Path::new("/sys/fs/cgroup"), read), Some((15 * GB, 16 * GB)));
    }

    #[test]
    fn no_limit_anywhere_is_none() {
        let read = files(&[("/sys/fs/cgroup/user.slice/memory.max", "max")]);
        assert_eq!(cgroup_headroom("0::/user.slice/x.service\n", Path::new("/sys/fs/cgroup"), read), None);
    }

    #[test]
    fn the_smaller_of_system_and_cgroup_applies() {
        // A 48 GB job on a 1.3 TB node: the job's limit applies.
        let a = combine(Some(1318 * GB), Some((44 * GB, 48 * GB)));
        assert_eq!(a, Available { bytes: 44 * GB, limit: Limit::Cgroup(48 * GB) });
        assert!(a.describe().contains("limit of 48.0 GB"), "{}", a.describe());
        // A loose container limit on a busy machine: the system's figure applies.
        assert_eq!(combine(Some(4 * GB), Some((60 * GB, 64 * GB))).limit, Limit::System);
        assert_eq!(combine(None, None), Available { bytes: 8 * GB, limit: Limit::Assumed });
    }
}
