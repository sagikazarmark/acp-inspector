//! Ordered attachment work, independent of how files entered the composer.

use crate::prompt_media::PromptMedia;
use acp_inspector_core::v1;
use std::collections::BTreeMap;

pub const MAX_ATTACHMENTS: usize = 8;
pub const MAX_TOTAL_BYTES: usize = 6 * 1024 * 1024;

#[derive(Clone, PartialEq)]
pub enum MediaState {
    Reading,
    Ready(PromptMedia),
    Failed(String),
}

#[derive(Clone, PartialEq)]
pub struct MediaEntry {
    pub id: u64,
    pub name: String,
    pub state: MediaState,
}

/// IDs never repeat, even after clearing. Late completions cannot resurrect a
/// removed row. Results settle in selection order, so disk speed cannot decide
/// which attachment gets the remaining byte budget.
#[derive(Default)]
pub struct MediaDraft {
    next: u64,
    entries: Vec<MediaEntry>,
    completed: BTreeMap<u64, Result<PromptMedia, String>>,
}

impl MediaDraft {
    pub fn entries(&self) -> &[MediaEntry] {
        &self.entries
    }
    pub fn bytes(&self) -> usize {
        self.entries
            .iter()
            .filter_map(|entry| match &entry.state {
                MediaState::Ready(media) => Some(media.bytes),
                _ => None,
            })
            .sum()
    }
    pub fn reserve(&mut self, name: String) -> Result<u64, String> {
        if self.entries.len() == MAX_ATTACHMENTS {
            return Err(
                "At most 8 attachment rows can be held. Remove or dismiss a row before adding more."
                    .into(),
            );
        }
        let id = self.next;
        self.next += 1;
        self.entries.push(MediaEntry {
            id,
            name,
            state: MediaState::Reading,
        });
        Ok(id)
    }
    pub fn finish(&mut self, id: u64, result: Result<PromptMedia, String>) {
        if !self
            .entries
            .iter()
            .any(|entry| entry.id == id && matches!(entry.state, MediaState::Reading))
        {
            return;
        }
        self.completed.insert(id, result);
        self.settle();
    }
    pub fn remove(&mut self, id: u64) {
        self.entries.retain(|entry| entry.id != id);
        self.completed.remove(&id);
        self.settle();
    }
    pub fn clear(&mut self) {
        self.entries.clear();
        self.completed.clear();
    }
    pub fn sendable(&self) -> bool {
        self.entries
            .iter()
            .all(|entry| matches!(entry.state, MediaState::Ready(_)))
    }
    pub fn content(&self) -> Option<Vec<v1::ContentBlock>> {
        self.sendable().then(|| {
            self.entries
                .iter()
                .filter_map(|entry| match &entry.state {
                    MediaState::Ready(media) => Some(media.content()),
                    _ => None,
                })
                .collect()
        })
    }
    fn settle(&mut self) {
        let mut bytes = 0;
        for entry in &mut self.entries {
            match &entry.state {
                MediaState::Ready(media) => {
                    bytes += media.bytes;
                    continue;
                }
                MediaState::Failed(_) => continue,
                MediaState::Reading => {}
            }
            let Some(result) = self.completed.remove(&entry.id) else {
                break;
            };
            entry.state = match result {
                Ok(media) if bytes + media.bytes <= MAX_TOTAL_BYTES => {
                    bytes += media.bytes;
                    MediaState::Ready(media)
                }
                Ok(_) => MediaState::Failed("This file would exceed the 6 MiB total attachment budget. Dismiss it and select it again after freeing room.".into()),
                Err(error) => MediaState::Failed(error),
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prompt_media::PromptMedia;

    #[test]
    fn images_and_audio_share_order_and_the_same_byte_budget() {
        let mut draft = MediaDraft::default();
        let first = draft.reserve("picture.png".into()).unwrap();
        let second = draft.reserve("sound.wav".into()).unwrap();
        let third = draft.reserve("music.mp3".into()).unwrap();
        draft.finish(
            third,
            Ok(PromptMedia::from_bytes("music.mp3".into(), b"\xff\xfb\x90\x64").unwrap()),
        );
        draft.finish(
            second,
            Ok(PromptMedia::from_bytes("sound.wav".into(), b"RIFF\x04\0\0\0WAVE").unwrap()),
        );
        draft.finish(first, Ok(image("picture.png", 8)));
        let content = draft.content().unwrap();
        assert!(
            matches!(&content[..], [v1::ContentBlock::Image(_),v1::ContentBlock::Audio(wav),v1::ContentBlock::Audio(mp3)] if wav.mime_type=="audio/wav" && mp3.mime_type=="audio/mpeg")
        );
        assert_eq!(draft.bytes(), 24);
        draft.remove(second);
        assert_eq!(draft.bytes(), 12);
    }

    fn image(name: &str, size: usize) -> PromptMedia {
        let mut bytes = b"\x89PNG\r\n\x1a\n".to_vec();
        bytes.resize(size.max(8), 0);
        PromptMedia::from_bytes(name.into(), &bytes).unwrap()
    }

    #[test]
    fn completion_order_does_not_change_selection_order_or_budget_priority() {
        let mut draft = MediaDraft::default();
        let first = draft.reserve("first.png".into()).unwrap();
        let second = draft.reserve("second.png".into()).unwrap();
        draft.finish(second, Ok(image("second.png", 3 * 1024 * 1024)));
        assert!(!draft.sendable());
        draft.finish(first, Ok(image("first.png", 4 * 1024 * 1024)));
        assert!(matches!(draft.entries()[0].state, MediaState::Ready(_)));
        assert!(matches!(draft.entries()[1].state, MediaState::Failed(_)));
        assert!(
            !draft.sendable(),
            "a failed file cannot be silently omitted"
        );
        draft.remove(second);
        assert!(draft.sendable());
        assert_eq!(draft.bytes(), 4 * 1024 * 1024);
    }

    #[test]
    fn removal_and_clear_discard_late_reads_and_ids_never_repeat() {
        let mut draft = MediaDraft::default();
        let removed = draft.reserve("same.png".into()).unwrap();
        draft.remove(removed);
        draft.finish(removed, Ok(image("same.png", 8)));
        assert!(draft.entries().is_empty());
        let old = draft.reserve("same.png".into()).unwrap();
        draft.clear();
        let fresh = draft.reserve("same.png".into()).unwrap();
        assert_ne!(old, fresh);
        draft.finish(old, Ok(image("same.png", 8)));
        assert!(!draft.sendable());
        draft.finish(fresh, Err("unreadable".into()));
        assert!(draft.content().is_none());
        draft.remove(fresh);
        assert_eq!(draft.content(), Some(vec![]));
    }

    #[test]
    fn eight_rows_bound_reads_and_duplicates_keep_their_places() {
        let mut draft = MediaDraft::default();
        for _ in 0..8 {
            let id = draft.reserve("same.png".into()).unwrap();
            draft.finish(id, Ok(image("same.png", 8)));
        }
        assert!(draft.reserve("ninth.png".into()).is_err());
        assert_eq!(draft.content().unwrap().len(), 8);
        assert_eq!(draft.bytes(), 64);
        draft.remove(3);
        let id = draft.reserve("same.png".into()).unwrap();
        assert!(id > 7);
    }

    #[test]
    fn the_exact_total_budget_fits_and_one_more_byte_is_a_visible_failure() {
        let mut draft = MediaDraft::default();
        let first = draft.reserve("five.png".into()).unwrap();
        draft.finish(first, Ok(image("five.png", 5 * 1024 * 1024)));
        let second = draft.reserve("one.png".into()).unwrap();
        draft.finish(second, Ok(image("one.png", 1024 * 1024)));
        assert_eq!(draft.bytes(), MAX_TOTAL_BYTES);
        assert!(draft.sendable());
        let third = draft.reserve("extra.png".into()).unwrap();
        draft.finish(third, Ok(image("extra.png", 8)));
        assert!(matches!(draft.entries()[2].state, MediaState::Failed(_)));
        assert!(draft.content().is_none());
    }
}
