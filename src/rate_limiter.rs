use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Semaphore;

pub struct RateLimiter {
    semaphore: Arc<Semaphore>,
}

impl RateLimiter {
    pub fn new(rate: f64) -> Self {
        if rate <= 0.0 {
            return RateLimiter {
                semaphore: Arc::new(Semaphore::new(usize::MAX)),
            };
        }

        let rate_usize = rate.ceil() as usize;
        let semaphore = Arc::new(Semaphore::new(rate_usize));
        let s = semaphore.clone();

        tokio::spawn(async move {
            let interval_ns = (1_000_000_000.0 / rate) as u64;
            let interval = Duration::from_nanos(interval_ns.max(1_000_000));
            loop {
                tokio::time::sleep(interval).await;
                s.add_permits(1);
            }
        });

        RateLimiter { semaphore }
    }

    pub async fn acquire(&self) {
        self.semaphore
            .acquire()
            .await
            .expect("Rate limiter semaphore closed")
            .forget();
    }
}

impl Clone for RateLimiter {
    fn clone(&self) -> Self {
        RateLimiter {
            semaphore: self.semaphore.clone(),
        }
    }
}
