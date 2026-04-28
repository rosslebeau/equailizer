use crate::error::{Error, Result};
use reqwest::{header::HeaderMap, RequestBuilder, StatusCode};
use std::time::{Duration, Instant};
use tokio::{sync::Mutex, time::sleep};

pub const DEFAULT_REQUESTS_PER_SECOND: u32 = 5;
pub const DEFAULT_MAX_RETRIES: u32 = 5;
pub const DEFAULT_BASE_BACKOFF: Duration = Duration::from_secs(1);
pub const ERROR_BODY_SNIPPET_LIMIT: usize = 500;

pub fn body_snippet(body: &str) -> String {
    if body.len() <= ERROR_BODY_SNIPPET_LIMIT {
        body.to_string()
    } else {
        format!("{}…", &body[..ERROR_BODY_SNIPPET_LIMIT])
    }
}

#[derive(Debug)]
pub struct RawResponse {
    pub status: StatusCode,
    pub body: String,
}

pub struct RateLimitedHttp {
    client: reqwest::Client,
    last_call: Mutex<Option<Instant>>,
    min_interval: Duration,
    max_retries: u32,
    base_backoff: Duration,
}

impl Default for RateLimitedHttp {
    fn default() -> Self {
        Self::new(
            DEFAULT_REQUESTS_PER_SECOND,
            DEFAULT_MAX_RETRIES,
            DEFAULT_BASE_BACKOFF,
        )
    }
}

impl RateLimitedHttp {
    pub fn new(requests_per_second: u32, max_retries: u32, base_backoff: Duration) -> Self {
        let min_interval = if requests_per_second == 0 {
            Duration::ZERO
        } else {
            Duration::from_secs_f64(1.0 / requests_per_second as f64)
        };
        Self {
            client: reqwest::Client::new(),
            last_call: Mutex::new(None),
            min_interval,
            max_retries,
            base_backoff,
        }
    }

    pub fn client(&self) -> &reqwest::Client {
        &self.client
    }

    async fn acquire_slot(&self) {
        let mut last = self.last_call.lock().await;
        if let Some(prev) = *last {
            let elapsed = prev.elapsed();
            if elapsed < self.min_interval {
                sleep(self.min_interval - elapsed).await;
            }
        }
        *last = Some(Instant::now());
    }

    pub async fn send(&self, builder: RequestBuilder) -> Result<RawResponse> {
        let mut attempt: u32 = 0;
        loop {
            let req = builder.try_clone().ok_or_else(|| {
                Error::Api("request body is not clonable for retry".to_string())
            })?;

            self.acquire_slot().await;
            let resp = req.send().await?;
            let status = resp.status();
            let headers = resp.headers().clone();
            let body = resp.text().await?;

            if status == StatusCode::TOO_MANY_REQUESTS && attempt < self.max_retries {
                let delay = retry_after_delay(&headers)
                    .unwrap_or_else(|| backoff_with_jitter(self.base_backoff, attempt));
                tracing::warn!(
                    attempt = attempt + 1,
                    max_retries = self.max_retries,
                    delay_ms = delay.as_millis() as u64,
                    "HTTP 429 received, backing off before retry"
                );
                sleep(delay).await;
                attempt += 1;
                continue;
            }

            return Ok(RawResponse { status, body });
        }
    }
}

fn retry_after_delay(headers: &HeaderMap) -> Option<Duration> {
    headers
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.trim().parse::<u64>().ok())
        .map(Duration::from_secs)
}

fn backoff_with_jitter(base: Duration, attempt: u32) -> Duration {
    let shift = attempt.min(6);
    let exp = base.saturating_mul(1u32 << shift);
    let jitter_max_ms = (exp.as_millis() as u64 / 2).saturating_add(1);
    let jitter_ms = rand::random_range(0..jitter_max_ms);
    exp + Duration::from_millis(jitter_ms)
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::header::{HeaderMap, HeaderValue, RETRY_AFTER};

    #[test]
    fn retry_after_parses_seconds() {
        let mut h = HeaderMap::new();
        h.insert(RETRY_AFTER, HeaderValue::from_static("12"));
        assert_eq!(retry_after_delay(&h), Some(Duration::from_secs(12)));
    }

    #[test]
    fn retry_after_missing_returns_none() {
        assert_eq!(retry_after_delay(&HeaderMap::new()), None);
    }

    #[test]
    fn retry_after_non_numeric_returns_none() {
        let mut h = HeaderMap::new();
        h.insert(RETRY_AFTER, HeaderValue::from_static("Wed, 21 Oct 2026 07:28:00 GMT"));
        assert_eq!(retry_after_delay(&h), None);
    }

    #[test]
    fn backoff_grows_exponentially() {
        let base = Duration::from_secs(1);
        let a0 = backoff_with_jitter(base, 0);
        let a3 = backoff_with_jitter(base, 3);
        assert!(a0 >= Duration::from_secs(1) && a0 < Duration::from_millis(1501));
        assert!(a3 >= Duration::from_secs(8) && a3 < Duration::from_millis(12001));
    }

    #[tokio::test]
    async fn acquire_slot_enforces_min_interval() {
        let http = RateLimitedHttp::new(50, 0, Duration::from_secs(1));
        let start = Instant::now();
        for _ in 0..3 {
            http.acquire_slot().await;
        }
        // 3 calls at 50 req/s should take >= 2 * 20ms.
        assert!(start.elapsed() >= Duration::from_millis(40));
    }
}
