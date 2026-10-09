//! Latest-only background work channel; queued obsolete work is replaced, never accumulated.
use std::{
    sync::{
        mpsc::{self, Receiver},
        Arc, Mutex,
    },
    time::Duration,
};
// A single replaceable slot keeps rapid track changes from dropping the newest
// job while an older waveform or network request is still running.
pub(crate) struct LatestJob<T> {
    pub(crate) pending: Arc<Mutex<Option<T>>>,
    wake: mpsc::SyncSender<()>,
}
pub(crate) struct LatestReceiver<T> {
    pub(crate) pending: Arc<Mutex<Option<T>>>,
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
        let _ = self.wake.try_send(());
    }
}
impl<T> LatestReceiver<T> {
    pub(crate) fn receive(&self) -> Option<T> {
        let _ = self.wake.recv_timeout(Duration::from_millis(100));
        self.pending.lock().unwrap().take()
    }
}
