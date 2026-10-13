use super::{
    render::{PopupKind, PopupRow},
    tag_input, view_templates,
};
use crate::views::SavedView;
use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use unicode_width::UnicodeWidthStr;

const FIELDS: [(&str, &str); 2] = [
    ("Name", "Exact name; spaces and Unicode allowed"),
    ("Luau script", "One filter expression; Enter adds a line"),
];

pub(super) enum Action {
    Save(SavedView),
    Cancel,
}

pub(super) struct Form {
    values: [String; 2],
    cursors: [tag_input::Cursor; 2],
    selected: usize,
    error: String,
    templates: Option<view_templates::Picker>,
}

impl Form {
    pub fn new() -> Self {
        let values = ["", "not archived"].map(str::to_owned);
        let cursors = std::array::from_fn(|index| tag_input::Cursor::at_end(&values[index]));
        Self {
            values,
            cursors,
            selected: 0,
            error: String::new(),
            templates: None,
        }
    }

    pub fn set_error(&mut self, error: String) {
        self.error = error;
    }

    pub fn paste(&mut self, text: &str) {
        if self.templates.is_none() {
            self.cursors[self.selected]
                .insert(&mut self.values[self.selected], &text.replace("\r\n", "\n"));
            self.error.clear();
        }
    }

    fn definition(&self) -> Result<SavedView> {
        let mut view = SavedView {
            name: self.values[0].clone(),
            criteria: crate::selection::Selectors {
                filter: Some(self.values[1].clone()),
                ..Default::default()
            },
            // The expression controls archive/status matching without hidden limits.
            include_archived: true,
            max_completed: None,
        };
        crate::views::validate(&mut view)?;
        Ok(view)
    }

    pub fn key(&mut self, key: KeyEvent) -> Option<Action> {
        if let Some(picker) = &mut self.templates {
            match picker.key(key) {
                Some(view_templates::Action::Use(script)) => {
                    self.values[1] = script.to_owned();
                    self.cursors[1] = tag_input::Cursor::at_end(script);
                    self.selected = 1;
                    self.error.clear();
                    self.templates = None;
                }
                Some(view_templates::Action::Cancel) => self.templates = None,
                None => (),
            }
            return None;
        }
        let control = key.modifiers.contains(KeyModifiers::CONTROL);
        if key.code == KeyCode::Esc || (control && key.code == KeyCode::Char('c')) {
            return Some(Action::Cancel);
        }
        if key.modifiers.contains(KeyModifiers::SUPER) {
            return None;
        }
        if control && !key.modifiers.contains(KeyModifiers::ALT) && key.code == KeyCode::Char('f') {
            self.templates = Some(view_templates::Picker::new());
        } else if control
            && !key.modifiers.contains(KeyModifiers::ALT)
            && key.code == KeyCode::Char('s')
        {
            match self.definition() {
                Ok(view) => return Some(Action::Save(view)),
                Err(error) => self.error = format!("{error:#}"),
            }
        } else if !key
            .modifiers
            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
            && (matches!(key.code, KeyCode::Tab | KeyCode::BackTab)
                || (key.code == KeyCode::Enter && self.selected == 0))
        {
            self.selected = 1 - self.selected;
            self.error.clear();
        } else {
            self.cursors[self.selected].edit(&mut self.values[self.selected], &mut self.error, key);
        }
        None
    }

    pub fn rows(&self, width: usize, height: usize) -> Vec<PopupRow> {
        if let Some(picker) = &self.templates {
            return picker.rows(width, height);
        }
        let mut rows = tag_input::rows(
            None,
            &self.values[self.selected],
            &self.cursors[self.selected],
            &self.error,
            width,
            height,
        );
        let last = rows.len().saturating_sub(1);
        let guidance_index = rows.iter().enumerate().find_map(|(index, row)| {
            (row.kind == PopupKind::Hint && index != last).then_some(index)
        });
        for (index, row) in rows.iter_mut().enumerate() {
            match row.kind {
                PopupKind::Heading => {
                    row.text = if self.selected == 1 && !self.values[0].is_empty() {
                        format!("Create view: {}", self.values[0])
                    } else {
                        "Create view".into()
                    };
                }
                PopupKind::Hint if index == last => {
                    row.text = [
                        "Tab field Ctrl-F templates Ctrl-S save Esc",
                        "Tab ^F templates ^S save Esc",
                        "Tab ^F ^S Esc",
                        "⇥ ^F ^S Esc",
                    ]
                    .into_iter()
                    .find(|text| text.width() <= width)
                    .unwrap_or("⇥ ^F ^S Esc")
                    .into();
                }
                _ => (),
            }
        }
        if let Some(index) = guidance_index {
            let (name, guidance) = FIELDS[self.selected];
            let label = if width >= 24 {
                format!("{name}: {guidance}")
            } else {
                name.to_owned()
            };
            let available = 1 + height.saturating_sub(rows.len());
            let guidance = super::wrap_modal(&label, width)
                .into_iter()
                .take(available)
                .map(|line| PopupRow::new(line, PopupKind::Hint));
            rows.splice(index..=index, guidance);
        }
        rows
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }
    fn control(letter: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(letter), KeyModifiers::CONTROL)
    }
    fn save(form: &mut Form) -> SavedView {
        match form.key(control('s')) {
            Some(Action::Save(view)) => view,
            _ => panic!("Expected valid view, got {}", form.error),
        }
    }

    #[test]
    fn name_and_script_are_only_fields_and_keep_independent_carets() {
        let mut form = Form::new();
        form.paste("界e\u{301}");
        form.key(key(KeyCode::Left));
        form.key(key(KeyCode::Tab));
        form.key(control('u'));
        let script = "-- Match backend work\r\nnot archived\nand has_tag(\"backend\")";
        form.paste(script);
        form.key(key(KeyCode::Tab));
        form.paste("X");
        let view = save(&mut form);
        assert_eq!(view.name, "界Xe\u{301}");
        assert_eq!(
            view.criteria.filter.as_deref(),
            Some(script.replace("\r\n", "\n").as_str())
        );
        assert!(view.criteria.tags.is_empty());
        assert!(view.criteria.statuses.is_empty());
        assert!(view.criteria.query.is_none() && view.criteria.readiness.is_none());
        assert!(view.include_archived && view.max_completed.is_none());
    }

    #[test]
    fn default_script_hides_archives_without_other_selector_fields() {
        let mut form = Form::new();
        form.paste("Default");
        let view = save(&mut form);
        assert_eq!(view.criteria.filter.as_deref(), Some("not archived"));
        assert!(view.include_archived && view.max_completed.is_none());
    }

    #[test]
    fn invalid_script_and_name_keep_drafts_for_correction() {
        let mut form = Form::new();
        form.paste(" bad ");
        assert!(form.key(control('s')).is_none());
        assert!(form.error.contains("View names must be nonempty"));
        form.key(control('u'));
        form.paste("Ready");
        form.key(key(KeyCode::Tab));
        form.key(control('u'));
        for script in ["", "unknown == 1", "return true", "priority > \"bad\""] {
            form.paste(script);
            assert!(form.key(control('s')).is_none(), "{script:?}");
            assert!(!form.error.is_empty());
            assert_eq!(form.values[1], script);
            form.key(control('u'));
        }
        form.paste("status == \"new\"");
        assert_eq!(save(&mut form).name, "Ready");
    }

    #[test]
    fn templates_are_optional_cancel_preserves_name_script_and_caret() {
        let mut form = Form::new();
        form.paste("Custom");
        form.key(key(KeyCode::Tab));
        form.key(control('u'));
        form.paste("priority > 5");
        form.key(key(KeyCode::Left));
        form.key(control('f'));
        assert!(
            form.rows(60, 14)
                .iter()
                .any(|row| row.text == "View templates")
        );
        form.key(key(KeyCode::Down));
        form.paste("ignored while choosing");
        assert!(form.key(control('s')).is_none());
        assert!(form.key(key(KeyCode::Esc)).is_none());
        form.paste("1");
        let view = save(&mut form);
        assert_eq!(view.name, "Custom");
        assert_eq!(view.criteria.filter.as_deref(), Some("priority > 15"));
        assert!(matches!(form.key(control('c')), Some(Action::Cancel)));
    }

    #[test]
    fn template_picker_uses_non_text_q_escape_alias() {
        for letter in ['q', 'Q'] {
            let mut form = Form::new();
            form.paste("Name");
            form.key(control('f'));
            assert!(form.key(key(KeyCode::Char(letter))).is_none());
            assert!(
                form.rows(60, 12)
                    .iter()
                    .any(|row| row.text == "Create view")
            );
            assert_eq!(
                save(&mut form).criteria.filter.as_deref(),
                Some("not archived")
            );
        }
    }

    #[test]
    fn each_template_populates_editable_valid_script_without_renaming() {
        for (index, expected) in [
            "not archived",
            "not archived and status ~= \"completed\"",
            "not archived and has_tag(\"backend\")",
            "not archived and priority >= 5",
            "not archived\nand datetime(created_time) >= datetime(\"now\", \"-7 days\")",
            "true",
        ]
        .into_iter()
        .enumerate()
        {
            let mut form = Form::new();
            form.paste("My view");
            form.key(control('f'));
            for _ in 0..index {
                form.key(key(KeyCode::Down));
            }
            form.key(key(KeyCode::Enter));
            let view = save(&mut form);
            assert_eq!(view.name, "My view");
            assert_eq!(view.criteria.filter.as_deref(), Some(expected));
            assert!(view.include_archived && view.max_completed.is_none());
            form.paste(" and priority >= 0");
            assert!(
                save(&mut form)
                    .criteria
                    .filter
                    .unwrap()
                    .ends_with(" and priority >= 0")
            );
        }
    }

    #[test]
    fn narrow_form_keeps_script_caret_and_templates_accessible() {
        let mut form = Form::new();
        form.paste("Name");
        form.key(key(KeyCode::Enter));
        form.key(control('u'));
        form.paste(&"界e\u{301}".repeat(40));
        for height in 1..=12 {
            let rows = form.rows(12, height);
            assert!(rows.len() <= height);
            assert!(
                rows.iter()
                    .any(|row| row.cursor.is_some_and(|column| column < 12))
            );
            assert!(rows.iter().all(|row| !row.text.contains("/8")));
            if height >= 2 {
                let hint = &rows.last().unwrap().text;
                assert!(
                    hint.contains("^F") && hint.contains("^S") && hint.contains("Esc"),
                    "{hint}"
                );
                assert!(unicode_width::UnicodeWidthStr::width(hint.as_str()) <= 12);
            }
        }
        form.key(control('f'));
        form.key(key(KeyCode::End));
        for height in 1..=12 {
            let rows = form.rows(12, height);
            assert!(rows.len() <= height);
            assert_eq!(
                rows.iter()
                    .filter(|row| row.kind == PopupKind::SelectedChoice)
                    .count(),
                1
            );
            assert!(rows.iter().any(|row| row.text == "All tasks"));
        }
        form.key(control('c'));
        assert!(form.rows(12, 12).iter().any(|row| row.cursor.is_some()));
    }
}
