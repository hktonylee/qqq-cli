use super::draft::Draft;
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub(super) enum DraftKey {
    Task(i64),
    New(Option<i64>),
}

impl DraftKey {
    pub fn current(id: Option<i64>, parent: Option<i64>) -> Self {
        id.map_or(Self::New(parent), Self::Task)
    }

    pub fn label(self) -> String {
        match self {
            Self::Task(id) => format!("task #{id}"),
            Self::New(None) => "new draft".into(),
            Self::New(Some(parent)) => format!("child draft (parent #{parent})"),
        }
    }
}

pub(super) struct ParkedDraft {
    pub draft: Draft,
    pub baseline: String,
    pub top: usize,
    pub follow_cursor: bool,
}

#[derive(Default)]
pub(super) struct DraftBuffers {
    entries: BTreeMap<DraftKey, ParkedDraft>,
}

impl DraftBuffers {
    pub fn park(
        &mut self,
        key: DraftKey,
        draft: &mut Draft,
        baseline: &str,
        top: usize,
        follow_cursor: bool,
    ) {
        if draft.is_dirty_against(baseline) {
            self.entries.insert(
                key,
                ParkedDraft {
                    draft: std::mem::replace(draft, Draft::new("")),
                    baseline: baseline.to_owned(),
                    top,
                    follow_cursor,
                },
            );
        }
    }

    pub fn take(&mut self, key: DraftKey) -> Option<ParkedDraft> {
        self.entries.remove(&key)
    }

    pub fn get(&self, key: DraftKey) -> Option<&ParkedDraft> {
        self.entries.get(&key)
    }

    pub fn keys(&self) -> Vec<DraftKey> {
        self.entries.keys().copied().collect()
    }

    pub fn task_ids(&self) -> impl Iterator<Item = i64> + '_ {
        self.entries.keys().filter_map(|key| match key {
            DraftKey::Task(id) => Some(*id),
            DraftKey::New(_) => None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{DraftBuffers, DraftKey};
    use crate::images::ImageInput;
    use crate::tui::draft::Draft;

    #[test]
    fn restores_task_contents_baseline_caret_and_manual_scroll() {
        let mut buffers = DraftBuffers::default();
        let mut draft = Draft::new("Saved\nSecond");
        draft.left();
        draft.insert("!");
        let cursor = draft.cursor();
        let contents = draft.finish().unwrap().description;
        buffers.park(DraftKey::Task(2), &mut draft, "Saved\nSecond", 7, false);
        assert!(draft.is_empty());
        assert_eq!(buffers.task_ids().collect::<Vec<_>>(), vec![2]);
        let restored = buffers.take(DraftKey::Task(2)).unwrap();
        assert_eq!(restored.draft.finish().unwrap().description, contents);
        assert_eq!(restored.draft.cursor(), cursor);
        assert_eq!(restored.baseline, "Saved\nSecond");
        assert_eq!(restored.top, 7);
        assert!(!restored.follow_cursor);
        assert!(restored.draft.is_dirty_against(&restored.baseline));
        assert!(buffers.keys().is_empty());
    }

    #[test]
    fn task_general_and_child_drafts_restore_independently_in_key_order() {
        let mut buffers = DraftBuffers::default();
        for (key, text) in [
            (DraftKey::New(Some(6)), "Child six"),
            (DraftKey::Task(2), "Second edit"),
            (DraftKey::New(None), "General draft"),
            (DraftKey::Task(1), "First edit"),
            (DraftKey::New(Some(5)), "Child five"),
        ] {
            buffers.park(key, &mut Draft::new(text), "", 0, true);
        }
        assert_eq!(
            buffers.keys(),
            vec![
                DraftKey::Task(1),
                DraftKey::Task(2),
                DraftKey::New(None),
                DraftKey::New(Some(5)),
                DraftKey::New(Some(6)),
            ]
        );
        assert_eq!(buffers.task_ids().collect::<Vec<_>>(), vec![1, 2]);
        assert_eq!(
            buffers
                .get(DraftKey::Task(1))
                .unwrap()
                .draft
                .finish()
                .unwrap()
                .description,
            "First edit"
        );
        let general = buffers.take(DraftKey::New(None)).unwrap();
        assert_eq!(general.draft.finish().unwrap().description, "General draft");
        assert_eq!(buffers.keys().len(), 4);
        let child = buffers.take(DraftKey::New(Some(5))).unwrap();
        assert_eq!(child.draft.finish().unwrap().description, "Child five");
        assert!(buffers.get(DraftKey::New(Some(6))).is_some());
        assert_eq!(DraftKey::current(Some(2), Some(5)), DraftKey::Task(2));
        assert_eq!(DraftKey::current(None, Some(5)), DraftKey::New(Some(5)));
    }

    #[test]
    fn clean_and_reverted_edits_remain_active_without_retention() {
        let mut buffers = DraftBuffers::default();
        let mut draft = Draft::new("Saved");
        draft.left();
        buffers.park(DraftKey::Task(1), &mut draft, "Saved", 0, true);
        assert!(buffers.keys().is_empty());
        assert_eq!(draft.finish().unwrap().description, "Saved");
        draft.insert("!");
        draft.backspace();
        buffers.park(DraftKey::Task(1), &mut draft, "Saved", 0, true);
        assert!(buffers.keys().is_empty());
        assert_eq!(draft.finish().unwrap().description, "Saved");
    }

    #[test]
    fn returning_to_task_keeps_edits_with_original_baseline_and_latest_status() {
        use crate::tui::{Target, load_target, restore_target};

        let mut buffers = DraftBuffers::default();
        buffers.park(
            DraftKey::Task(2),
            &mut Draft::new("Local edits"),
            "Original",
            7,
            false,
        );
        let target = restore_target(
            Target::Task {
                id: 2,
                description: "External edit".into(),
                status: "error".into(),
                draft: Draft::new("External edit"),
            },
            &mut buffers,
        );
        let mut draft = Draft::new("");
        let mut id = None;
        let mut status = None;
        let mut parent = None;
        let mut baseline = String::new();
        let mut top = 0;
        let follow = load_target(
            target,
            &mut draft,
            &mut id,
            &mut status,
            &mut parent,
            &mut baseline,
            &mut top,
        );
        assert_eq!(id, Some(2));
        assert_eq!(status.as_deref(), Some("error"));
        assert_eq!(parent, None);
        assert_eq!(baseline, "Original");
        assert_eq!(draft.finish().unwrap().description, "Local edits");
        assert_eq!(top, 7);
        assert!(!follow);
        assert!(buffers.keys().is_empty());
    }

    #[test]
    fn retains_pending_image_bytes_paste_atoms_and_image_numbering() {
        let mut buffers = DraftBuffers::default();
        let mut draft = Draft::new("Saved");
        let data = b"\x89PNG\r\n\x1a\nbytes".to_vec();
        draft
            .image(ImageInput {
                name: "x.png".into(),
                data: data.clone(),
            })
            .unwrap();
        draft.paste(&"x".repeat(1001));
        draft.left();
        let cursor = draft.cursor();
        let fragments = draft.fragments();
        let images = draft.image_mask();
        let pastes = draft.paste_mask();
        let description = draft.finish().unwrap().description;
        buffers.park(DraftKey::Task(4), &mut draft, "Saved", 9, false);
        let mut restored = buffers.take(DraftKey::Task(4)).unwrap();
        assert_eq!(restored.draft.fragments(), fragments);
        assert_eq!(restored.draft.image_mask(), images);
        assert_eq!(restored.draft.paste_mask(), pastes);
        assert_eq!(restored.draft.cursor(), cursor);
        assert_eq!(restored.draft.finish().unwrap().description, description);
        assert_eq!(restored.draft.finish().unwrap().images[0].data, data);
        restored
            .draft
            .image(ImageInput {
                name: "y.png".into(),
                data,
            })
            .unwrap();
        assert!(
            restored
                .draft
                .fragments()
                .iter()
                .any(|text| text == "[Image #2: y.png]")
        );
    }
}
