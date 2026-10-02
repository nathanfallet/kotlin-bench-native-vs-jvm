//! Port of `Result.kt`: sampling and the `RESULT {...}` JSON line, field for field.

use std::time::Instant;

pub fn platform_name() -> String {
    format!("rust-{}-{}", std::env::consts::OS, std::env::consts::ARCH)
}

pub fn available_processors() -> usize {
    std::thread::available_parallelism().map_or(1, |n| n.get())
}

pub struct BenchResult {
    pub workload: String,
    pub unit: String,
    pub warmup: usize,
    pub samples: Vec<i64>,
    pub checksum: i64,
    pub setup_nanos: i64,
    pub extra: Vec<(String, String)>,
}

/// Runs `body` `warmup + iterations` times and returns the duration of every call in nanoseconds,
/// warm-up calls included.
pub fn sample_durations(warmup: usize, iterations: usize, mut body: impl FnMut(usize)) -> Vec<i64> {
    let mut samples = Vec::with_capacity(warmup + iterations);
    for i in 0..warmup + iterations {
        let start = Instant::now();
        body(i);
        samples.push(start.elapsed().as_nanos() as i64);
    }
    samples
}

impl BenchResult {
    pub fn to_json(&self) -> String {
        let measured = &self.samples[self.warmup..];
        let mut sorted = measured.to_vec();
        sorted.sort_unstable();
        let percentile = |p: f64| sorted[((sorted.len() - 1) as f64 * p) as usize];
        let total: i64 = measured.iter().sum();
        let first = &self.samples[..self.samples.len().min(50)];
        let window = (self.samples.len() / 10).max(1);
        let timeline: Vec<String> = (0..self.samples.len() / window)
            .map(|w| (self.samples[w * window..(w + 1) * window].iter().sum::<i64>() / window as i64).to_string())
            .collect();
        let mut fields: Vec<(String, String)> = vec![
            ("target".into(), format!("\"{}\"", platform_name())),
            ("workload".into(), format!("\"{}\"", self.workload)),
            ("unit".into(), format!("\"{}\"", self.unit)),
            ("cpus".into(), available_processors().to_string()),
            ("warmup".into(), self.warmup.to_string()),
            ("iterations".into(), measured.len().to_string()),
            ("setupNs".into(), self.setup_nanos.to_string()),
            ("totalNs".into(), total.to_string()),
            ("meanNs".into(), (total / measured.len() as i64).to_string()),
            ("p50Ns".into(), percentile(0.50).to_string()),
            ("p90Ns".into(), percentile(0.90).to_string()),
            ("p99Ns".into(), percentile(0.99).to_string()),
            ("maxNs".into(), sorted[sorted.len() - 1].to_string()),
            ("first50MeanNs".into(), (first.iter().sum::<i64>() / first.len() as i64).to_string()),
            ("gcCount".into(), "0".into()),
            ("gcMillis".into(), "0".into()),
            ("gcPauseMillis".into(), "0".into()),
            ("checksum".into(), format!("\"{:x}\"", self.checksum as u64)),
            ("timelineNs".into(), format!("[{}]", timeline.join(","))),
        ];
        for (key, value) in &self.extra {
            fields.push((key.clone(), format!("\"{value}\"")));
        }
        let body: Vec<String> = fields.iter().map(|(k, v)| format!("\"{k}\":{v}")).collect();
        format!("RESULT {{{}}}", body.join(","))
    }
}
