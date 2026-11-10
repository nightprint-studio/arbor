//! The loop that fires what is due: one thread per product backend, looking at the list every [`TICK`].
//!
//! It only runs while the backend does — there is no daemon. The first look is immediate, which is where runs
//! missed while the app was closed are dropped or caught up (the job's own policy).

use std::sync::Once;
use std::time::Duration;

/// How often the loop looks at the list. A run starts at most this late.
pub const TICK: Duration = Duration::from_secs(20);

/// Start the loop once per `START`, calling `tick` now and then every [`TICK`]. A second call does nothing.
/// Returns the reason when the thread could not be created.
pub fn start(once: &'static Once, name: &str, mut tick: impl FnMut() + Send + 'static) -> Result<(), String> {
    let mut failure = None;
    once.call_once(|| {
        let spawned = std::thread::Builder::new().name(name.to_string()).spawn(move || loop {
            tick();
            std::thread::sleep(TICK);
        });
        if let Err(e) = spawned {
            failure = Some(e.to_string());
        }
    });
    failure.map_or(Ok(()), Err)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    #[test]
    fn the_loop_looks_at_once_and_starts_only_once() {
        static START: Once = Once::new();
        let looks = Arc::new(AtomicU32::new(0));
        let seen = Arc::clone(&looks);
        start(&START, "test-loop", move || {
            seen.fetch_add(1, Ordering::SeqCst);
        })
        .unwrap();
        // The second call is a no-op even with a different tick.
        start(&START, "test-loop-2", || panic!("never started")).unwrap();
        for _ in 0..100 {
            if looks.load(Ordering::SeqCst) > 0 {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(looks.load(Ordering::SeqCst), 1, "the first look is immediate and the next is a tick away");
    }
}
