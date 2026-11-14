//! How much CPU and memory this process uses, shown next to the FPS counter.
//! CPU is a share of the whole machine (all cores), the same way Task Manager shows it.

#[derive(Default)]
pub struct Usage {
    last: Option<(f64, f64)>, // (wall seconds, cpu seconds)
    /// Percent of total CPU time across all cores.
    pub cpu_pct: f64,
    /// Working set (RAM in use by the process), in MB.
    pub ram_mb: f64,
}

impl Usage {
    pub fn sample(&mut self, now: f64) {
        let Some((cpu, ram)) = read() else { return };
        if let Some((w0, c0)) = self.last {
            let cores = std::thread::available_parallelism().map_or(1, |n| n.get()) as f64;
            if now > w0 {
                self.cpu_pct = ((cpu - c0) / (now - w0) / cores * 100.0).max(0.0);
            }
        }
        self.last = Some((now, cpu));
        self.ram_mb = ram as f64 / (1024.0 * 1024.0);
    }

    pub fn available(&self) -> bool {
        self.last.is_some()
    }
}

/// (cpu seconds used so far, working set bytes)
#[cfg(windows)]
fn read() -> Option<(f64, usize)> {
    use std::ffi::c_void;

    #[repr(C)]
    #[derive(Default)]
    struct FileTime {
        low: u32,
        high: u32,
    }
    impl FileTime {
        fn secs(&self) -> f64 {
            (((self.high as u64) << 32) | self.low as u64) as f64 / 1e7 // 100 ns ticks
        }
    }

    #[repr(C)]
    #[derive(Default)]
    struct MemoryCounters {
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

    #[link(name = "kernel32")]
    extern "system" {
        fn GetCurrentProcess() -> *mut c_void;
        fn GetProcessTimes(
            process: *mut c_void,
            creation: *mut FileTime,
            exit: *mut FileTime,
            kernel: *mut FileTime,
            user: *mut FileTime,
        ) -> i32;
        fn K32GetProcessMemoryInfo(process: *mut c_void, counters: *mut MemoryCounters, cb: u32) -> i32;
    }

    unsafe {
        let me = GetCurrentProcess();
        let (mut c, mut e, mut k, mut u) = Default::default();
        if GetProcessTimes(me, &mut c, &mut e, &mut k, &mut u) == 0 {
            return None;
        }
        let mut m = MemoryCounters { cb: std::mem::size_of::<MemoryCounters>() as u32, ..Default::default() };
        if K32GetProcessMemoryInfo(me, &mut m, m.cb) == 0 {
            return None;
        }
        Some((k.secs() + u.secs(), m.working_set_size))
    }
}

#[cfg(not(windows))]
fn read() -> Option<(f64, usize)> {
    None
}
