use std::collections::VecDeque;
use std::time::Instant;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TransferStats {
    pub total_files: usize,
    pub completed_files: usize,
    pub skipped_files: usize,
    pub failed_files: usize,
    pub total_bytes: u64,
    pub transferred_bytes: u64,
    pub current_speed_bps: u64, // bytes per second
    pub average_speed_bps: u64,
    pub eta_seconds: u64,
    pub overall_percentage: f32,
    pub current_file_name: String,
    pub current_file_transferred: u64,
    pub current_file_size: u64,
}

#[derive(Clone)]
pub struct SpeedCalculator {
    start_time: Instant,
    samples: VecDeque<(Instant, u64)>, // (time, cumulative bytes)
    window_duration_secs: f64,
}

impl SpeedCalculator {
    pub fn new(window_duration_secs: f64) -> Self {
        let now = Instant::now();
        let mut samples = VecDeque::new();
        samples.push_back((now, 0));

        Self {
            start_time: now,
            samples,
            window_duration_secs,
        }
    }

    pub fn record_progress(&mut self, total_bytes: u64) {
        let now = Instant::now();
        self.samples.push_back((now, total_bytes));

        // Evict samples older than window_duration
        while let Some(&(time, _)) = self.samples.front() {
            if now.duration_since(time).as_secs_f64() > self.window_duration_secs && self.samples.len() > 2 {
                self.samples.pop_front();
            } else {
                break;
            }
        }
    }

    /// Calculates current rolling speed in bytes per second.
    pub fn current_speed(&self) -> u64 {
        if self.samples.len() < 2 {
            return 0;
        }

        let (front_time, front_bytes) = *self.samples.front().unwrap();
        let (back_time, back_bytes) = *self.samples.back().unwrap();

        let elapsed = back_time.duration_since(front_time).as_secs_f64();
        if elapsed <= 0.001 {
            return 0;
        }

        let bytes_diff = back_bytes.saturating_sub(front_bytes);
        (bytes_diff as f64 / elapsed) as u64
    }

    /// Calculates overall average speed from start.
    pub fn average_speed(&self, total_bytes: u64) -> u64 {
        let elapsed = self.start_time.elapsed().as_secs_f64();
        if elapsed <= 0.001 {
            return 0;
        }
        (total_bytes as f64 / elapsed) as u64
    }

    /// Estimates remaining time (ETA) in seconds.
    pub fn estimate_eta(&self, remaining_bytes: u64) -> u64 {
        let speed = self.current_speed();
        if speed == 0 {
            return 0;
        }
        remaining_bytes / speed
    }
}
