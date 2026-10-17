use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Mutex;
use tokio::time::{sleep, Duration};

/// Token bucket bandwidth rate limiter for throttling downloads
#[derive(Clone)]
pub struct RateLimiter {
    bytes_per_sec: Arc<AtomicU64>,
    state: Arc<Mutex<TokenBucketState>>,
}

struct TokenBucketState {
    tokens: f64,
    last_update: Instant,
}

impl RateLimiter {
    pub fn new(bytes_per_sec: Option<u64>) -> Self {
        let bps = bytes_per_sec.unwrap_or(0);
        Self {
            bytes_per_sec: Arc::new(AtomicU64::new(bps)),
            state: Arc::new(Mutex::new(TokenBucketState {
                tokens: bps as f64,
                last_update: Instant::now(),
            })),
        }
    }

    #[allow(dead_code)]
    pub fn set_limit(&self, bytes_per_sec: Option<u64>) {
        self.bytes_per_sec
            .store(bytes_per_sec.unwrap_or(0), Ordering::Relaxed);
    }

    pub async fn acquire(&self, count: usize) {
        let limit = self.bytes_per_sec.load(Ordering::Relaxed);
        if limit == 0 {
            return;
        }

        let max_tokens = limit as f64;
        let mut state = self.state.lock().await;
        let now = Instant::now();
        let elapsed = now.duration_since(state.last_update).as_secs_f64();
        state.last_update = now;

        state.tokens = (state.tokens + elapsed * max_tokens).min(max_tokens);

        let needed = count as f64;
        if state.tokens < needed {
            let deficit = needed - state.tokens;
            let wait_secs = deficit / max_tokens;
            state.tokens = 0.0;
            drop(state);
            sleep(Duration::from_secs_f64(wait_secs)).await;
        } else {
            state.tokens -= needed;
        }
    }
}
