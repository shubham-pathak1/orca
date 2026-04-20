use crate::AppState;
use slint::ComponentHandle;
use std::rc::Rc;

use slint::{ModelRc, VecModel};

use crate::{AppController, MainWindow, QueueRow};

impl AppController {
    pub(crate) fn refresh_queue_ui(&mut self, window: &MainWindow) {
        self.queue.retain(|idx| self.songs.get(*idx).is_some());
        let rows = self
            .queue
            .iter()
            .filter_map(|idx| self.songs.get(*idx))
            .map(|entry| QueueRow {
                title: entry.song.title.clone().into(),
                artist: entry.song.artist.clone().into(),
            })
            .collect::<Vec<_>>();
        let model: ModelRc<QueueRow> = Rc::new(VecModel::from(rows)).into();
        window.global::<AppState>().set_queue_items(model);
    }

    pub(crate) fn enqueue_selected(&mut self, window: &MainWindow) {
        let Some(song_idx) = self.selected_song_index() else {
            self.set_status("Select a song to add to queue", window);
            return;
        };
        self.queue.push_back(song_idx);
        self.refresh_queue_ui(window);
        if let Some(entry) = self.songs.get(song_idx) {
            self.set_status(
                format!("Queued: {} - {}", entry.song.artist, entry.song.title),
                window,
            );
        }
    }

    pub(crate) fn enqueue_song_by_row(&mut self, row_index: usize, window: &MainWindow) {
        let Some(song_idx) = self.displayed_indices.get(row_index).copied() else {
            return;
        };
        self.queue.push_back(song_idx);
        self.refresh_queue_ui(window);
        if let Some(entry) = self.songs.get(song_idx) {
            self.set_status(
                format!("Queued: {} - {}", entry.song.artist, entry.song.title),
                window,
            );
        }
    }

    pub(crate) fn clear_queue(&mut self, window: &MainWindow) {
        self.queue.clear();
        self.refresh_queue_ui(window);
        self.set_status("Queue cleared", window);
    }

    pub(crate) fn pop_queue_next(&mut self, window: &MainWindow) -> Option<usize> {
        let next = self.queue.pop_front();
        if next.is_some() {
            self.refresh_queue_ui(window);
        }
        next
    }
}
