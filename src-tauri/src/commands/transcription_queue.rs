use std::{
    collections::{HashMap, hash_map::Entry},
    future::Future,
    sync::{Arc, Mutex},
};

use tokio_util::sync::CancellationToken;

use crate::error::{Error, Result};

#[derive(Default)]
pub(super) struct TranscriptionQueue {
    slot: Arc<tokio::sync::Mutex<()>>,
    jobs: Arc<Mutex<HashMap<String, CancellationToken>>>,
}

struct Registration {
    path: String,
    token: CancellationToken,
    jobs: Arc<Mutex<HashMap<String, CancellationToken>>>,
}

impl Drop for Registration {
    fn drop(&mut self) {
        self.jobs
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(&self.path);
    }
}

impl TranscriptionQueue {
    fn register(&self, path: String) -> Result<Registration> {
        let token = CancellationToken::new();
        let mut jobs = self
            .jobs
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        match jobs.entry(path.clone()) {
            Entry::Vacant(entry) => {
                entry.insert(token.clone());
            }
            Entry::Occupied(_) => {
                return Err(Error::Transcribe(format!(
                    "transcription already queued: {path}"
                )));
            }
        }
        drop(jobs);
        Ok(Registration {
            path,
            token,
            jobs: self.jobs.clone(),
        })
    }

    pub(super) async fn run<T, F, Fut>(&self, path: String, work: F) -> Result<T>
    where
        T: Send + 'static,
        F: FnOnce(CancellationToken) -> Fut + Send + 'static,
        Fut: Future<Output = Result<T>> + Send + 'static,
    {
        let registration = self.register(path)?;
        let token = registration.token.clone();
        let slot = tokio::select! {
            biased;
            () = token.cancelled() => return Err(Error::Cancelled),
            guard = self.slot.clone().lock_owned() => guard,
        };
        if token.is_cancelled() {
            return Err(Error::Cancelled);
        }
        tokio::spawn(async move {
            let _registration = registration;
            let _slot = slot;
            work(token).await
        })
        .await
        .map_err(|error| Error::Transcribe(format!("task: {error}")))?
    }

    pub(super) fn cancel(&self, path: &str) -> bool {
        let jobs = self
            .jobs
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(token) = jobs.get(path) else {
            return false;
        };
        token.cancel();
        drop(jobs);
        true
    }

    pub(super) fn cancel_all(&self) -> usize {
        let jobs = self
            .jobs
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        for token in jobs.values() {
            token.cancel();
        }
        jobs.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::poll;
    use tokio::sync::oneshot;

    async fn receive<T>(receiver: oneshot::Receiver<T>) -> T {
        tokio::time::timeout(std::time::Duration::from_secs(2), receiver)
            .await
            .unwrap()
            .unwrap()
    }

    #[tokio::test]
    async fn appending_jobs_keeps_active_work_and_serialises_the_queue() {
        let queue = TranscriptionQueue::default();
        let (started, active) = oneshot::channel();
        let (finish, finished) = oneshot::channel();
        let mut first = Box::pin(queue.run("first".into(), |cancel| async move {
            started.send(()).unwrap();
            receive(finished).await;
            assert!(!cancel.is_cancelled());
            Ok(1)
        }));
        assert!(poll!(first.as_mut()).is_pending());
        receive(active).await;
        let order = Arc::new(Mutex::new(Vec::new()));
        let second_order = order.clone();
        let third_order = order.clone();
        let (second_started, mut second_active) = oneshot::channel();
        let mut second = Box::pin(queue.run("second".into(), |_| async move {
            second_order.lock().unwrap().push(2);
            second_started.send(()).unwrap();
            Ok(2)
        }));
        let mut third = Box::pin(queue.run("third".into(), |_| async move {
            third_order.lock().unwrap().push(3);
            Ok(3)
        }));
        assert!(poll!(second.as_mut()).is_pending());
        assert!(poll!(third.as_mut()).is_pending());
        tokio::task::yield_now().await;
        assert!(second_active.try_recv().is_err());
        finish.send(()).unwrap();
        assert_eq!(first.await.unwrap(), 1);
        assert_eq!(second.await.unwrap(), 2);
        assert_eq!(third.await.unwrap(), 3);
        assert_eq!(*order.lock().unwrap(), vec![2, 3]);
        assert!(queue.jobs.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn cancelling_queued_work_skips_it_without_waiting_for_active_work() {
        let queue = TranscriptionQueue::default();
        let slot = queue.slot.lock().await;
        let mut cancelled = Box::pin(queue.run("cancelled".into(), |_| async {
            panic!("cancelled job must never start");
            #[allow(unreachable_code)]
            Ok(())
        }));
        let mut next = Box::pin(queue.run("next".into(), |_| async { Ok(42) }));
        assert!(poll!(cancelled.as_mut()).is_pending());
        assert!(poll!(next.as_mut()).is_pending());
        assert!(queue.cancel("cancelled"));
        assert!(matches!(cancelled.await, Err(Error::Cancelled)));
        assert!(!queue.cancel("cancelled"));
        assert!(!queue.jobs.lock().unwrap()["next"].is_cancelled());
        drop(slot);
        assert_eq!(next.await.unwrap(), 42);
    }

    #[tokio::test]
    async fn active_cancellation_holds_slot_until_worker_finishes() {
        let queue = TranscriptionQueue::default();
        let (started, active) = oneshot::channel();
        let (cancel_observed, cancelled) = oneshot::channel();
        let (finish, finished) = oneshot::channel();
        let mut first = Box::pin(queue.run("first".into(), |token| async move {
            started.send(()).unwrap();
            token.cancelled().await;
            cancel_observed.send(()).unwrap();
            receive(finished).await;
            Err::<(), _>(Error::Cancelled)
        }));
        assert!(poll!(first.as_mut()).is_pending());
        receive(active).await;
        let (next_started, mut next_active) = oneshot::channel();
        let mut next = Box::pin(queue.run("next".into(), |token| async move {
            assert!(!token.is_cancelled());
            next_started.send(()).unwrap();
            Ok(())
        }));
        assert!(poll!(next.as_mut()).is_pending());
        assert!(queue.cancel("first"));
        receive(cancelled).await;
        assert!(poll!(first.as_mut()).is_pending());
        assert!(next_active.try_recv().is_err());
        finish.send(()).unwrap();
        assert!(matches!(first.await, Err(Error::Cancelled)));
        next.await.unwrap();
    }

    #[tokio::test]
    async fn dropping_the_caller_keeps_the_worker_registered_and_bounded() {
        let queue = TranscriptionQueue::default();
        let (started, active) = oneshot::channel();
        let (finish, finished) = oneshot::channel();
        let mut first = Box::pin(queue.run("first".into(), |_| async move {
            started.send(()).unwrap();
            receive(finished).await;
            Ok(())
        }));
        assert!(poll!(first.as_mut()).is_pending());
        receive(active).await;
        drop(first);
        assert!(queue.jobs.lock().unwrap().contains_key("first"));
        assert!(queue.slot.try_lock().is_err());
        finish.send(()).unwrap();
        queue
            .run("next".into(), |_| async { Ok(()) })
            .await
            .unwrap();
        assert!(queue.jobs.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn duplicate_paths_cannot_replace_the_active_cancellation_token() {
        let queue = TranscriptionQueue::default();
        let original = queue.register("same".into()).unwrap();
        assert!(
            queue
                .run("same".into(), |_| async { Ok(()) })
                .await
                .is_err()
        );
        assert!(queue.cancel("same"));
        assert!(original.token.is_cancelled());
        drop(original);
        queue
            .run("same".into(), |_| async { Ok(()) })
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn failures_and_panics_release_the_slot_and_registration() {
        let queue = TranscriptionQueue::default();
        assert!(
            queue
                .run("failure".into(), |_| async {
                    Err::<(), _>(Error::Transcribe("failed".into()))
                })
                .await
                .is_err()
        );
        assert!(
            queue
                .run("panic".into(), |_| async {
                    panic!("worker panic");
                    #[allow(unreachable_code)]
                    Ok(())
                })
                .await
                .is_err()
        );
        queue
            .run("next".into(), |_| async { Ok(()) })
            .await
            .unwrap();
        assert!(queue.jobs.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn cancel_all_affects_only_current_registrations() {
        let queue = TranscriptionQueue::default();
        let first = queue.register("first".into()).unwrap();
        let second = queue.register("second".into()).unwrap();
        assert_eq!(queue.cancel_all(), 2);
        assert!(first.token.is_cancelled());
        assert!(second.token.is_cancelled());
        drop((first, second));
        queue
            .run("next".into(), |token| async move {
                assert!(!token.is_cancelled());
                Ok(())
            })
            .await
            .unwrap();
    }
}
