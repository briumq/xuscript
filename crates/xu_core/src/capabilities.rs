//! System capability traits for dependency injection.

use std::sync::OnceLock;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

pub trait Clock {
    fn unix_secs(&self) -> i64;
    fn unix_millis(&self) -> i64;
    fn mono_micros(&self) -> i64;
    fn mono_nanos(&self) -> i64;
}

pub struct SystemClock;

impl Clock for SystemClock {
    fn unix_secs(&self) -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0)
    }

    fn unix_millis(&self) -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0)
    }

    fn mono_micros(&self) -> i64 {
        static START: OnceLock<Instant> = OnceLock::new();
        let start = START.get_or_init(Instant::now);
        start.elapsed().as_micros() as i64
    }

    fn mono_nanos(&self) -> i64 {
        static START: OnceLock<Instant> = OnceLock::new();
        let start = START.get_or_init(Instant::now);
        start.elapsed().as_nanos() as i64
    }
}

pub trait FileSystem {
    fn metadata(&self, path: &str) -> Result<(), String>;
    fn stat(&self, path: &str) -> Result<FileStat, String>;
    fn canonicalize(&self, path: &str) -> Result<String, String>;
    fn read_to_string(&self, path: &str) -> Result<String, String>;
}

pub struct StdFileSystem;

impl FileSystem for StdFileSystem {
    fn metadata(&self, path: &str) -> Result<(), String> {
        std::fs::metadata(path)
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    fn stat(&self, path: &str) -> Result<FileStat, String> {
        let meta = std::fs::metadata(path).map_err(|e| e.to_string())?;
        let modified_nanos = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_nanos());
        Ok(FileStat {
            len: meta.len(),
            modified_nanos,
        })
    }

    fn canonicalize(&self, path: &str) -> Result<String, String> {
        let canonical = std::fs::canonicalize(path).map_err(|e| e.to_string())?;
        Ok(canonical.to_string_lossy().to_string())
    }

    fn read_to_string(&self, path: &str) -> Result<String, String> {
        std::fs::read_to_string(path).map_err(|e| e.to_string())
    }
}

pub trait RngAlgorithm {
    fn next_u64(&self, state: &mut u64) -> u64;
}

pub struct Lcg64;

impl RngAlgorithm for Lcg64 {
    fn next_u64(&self, state: &mut u64) -> u64 {
        *state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
        *state
    }
}

pub struct Capabilities {
    pub clock: Box<dyn Clock>,
    pub fs: Box<dyn FileSystem>,
    pub rng: Box<dyn RngAlgorithm>,
    pub allowed_roots: Vec<String>,
}

impl Default for Capabilities {
    fn default() -> Self {
        Self {
            clock: Box::new(SystemClock),
            fs: Box::new(StdFileSystem),
            rng: Box::new(Lcg64),
            allowed_roots: Vec::new(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FileStat {
    pub len: u64,
    pub modified_nanos: Option<u128>,
}
