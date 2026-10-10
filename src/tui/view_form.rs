use super::{
    render::{PopupKind, PopupRow},
    tag_input,
};
use crate::{list_filter::ListStatus, selection::Readiness, views::SavedView};
use anyhow::{Context, Result, bail};
use clap::ValueEnum;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

const FIELDS: [(&str, &str); 8] = [
    ("Name", "Exact name; spaces and Unicode allowed"),
    ("Tags", "One per line or comma-separated; blank means any"),
    ("Luau filter", "Expression; blank means any"),
    ("Query", "Description text; blank means any"),
    (
        "Statuses",
        "new,in_progress,completed,error; blank means any",
    ),
    ("Readiness", "ready or blocked; blank means any"),
    ("Archived", "hide or show"),
    ("Completed limit", "all, 0 to hide, or positive count"),
];

pub(super) enum Action {
    Save(SavedView),
    Cancel,
}

pub(super) struct Form {
    values: [String; 8],
    cursors: [tag_input::Cursor; 8],
    selected: usize,
    error: String,
}

impl Form {
    pub fn new() -> Self {
        let values = ["", "", "", "", "", "", "hide", "all"].map(str::to_owned);
        let cursors = std::array::from_fn(|index| tag_input::Cursor::at_end(&values[index]));
        Self {
            values,
            cursors,
            selected: 0,
            error: String::new(),
        }
    }

    pub fn set_error(&mut self, error: String) {
        self.error = error;
    }

    pub fn paste(&mut self, text: &str) {
        self.cursors[self.selected]
            .insert(&mut self.values[self.selected], &text.replace("\r\n", "\n"));
        self.error.clear();
    }

    fn definition(&self) -> Result<SavedView> {
        let statuses = if self.values[4].trim().is_empty() {
            Vec::new()
        } else {
            self.values[4]
                .split(',')
                .map(|value| {
                    ListStatus::from_str(value.trim(), false)
                        .map_err(anyhow::Error::msg)
                        .context("Statuses must be new,in_progress,completed,error")
                })
                .collect::<Result<Vec<_>>>()?
        };
        let readiness = match self.values[5].trim() {
            "" => None,
            value => Some(
                Readiness::from_str(value, false)
                    .map_err(anyhow::Error::msg)
                    .context("Readiness must be ready or blocked, or blank")?,
            ),
        };
        let include_archived = match self.values[6].trim() {
            "hide" => false,
            "show" => true,
            _ => bail!("Archived must be hide or show"),
        };
        let max_completed = match self.values[7].trim() {
            "all" | "" => None,
            value => Some(
                value
                    .parse::<i64>()
                    .context("Completed limit must be all or nonnegative count")?,
            ),
        };
        let mut view = SavedView {
            name: self.values[0].clone(),
            criteria: crate::selection::Selectors {
                tags: crate::tags::parse_lines(&self.values[1])?,
                filter: (!self.values[2].is_empty()).then(|| self.values[2].clone()),
                query: (!self.values[3].is_empty()).then(|| self.values[3].clone()),
                statuses,
                readiness,
            },
            include_archived,
            max_completed,
        };
        crate::views::validate(&mut view)?;
        Ok(view)
    }

    pub fn key(&mut self, key: KeyEvent) -> Option<Action> {
        let control = key.modifiers.contains(KeyModifiers::CONTROL);
        if key.code == KeyCode::Esc || (control && key.code == KeyCode::Char('c')) {
            return Some(Action::Cancel);
        }
        if key.modifiers.contains(KeyModifiers::SUPER) {
            return None;
        }
        if control && !key.modifiers.contains(KeyModifiers::ALT) && key.code == KeyCode::Char('s') {
            match self.definition() {
                Ok(view) => return Some(Action::Save(view)),
                Err(error) => self.error = format!("{error:#}"),
            }
        } else if !key
            .modifiers
            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
            && matches!(key.code, KeyCode::Tab | KeyCode::BackTab)
        {
            let previous =
                key.code == KeyCode::BackTab || key.modifiers.contains(KeyModifiers::SHIFT);
            self.selected =
                (self.selected + if previous { FIELDS.len() - 1 } else { 1 }) % FIELDS.len();
            self.error.clear();
        } else if key.code == KeyCode::Enter
            && !control
            && !key.modifiers.contains(KeyModifiers::ALT)
            && !matches!(self.selected, 1..=3)
        {
            self.selected = (self.selected + 1) % FIELDS.len();
            self.error.clear();
        } else {
            self.cursors[self.selected].edit(&mut self.values[self.selected], &mut self.error, key);
        }
        None
    }

    pub fn rows(&self, width: usize, height: usize) -> Vec<PopupRow> {
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
                PopupKind::Heading => row.text = "Create view".into(),
                PopupKind::Hint if index == last => {
                    row.text = if width >= 38 {
                        "Tab next Shift-Tab back Ctrl-S save Esc".into()
                    } else if width >= 24 {
                        "Tab/Shift-Tab ^S save Esc".into()
                    } else {
                        "Tab ^S Esc".into()
                    };
                }
                _ => (),
            }
        }
        if let Some(index) = guidance_index {
            let (name, guidance) = FIELDS[self.selected];
            let label = if width >= 24 {
                format!("{name} ({}/8): {guidance}", self.selected + 1)
            } else {
                format!("{name} ({}/8)", self.selected + 1)
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
    fn save(form: &mut Form) -> SavedView {
        match form.key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL)) {
            Some(Action::Save(view)) => view,
            _ => panic!("Expected valid view, got {}", form.error),
        }
    }

    #[test]
    fn blank_optional_fields_and_unicode_names_use_shared_validation() {
        let mut form = Form::new();
        form.paste("界 e\u{301}X");
        form.key(key(KeyCode::Backspace));
        let view = save(&mut form);
        assert_eq!(view.name, "界 e\u{301}");
        assert!(view.criteria.tags.is_empty());
        assert!(view.criteria.statuses.is_empty());
        assert!(!view.include_archived && view.max_completed.is_none());
    }

    #[test]
    fn invalid_fields_keep_values_until_corrected() {
        let mut form = Form::new();
        form.values[0] = "Ready".into();
        for (index, value) in [
            (0, " bad "),
            (1, "[tag]"),
            (2, "unknown == 1"),
            (4, "new,bad"),
            (5, "bad"),
            (6, "bad"),
            (7, "-1"),
        ] {
            let original = form.values[index].clone();
            form.values[index] = value.into();
            assert!(form.definition().is_err(), "{index}: {value}");
            assert_eq!(form.values[index], value);
            form.values[index] = original;
        }
        assert_eq!(save(&mut form).name, "Ready");
    }

    #[test]
    fn field_navigation_and_paste_preserve_each_caret() {
        let mut form = Form::new();
        form.paste("界e\u{301}");
        form.key(key(KeyCode::Left));
        form.key(key(KeyCode::Tab));
        form.paste("backend\r\n界 面\nbackend");
        form.key(key(KeyCode::BackTab));
        form.paste("X");
        let view = save(&mut form);
        assert_eq!(view.name, "界Xe\u{301}");
        assert_eq!(view.criteria.tags, vec!["backend", "界 面"]);
        assert!(matches!(
            form.key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)),
            Some(Action::Cancel)
        ));
    }

    #[test]
    fn normal_popup_width_explains_status_values_before_input() {
        let mut form = Form::new();
        form.selected = 4;
        let rows = form.rows(44, 12);
        assert!(
            rows.iter()
                .any(|row| row.kind == PopupKind::Hint && row.text.contains("error"))
        );
        let hints = rows
            .iter()
            .filter(|row| row.kind == PopupKind::Hint)
            .map(|row| row.text.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        assert!(hints.contains("blank means any"));
    }

    #[test]
    fn narrow_form_keeps_input_caret_and_bounded_rows() {
        let mut form = Form::new();
        form.paste(&"界e\u{301}".repeat(40));
        for height in 1..=12 {
            let rows = form.rows(12, height);
            assert!(rows.len() <= height);
            assert!(
                rows.iter()
                    .any(|row| row.cursor.is_some_and(|column| column < 12))
            );
        }
    }
}
