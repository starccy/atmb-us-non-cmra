use std::future::Future;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use backoff::{future::retry, ExponentialBackoff};
use log::warn;

/// Retry a function for a number of times
pub async fn retry_wrapper<I, E, F, Fut>(retry_times: usize, f: F) -> Result<I, E>
where
    F: Fn() -> Fut,
    Fut: Future<Output = Result<I, E>>,
{
    let cur_times = AtomicUsize::new(0);
    retry(backoff_config(), || async {
        let prev_times = cur_times.fetch_add(1, Ordering::AcqRel);
        let times = prev_times + 1;
        if times > 1 {
            warn!("retrying for the {} time", times);
        }
        f().await
            .map_err(|err| map_to_backoff_err(err, times, retry_times))
    })
        .await
}

fn map_to_backoff_err<E>(err: E, cur_times: usize, max_times: usize) -> backoff::Error<E> {
    if cur_times > max_times {
        backoff::Error::permanent(err)
    } else {
        backoff::Error::transient(err)
    }
}

#[inline]
fn backoff_config() -> ExponentialBackoff {
    ExponentialBackoff {
        initial_interval: Duration::from_millis(1000),
        max_interval: Duration::from_millis(5000),
        ..Default::default()
    }
}