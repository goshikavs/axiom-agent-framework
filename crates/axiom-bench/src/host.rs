//! Host/software fingerprint. Fields that cannot be read are left unknown.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostInfo {
    pub os: String,
    pub arch: String,
    pub os_family: String,
    pub parallelism: Option<u32>,
    pub rustc_version: Option<String>,
    pub axiom_version: String,
    pub cpu_brand: Option<String>,
    pub rss_bytes_after: Option<u64>,
    pub peak_working_set_bytes: Option<u64>,
    pub notes: Vec<String>,
}

pub fn collect() -> HostInfo {
    let mut notes = vec![
        "Database overhead is not applicable: this benchmark is in-process graph analysis.".into(),
        "Model API cost is unknown: no LLM calls are made.".into(),
        "CPU time is not sampled; latency is wall-clock Instant.".into(),
    ];
    let cpu_brand = probe_cpu_brand();
    if cpu_brand.is_none() {
        notes.push("CPU brand could not be probed on this host.".into());
    }
    HostInfo {
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        os_family: std::env::consts::FAMILY.to_string(),
        parallelism: std::thread::available_parallelism().ok().map(|n| n.get() as u32),
        rustc_version: probe_rustc(),
        axiom_version: env!("CARGO_PKG_VERSION").to_string(),
        cpu_brand,
        rss_bytes_after: rss_bytes(),
        peak_working_set_bytes: peak_rss_bytes(),
        notes,
    }
}

fn probe_rustc() -> Option<String> {
    let out = std::process::Command::new("rustc").arg("--version").output().ok()?;
    if !out.status.success() {
        return None;
    }
    String::from_utf8(out.stdout).ok().map(|s| s.trim().to_string())
}

fn probe_cpu_brand() -> Option<String> {
    #[cfg(windows)]
    {
        let out = std::process::Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                "(Get-CimInstance Win32_Processor | Select-Object -First 1 -ExpandProperty Name)",
            ])
            .output()
            .ok()?;
        if !out.status.success() {
            return None;
        }
        let s = String::from_utf8(out.stdout).ok()?;
        let t = s.trim();
        if t.is_empty() {
            None
        } else {
            Some(t.to_string())
        }
    }
    #[cfg(not(windows))]
    {
        None
    }
}

#[cfg(windows)]
fn memory_counters() -> Option<(u64, u64)> {
    #[repr(C)]
    struct ProcessMemoryCounters {
        cb: u32,
        page_fault_count: u32,
        peak_working_set_size: usize,
        working_set_size: usize,
        quota_peak_paged_pool_usage: usize,
        quota_paged_pool_usage: usize,
        quota_peak_non_paged_pool_usage: usize,
        quota_non_paged_pool_usage: usize,
        pagefile_usage: usize,
        peak_pagefile_usage: usize,
    }
    extern "system" {
        fn GetCurrentProcess() -> *mut std::ffi::c_void;
        fn K32GetProcessMemoryInfo(
            process: *mut std::ffi::c_void,
            ppsmemcounters: *mut ProcessMemoryCounters,
            cb: u32,
        ) -> i32;
    }
    unsafe {
        let mut info: ProcessMemoryCounters = std::mem::zeroed();
        info.cb = std::mem::size_of::<ProcessMemoryCounters>() as u32;
        let ok = K32GetProcessMemoryInfo(GetCurrentProcess(), &mut info, info.cb);
        if ok == 0 {
            return None;
        }
        Some((info.working_set_size as u64, info.peak_working_set_size as u64))
    }
}

#[cfg(windows)]
pub fn rss_bytes() -> Option<u64> {
    memory_counters().map(|(ws, _)| ws)
}

#[cfg(windows)]
pub fn peak_rss_bytes() -> Option<u64> {
    memory_counters().map(|(_, peak)| peak)
}

#[cfg(not(windows))]
pub fn rss_bytes() -> Option<u64> {
    let text = std::fs::read_to_string("/proc/self/status").ok()?;
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("VmRSS:") {
            let kb: u64 = rest.split_whitespace().next()?.parse().ok()?;
            return Some(kb * 1024);
        }
    }
    None
}

#[cfg(not(windows))]
pub fn peak_rss_bytes() -> Option<u64> {
    let text = std::fs::read_to_string("/proc/self/status").ok()?;
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("VmHWM:") {
            let kb: u64 = rest.split_whitespace().next()?.parse().ok()?;
            return Some(kb * 1024);
        }
    }
    None
}
