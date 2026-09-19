//! Ordered attachment work, independent of how files entered the composer.

use crate::prompt_image::PromptImage;
use acp_inspector_core::v1;
use std::collections::BTreeMap;

pub const MAX_IMAGES: usize = 8;
pub const MAX_TOTAL_BYTES: usize = 6 * 1024 * 1024;

#[derive(Clone, PartialEq)]
pub enum ImageState {
    Reading,
    Ready(PromptImage),
    Failed(String),
}

#[derive(Clone, PartialEq)]
pub struct ImageEntry {
    pub id: u64,
    pub name: String,
    pub state: ImageState,
}

/// IDs never repeat, even after clearing. Late completions cannot resurrect a
/// removed row. Results settle in selection order, so disk speed cannot decide
/// which image gets the remaining byte budget.
#[derive(Default)]
pub struct ImageDraft {
    next: u64,
    entries: Vec<ImageEntry>,
    completed: BTreeMap<u64, Result<PromptImage, String>>,
}

impl ImageDraft {
    pub fn entries(&self) -> &[ImageEntry] {
        &self.entries
    }
    pub fn bytes(&self) -> usize {
        self.entries
            .iter()
            .filter_map(|entry| match &entry.state {
                ImageState::Ready(image) => Some(image.bytes),
                _ => None,
            })
            .sum()
    }
    pub fn reserve(&mut self, name: String) -> Result<u64, String> {
        if self.entries.len() == MAX_IMAGES {
            return Err(
                "At most 8 image rows can be held. Remove or dismiss a row before adding more."
                    .into(),
            );
        }
        let id = self.next;
        self.next += 1;
        self.entries.push(ImageEntry {
            id,
            name,
            state: ImageState::Reading,
        });
        Ok(id)
    }
    pub fn finish(&mut self, id: u64, result: Result<PromptImage, String>) {
        if !self
            .entries
            .iter()
            .any(|entry| entry.id == id && matches!(entry.state, ImageState::Reading))
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
            .all(|entry| matches!(entry.state, ImageState::Ready(_)))
    }
    pub fn content(&self) -> Option<Vec<v1::ContentBlock>> {
        self.sendable().then(|| {
            self.entries
                .iter()
                .filter_map(|entry| match &entry.state {
                    ImageState::Ready(image) => Some(image.content()),
                    _ => None,
                })
                .collect()
        })
    }
    fn settle(&mut self) {
        let mut bytes = 0;
        for entry in &mut self.entries {
            match &entry.state {
                ImageState::Ready(image) => {
                    bytes += image.bytes;
                    continue;
                }
                ImageState::Failed(_) => continue,
                ImageState::Reading => {}
            }
            let Some(result) = self.completed.remove(&entry.id) else {
                break;
            };
            entry.state = match result {
                Ok(image) if bytes + image.bytes <= MAX_TOTAL_BYTES => {
                    bytes += image.bytes;
                    ImageState::Ready(image)
                }
                Ok(_) => ImageState::Failed("This image would exceed the 6 MiB total image budget. Dismiss it and select it again after freeing room.".into()),
                Err(error) => ImageState::Failed(error),
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prompt_image::PromptImage;

    fn image(name: &str, size: usize) -> PromptImage {
        let mut bytes = b"\x89PNG\r\n\x1a\n".to_vec();
        bytes.resize(size.max(8), 0);
        PromptImage::from_bytes(name.into(), &bytes).unwrap()
    }

    #[test]
    fn completion_order_does_not_change_selection_order_or_budget_priority() {
        let mut draft = ImageDraft::default();
        let first = draft.reserve("first.png".into()).unwrap();
        let second = draft.reserve("second.png".into()).unwrap();
        draft.finish(second, Ok(image("second.png", 3 * 1024 * 1024)));
        assert!(!draft.sendable());
        draft.finish(first, Ok(image("first.png", 4 * 1024 * 1024)));
        assert!(matches!(draft.entries()[0].state, ImageState::Ready(_)));
        assert!(matches!(draft.entries()[1].state, ImageState::Failed(_)));
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
        let mut draft = ImageDraft::default();
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
        let mut draft = ImageDraft::default();
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
        let mut draft = ImageDraft::default();
        let first = draft.reserve("five.png".into()).unwrap();
        draft.finish(first, Ok(image("five.png", 5 * 1024 * 1024)));
        let second = draft.reserve("one.png".into()).unwrap();
        draft.finish(second, Ok(image("one.png", 1024 * 1024)));
        assert_eq!(draft.bytes(), MAX_TOTAL_BYTES);
        assert!(draft.sendable());
        let third = draft.reserve("extra.png".into()).unwrap();
        draft.finish(third, Ok(image("extra.png", 8)));
        assert!(matches!(draft.entries()[2].state, ImageState::Failed(_)));
        assert!(draft.content().is_none());
    }
}
