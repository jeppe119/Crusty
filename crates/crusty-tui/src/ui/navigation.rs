//! List navigation and item management methods for MusicPlayerApp.
//!
//! Handles cursor movement across search results, queue, mix, history, and accounts.

use crate::config::clean_title;

use super::app::MusicPlayerApp;

impl MusicPlayerApp {
    pub(super) fn next_search_result(&mut self) {
        if !self.search.results.is_empty() {
            self.ui.selected_result = (self.ui.selected_result + 1) % self.search.results.len();
        }
    }

    pub(super) fn prev_search_result(&mut self) {
        if !self.search.results.is_empty() {
            if self.ui.selected_result == 0 {
                self.ui.selected_result = self.search.results.len() - 1;
            } else {
                self.ui.selected_result -= 1;
            }
        }
    }

    pub(super) fn next_queue_item(&mut self) {
        let queue_len = self.queue.len();
        if queue_len > 0 {
            self.ui.selected_queue_item = (self.ui.selected_queue_item + 1) % queue_len;
            // HOVER DOWNLOAD: Start downloading this track immediately!
            self.trigger_hover_download(self.ui.selected_queue_item);
        }
    }

    pub(super) fn prev_queue_item(&mut self) {
        let queue_len = self.queue.len();
        if queue_len > 0 {
            if self.ui.selected_queue_item == 0 {
                self.ui.selected_queue_item = queue_len - 1;
            } else {
                self.ui.selected_queue_item -= 1;
            }
            // HOVER DOWNLOAD: Start downloading this track immediately!
            self.trigger_hover_download(self.ui.selected_queue_item);
        }
    }

    fn trigger_hover_download(&self, index: usize) {
        let queue_slice = self.queue.get_queue_slice(index + 15, 1);
        self.downloads
            .trigger_hover_download(&queue_slice, self.cookie_config());
    }

    pub(super) fn delete_selected_queue_item(&mut self) {
        if self.ui.queue_expanded && !self.queue.is_empty() {
            if let Some(removed_track) = self.queue.remove_at(self.ui.selected_queue_item) {
                let clean_title = clean_title(&removed_track.title);
                self.status_message = format!("Removed '{}' from queue", clean_title);

                // Adjust selection if needed
                let queue_len = self.queue.len();
                if queue_len == 0 {
                    self.ui.selected_queue_item = 0;
                } else if self.ui.selected_queue_item >= queue_len {
                    self.ui.selected_queue_item = queue_len - 1;
                }
            }
        } else if !self.ui.queue_expanded {
            self.status_message = "Press 't' to expand queue first".to_string();
        }
    }

    pub(super) fn next_mix_item(&mut self) {
        if !self.playlist.my_mix_playlists.is_empty() {
            self.ui.selected_mix_item =
                (self.ui.selected_mix_item + 1) % self.playlist.my_mix_playlists.len();
        }
    }

    pub(super) fn prev_mix_item(&mut self) {
        if !self.playlist.my_mix_playlists.is_empty() {
            if self.ui.selected_mix_item == 0 {
                self.ui.selected_mix_item = self.playlist.my_mix_playlists.len() - 1;
            } else {
                self.ui.selected_mix_item -= 1;
            }
        }
    }

    pub(super) fn next_history_item(&mut self) {
        let history_len = self.queue.get_history().len();
        if history_len > 0 {
            self.ui.selected_history_item = (self.ui.selected_history_item + 1) % history_len;
        }
    }

    pub(super) fn prev_history_item(&mut self) {
        let history_len = self.queue.get_history().len();
        if history_len > 0 {
            if self.ui.selected_history_item == 0 {
                self.ui.selected_history_item = history_len - 1;
            } else {
                self.ui.selected_history_item -= 1;
            }
        }
    }

    pub(super) fn clear_history(&mut self) {
        let count = self.queue.get_history().len();
        self.queue.clear_history();
        self.ui.selected_history_item = 0;
        self.status_message = format!("Cleared {} tracks from history", count);

        // Save to disk
        if let Err(e) = self.save_history() {
            self.status_message = format!("History cleared but failed to save: {}", e);
        }
    }

    pub(super) fn delete_selected_history_item(&mut self) {
        let history_len = self.queue.get_history().len();
        if history_len == 0 {
            return;
        }

        if let Some(removed) = self.queue.remove_history_at(self.ui.selected_history_item) {
            let title = clean_title(&removed.title);
            self.status_message = format!("Removed '{}' from history", title);

            // Adjust selection
            let new_len = self.queue.get_history().len();
            if new_len == 0 {
                self.ui.selected_history_item = 0;
            } else if self.ui.selected_history_item >= new_len {
                self.ui.selected_history_item = new_len - 1;
            }

            // Save to disk
            if let Err(e) = self.save_history() {
                self.status_message = format!("Removed from history but save failed: {}", e);
            }
        }
    }

    pub(super) fn next_account(&mut self) {
        if !self.available_accounts.is_empty() {
            self.ui.selected_account_idx =
                (self.ui.selected_account_idx + 1) % self.available_accounts.len();
        }
    }

    pub(super) fn prev_account(&mut self) {
        if !self.available_accounts.is_empty() {
            if self.ui.selected_account_idx == 0 {
                self.ui.selected_account_idx = self.available_accounts.len() - 1;
            } else {
                self.ui.selected_account_idx -= 1;
            }
        }
    }
}
