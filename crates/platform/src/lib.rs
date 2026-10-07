//! Lightweight platform metrics; no whole-system process scanner.
use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};
pub fn memory_bytes() -> Option<u64> {
    #[cfg(target_os = "linux")]
    {
        let status = std::fs::read_to_string("/proc/self/status").ok()?;
        status
            .lines()
            .find(|l| l.starts_with("VmRSS:"))?
            .split_whitespace()
            .nth(1)?
            .parse::<u64>()
            .ok()
            .map(|kb| kb * 1024)
    }
    #[cfg(windows)]
    {
        use windows_sys::Win32::System::{
            ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS},
            Threading::GetCurrentProcess,
        };
        // SAFETY: valid current-process handle and initialized buffer of the required size.
        unsafe {
            let mut counters: PROCESS_MEMORY_COUNTERS = std::mem::zeroed();
            let size = std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
            counters.cb = size;
            if GetProcessMemoryInfo(GetCurrentProcess(), &mut counters, size) != 0 {
                Some(counters.WorkingSetSize as u64)
            } else {
                None
            }
        }
    }
    #[cfg(not(any(target_os = "linux", windows)))]
    {
        None
    }
}
pub struct Metrics {
    pub startup_ms: f64,
    pub frame_ms: f64,
    pub fps: f64,
    pub memory: Option<u64>,
    frames: VecDeque<Instant>,
    last_sample: Instant,
}
impl Metrics {
    pub fn new(started: Instant) -> Self {
        Self {
            startup_ms: started.elapsed().as_secs_f64() * 1000.0,
            frame_ms: 0.0,
            fps: 0.0,
            memory: memory_bytes(),
            frames: VecDeque::new(),
            last_sample: Instant::now(),
        }
    }
    pub fn frame(&mut self, elapsed: Duration) {
        self.frame_ms = elapsed.as_secs_f64() * 1000.0;
        let now = Instant::now();
        self.frames.push_back(now);
        while self
            .frames
            .front()
            .is_some_and(|t| now.duration_since(*t) > Duration::from_secs(1))
        {
            self.frames.pop_front();
        }
        self.fps = self.frames.len() as f64;
        if now.duration_since(self.last_sample) >= Duration::from_secs(1) {
            self.memory = memory_bytes();
            self.last_sample = now;
        }
    }
}
