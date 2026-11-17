//! Latest-only background work channel; queued obsolete work is replaced, never accumulated.
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc::{self, Receiver},
    Arc, Mutex,
};
// A single replaceable slot keeps rapid track changes from dropping the newest
// job while an older waveform or network request is still running.
pub(crate) struct LatestJob<T> {
    pending: Arc<Mutex<Option<T>>>,
    wake: mpsc::SyncSender<()>,
}
pub(crate) struct LatestReceiver<T> {
    pending: Arc<Mutex<Option<T>>>,
    wake: Receiver<()>,
}
pub(crate) fn latest_channel<T>() -> (LatestJob<T>, LatestReceiver<T>) {
    let (wake, receiver) = mpsc::sync_channel(1);
    let pending = Arc::new(Mutex::new(None));
    (
        LatestJob {
            pending: pending.clone(),
            wake,
        },
        LatestReceiver {
            pending,
            wake: receiver,
        },
    )
}
impl<T> LatestJob<T> {
    pub(crate) fn submit(&self, value: T) {
        *self.pending.lock().unwrap() = Some(value);
        self.wake();
    }
    pub(crate) fn wake(&self) {
        let _ = self.wake.try_send(());
    }
}
impl<T> LatestReceiver<T> {
    pub(crate) fn try_receive(&self) -> Option<T> {
        self.pending.lock().unwrap().take()
    }
    pub(crate) fn receive(&self, stop: &AtomicBool) -> Option<T> {
        while !stop.load(Ordering::Relaxed) {
            self.wake.recv().ok()?;
            if stop.load(Ordering::Relaxed) {
                return None;
            }
            if let Some(value) = self.pending.lock().unwrap().take() {
                return Some(value);
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{thread, time::Duration};

    #[test]
    fn idle_receiver_waits_until_cancelled_and_woken() {
        let (jobs, receiver) = latest_channel::<u8>();
        let stop = Arc::new(AtomicBool::new(false));
        let cancelled = stop.clone();
        let (done, finished) = mpsc::channel();
        let worker = thread::spawn(move || {
            done.send(receiver.receive(&cancelled)).unwrap();
        });
        assert!(matches!(
            finished.recv_timeout(Duration::from_millis(150)),
            Err(mpsc::RecvTimeoutError::Timeout)
        ));
        stop.store(true, Ordering::Relaxed);
        jobs.wake();
        assert_eq!(finished.recv_timeout(Duration::from_secs(2)).unwrap(), None);
        worker.join().unwrap();
    }

    #[test]
    fn empty_notifications_do_not_end_receiver_and_disconnect_does() {
        let (jobs, receiver) = latest_channel();
        jobs.wake();
        let worker = thread::spawn(move || {
            let stop = AtomicBool::new(false);
            assert_eq!(receiver.receive(&stop), Some(42));
            assert_eq!(receiver.receive(&stop), None);
        });
        jobs.submit(42);
        drop(jobs);
        worker.join().unwrap();
    }
}
