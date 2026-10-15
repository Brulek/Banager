//! One run of a piece of work at a time, shared by every call that arrives
//! while it is under way (`ipc::scan_unknown`).
//!
//! The Other Programs page asks for a scan once per snapshot generation
//! while it is open, and whenever Scan Again is pressed
//! (`src/pages/UnknownPage.tsx`), and a scan takes up to ten seconds
//! (`ScanBudget::default()`). A request that arrived while one was running
//! used to wait its turn and then scan again, so a page that asked over
//! and over queued scans without end. Now it is handed the result of the
//! scan under way, which is what it would have had moments later, and the
//! page shows nothing different (the author's decision S10, 2026-10-06).
//! Except when the snapshot has moved on since that scan began: a scan is
//! judged against the snapshot as it was when it started
//! (`Session::scan_unknown`), and the page asks again when a refresh
//! commits precisely so that a list judged against the snapshot before it
//! does not stand. So each run has a key -- for the scan, the snapshot
//! generation -- and a call with another key waits for the run under way
//! and then has its own.

use std::sync::{Mutex, PoisonError};
use tokio::sync::watch;

/// What the calls waiting for a run are handed: nothing yet, then the
/// run's result, or the runtime's sentence when the work panicked.
type Answer<T> = Option<Result<T, String>>;

/// One run at a time, shared: a call that arrives while a run for the same
/// key is under way is handed that run's result instead of starting
/// another, and the first call after it has finished starts a new one.
pub(crate) struct SharedRun<K, T> {
    /// The run under way, if any, and what it was started for: whoever
    /// calls now subscribes to its answer. Taken out, and so empty, as the
    /// run finishes.
    running: Mutex<Option<(K, watch::Sender<Answer<T>>)>>,
}

impl<K, T> SharedRun<K, T>
where
    K: Clone + PartialEq + Send + Sync + 'static,
    T: Clone + Send + Sync + 'static,
{
    pub(crate) const fn new() -> Self {
        SharedRun {
            running: Mutex::new(None),
        }
    }

    /// `work` on the blocking pool, unless a run for the same `key` is
    /// already under way: then that run's result. A run for another key
    /// is waited for first -- one run at a time -- and then this call
    /// starts its own, or shares the one another call waiting with the
    /// same key started. The run belongs to no caller -- a task of its own
    /// -- so the calls waiting for it are answered even when the call that
    /// started it is no longer waiting; and no waiting call holds a thread,
    /// only a subscription.
    pub(crate) async fn run(
        &'static self,
        key: K,
        work: impl FnOnce() -> T + Send + 'static,
    ) -> Result<T, String> {
        // Moved into the run this call starts, if it starts one; a call
        // that starts one is answered by it, so this is taken at most once.
        let mut work = Some(work);
        loop {
            let (mut answer, ours) = {
                let mut running = self.running.lock().unwrap_or_else(PoisonError::into_inner);
                match running.as_ref() {
                    Some((under_way, sender)) => (sender.subscribe(), *under_way == key),
                    None => {
                        let work = work.take().expect("a call starts at most one run");
                        let (sender, answer) = watch::channel(None);
                        *running = Some((key.clone(), sender));
                        self.start(work);
                        (answer, true)
                    }
                }
            };
            let answered = answer
                .wait_for(Option::is_some)
                .await
                .map_err(|e| e.to_string())?;
            if ours {
                return answered.clone().expect("waited until the run had answered");
            }
            // A run for another key has finished: round again, to start a
            // run for this one or share the one another call started.
        }
    }

    /// The run itself, a task of its own on the async runtime: `work` on
    /// the blocking pool, then its answer to every call subscribed.
    fn start(&'static self, work: impl FnOnce() -> T + Send + 'static) {
        tauri::async_runtime::spawn(async move {
            // Only a panic inside `work` makes this an error: the runtime's
            // sentence, as before.
            let result = tauri::async_runtime::spawn_blocking(work)
                .await
                .map_err(|e| e.to_string());
            // Taken out before the answer goes, so a call from now on
            // starts a run of its own; every call that subscribed before is
            // answered.
            let finished = self
                .running
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .take();
            if let Some((_, sender)) = finished {
                sender.send_replace(Some(result));
            }
        });
    }

    /// How many calls wait for the run under way, its own caller among
    /// them.
    #[cfg(test)]
    pub(crate) fn waiting(&self) -> usize {
        self.running
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
            .map_or(0, |(_, sender)| sender.receiver_count())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{mpsc, Arc, Mutex};
    use std::time::Duration;

    /// Until `waiting` says `n` calls wait for the run under way: they have
    /// all arrived while it runs.
    async fn until_waiting<K, T>(shared: &SharedRun<K, T>, n: usize)
    where
        K: Clone + PartialEq + Send + Sync + 'static,
        T: Clone + Send + Sync + 'static,
    {
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while shared.waiting() < n {
            assert!(
                std::time::Instant::now() < deadline,
                "{n} calls never waited for one run (waiting: {})",
                shared.waiting()
            );
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn test_calls_that_arrive_during_a_run_get_that_runs_result() {
        static SHARED: SharedRun<u64, usize> = SharedRun::new();
        let runs = Arc::new(AtomicUsize::new(0));
        // The run holds until the test lets it go, so every call below
        // arrives while it is under way.
        let (go, wait) = mpsc::channel::<()>();
        let wait = Arc::new(Mutex::new(wait));
        let calls: Vec<_> = (0..6)
            .map(|_| {
                let (runs, wait) = (runs.clone(), wait.clone());
                tokio::spawn(async move {
                    SHARED
                        .run(7, move || {
                            wait.lock().unwrap().recv().unwrap();
                            runs.fetch_add(1, Ordering::SeqCst) + 1
                        })
                        .await
                })
            })
            .collect();
        until_waiting(&SHARED, 6).await;
        go.send(()).unwrap();

        let mut answers = Vec::new();
        for call in calls {
            answers.push(call.await.unwrap().expect("the run's result"));
        }
        assert_eq!(
            answers,
            vec![1; 6],
            "every call is handed the one run's result"
        );
        assert_eq!(runs.load(Ordering::SeqCst), 1, "the work ran once");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn test_a_call_after_a_run_has_finished_starts_a_new_one() {
        static SHARED: SharedRun<u64, usize> = SharedRun::new();
        let runs = Arc::new(AtomicUsize::new(0));
        for expected in 1..=3 {
            let runs = runs.clone();
            let answer = SHARED
                .run(7, move || runs.fetch_add(1, Ordering::SeqCst) + 1)
                .await
                .expect("the run's result");
            assert_eq!(answer, expected, "each call after the last run ran again");
        }
        assert_eq!(SHARED.waiting(), 0, "no run is under way");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn test_a_run_that_panics_fails_every_call_waiting_and_the_next_call_runs_again() {
        static SHARED: SharedRun<u64, usize> = SharedRun::new();
        let (go, wait) = mpsc::channel::<()>();
        let wait = Arc::new(Mutex::new(wait));
        let calls: Vec<_> = (0..3)
            .map(|_| {
                let wait = wait.clone();
                tokio::spawn(async move {
                    SHARED
                        .run(7, move || -> usize {
                            wait.lock().unwrap().recv().unwrap();
                            panic!("the work failed");
                        })
                        .await
                })
            })
            .collect();
        until_waiting(&SHARED, 3).await;
        go.send(()).unwrap();
        for call in calls {
            let err = call.await.unwrap().expect_err("the run panicked");
            assert!(err.contains("panic"), "{err}");
        }

        assert_eq!(SHARED.run(7, || 7).await, Ok(7), "a new call runs again");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn test_a_call_for_another_key_waits_for_the_run_under_way_then_runs_its_own() {
        // The scan's key is the snapshot generation it is judged against: a
        // page that asks again because a refresh committed must not be
        // handed a scan judged against the snapshot before it.
        static SHARED: SharedRun<u64, (u64, usize)> = SharedRun::new();
        let runs = Arc::new(AtomicUsize::new(0));
        let (go, wait) = mpsc::channel::<()>();
        let wait = Arc::new(Mutex::new(wait));
        let call = |key: u64| {
            let (runs, wait) = (runs.clone(), wait.clone());
            tokio::spawn(async move {
                SHARED
                    .run(key, move || {
                        wait.lock().unwrap().recv().unwrap();
                        (key, runs.fetch_add(1, Ordering::SeqCst) + 1)
                    })
                    .await
            })
        };
        let first = call(1);
        until_waiting(&SHARED, 1).await;
        // Two calls for the next generation arrive during the first run.
        let later = [call(2), call(2)];
        until_waiting(&SHARED, 3).await;
        go.send(()).unwrap();
        assert_eq!(first.await.unwrap(), Ok((1, 1)));
        // They then share one run of their own.
        until_waiting(&SHARED, 2).await;
        go.send(()).unwrap();
        for call in later {
            assert_eq!(
                call.await.unwrap(),
                Ok((2, 2)),
                "a run judged for its own key"
            );
        }
        assert_eq!(runs.load(Ordering::SeqCst), 2);
    }
}
