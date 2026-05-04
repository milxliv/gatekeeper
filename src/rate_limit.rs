use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// In-memory per-key sliding-window attempt tracker. Used for /login brute-force
/// defense. Keyed by source IP string. Bounded memory: stale entries expire
/// from the inner Vec via `retain_recent` and empty keys are cleaned up by the
/// background sweep.
pub struct AuthAttemptTracker {
    inner: Mutex<HashMap<String, Vec<Instant>>>,
}

impl AuthAttemptTracker {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
        }
    }

    /// True if the caller has fewer than `max` recorded attempts within
    /// `window`. Does NOT record an attempt — call `record_fail` separately
    /// after a failed authentication so successful logins don't count
    /// toward the lockout budget.
    pub fn allowed(&self, key: &str, max: usize, window: Duration) -> bool {
        let now = Instant::now();
        let mut map = self.inner.lock().expect("rate limit mutex poisoned");
        let entry = map.entry(key.to_string()).or_default();
        entry.retain(|t| now.duration_since(*t) < window);
        entry.len() < max
    }

    /// Record a failed attempt against `key`. Call this only on auth
    /// failures so legitimate logins are not rate-limited.
    pub fn record_fail(&self, key: &str) {
        let mut map = self.inner.lock().expect("rate limit mutex poisoned");
        map.entry(key.to_string()).or_default().push(Instant::now());
    }

    /// Drop entries older than `window`. Call periodically from the background
    /// cleanup task to bound memory growth in long-running processes.
    pub fn sweep(&self, window: Duration) {
        let now = Instant::now();
        let mut map = self.inner.lock().expect("rate limit mutex poisoned");
        for entries in map.values_mut() {
            entries.retain(|t| now.duration_since(*t) < window);
        }
        map.retain(|_, v| !v.is_empty());
    }
}

impl Default for AuthAttemptTracker {
    fn default() -> Self {
        Self::new()
    }
}
