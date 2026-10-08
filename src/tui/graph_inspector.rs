use super::render::{PopupKind, PopupRow};
use crate::{
    db::Db,
    graph::{self, Direction, Options, Report},
};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

pub(super) struct Inspector {
    id: i64,
    options: Options,
    version: Option<i64>,
    report: Option<Report>,
    error: String,
    top: usize,
}

impl Inspector {
    pub fn new(id: i64) -> Self {
        Self {
            id,
            options: Options::default(),
            version: None,
            report: None,
            error: String::new(),
            top: 0,
        }
    }
    pub fn refresh(&mut self, db: &Db, version: i64) {
        if self.version == Some(version) {
            return;
        }
        self.version = Some(version);
        match db.graph(self.id, self.options) {
            Ok(report) => {
                self.report = Some(report);
                self.error.clear();
            }
            Err(error) => {
                self.report = None;
                self.error = crate::output::clean(&format!("{error:#}"));
            }
        }
    }
    pub fn set_error(&mut self, error: &anyhow::Error, version: i64) {
        self.version = Some(version);
        self.report = None;
        self.error = crate::output::clean(&format!("{error:#}"));
    }
    pub fn key(&mut self, key: KeyEvent, height: usize) -> bool {
        if key.code == KeyCode::Esc
            || (key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c'))
        {
            return true;
        }
        if key
            .modifiers
            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER)
        {
            return false;
        }
        let previous = self.options;
        match key.code {
            KeyCode::Up => self.top = self.top.saturating_sub(1),
            KeyCode::Down => self.top = self.top.saturating_add(1),
            KeyCode::PageUp => self.top = self.top.saturating_sub(height.saturating_sub(3).max(1)),
            KeyCode::PageDown => {
                self.top = self.top.saturating_add(height.saturating_sub(3).max(1))
            }
            KeyCode::Home => self.top = 0,
            KeyCode::End => self.top = usize::MAX,
            KeyCode::Char('u') => self.options.direction = Direction::Upstream,
            KeyCode::Char('d') => self.options.direction = Direction::Downstream,
            KeyCode::Char('b') => self.options.direction = Direction::Both,
            KeyCode::Char('+' | '=') => self.options.depth = (self.options.depth + 1).min(128),
            KeyCode::Char('-') => self.options.depth = self.options.depth.saturating_sub(1),
            _ => (),
        }
        if previous.direction != self.options.direction || previous.depth != self.options.depth {
            self.version = None;
            self.top = 0;
        }
        false
    }
    pub fn rows(&mut self, width: usize, height: usize) -> Vec<PopupRow> {
        let direction = match self.options.direction {
            Direction::Both => "both",
            Direction::Upstream => "upstream",
            Direction::Downstream => "downstream",
        };
        let mut rows = vec![
            PopupRow::new(format!("Dependency graph #{}", self.id), PopupKind::Heading),
            PopupRow::new(
                format!("{direction} · depth {}", self.options.depth),
                PopupKind::Hint,
            ),
        ];
        let lines = if let Some(report) = &self.report {
            graph::lines(report).into_iter().skip(1).collect()
        } else {
            vec![self.error.clone()]
        };
        let wrapped: Vec<_> = lines
            .iter()
            .flat_map(|line| super::wrap_modal(line, width))
            .collect();
        let available = height.saturating_sub(3).max(1);
        self.top = self.top.min(wrapped.len().saturating_sub(available));
        let kind = if self.error.is_empty() {
            PopupKind::Body
        } else {
            PopupKind::Error
        };
        rows.extend(
            wrapped
                .into_iter()
                .skip(self.top)
                .take(available)
                .map(|line| PopupRow::new(line, kind)),
        );
        rows.push(PopupRow::new(
            if width >= 64 {
                "u upstream d downstream b both +/- depth ↑↓ PgUp/PgDn Esc"
            } else {
                "u/d/b +/- ↑↓ Pg↑↓ Esc"
            },
            PopupKind::Hint,
        ));
        rows
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_cache_skips_reads_until_database_or_options_change_and_retains_errors() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE tasks(id INTEGER PRIMARY KEY, description TEXT, status TEXT, archived INTEGER, parent_id INTEGER);
            CREATE TABLE task_dependencies(task_id INTEGER, prerequisite_id INTEGER);
            INSERT INTO tasks VALUES (1,'Root','new',0,NULL);").unwrap();
        let dir = tempfile::TempDir::new().unwrap();
        let db = Db {
            conn,
            image_store: crate::images::ImageStore::new(dir.path().join("images")),
        };
        let mut ui = Inspector::new(1);
        ui.refresh(&db, 1);
        assert!(ui.report.is_some() && ui.error.is_empty());
        db.conn
            .execute_batch("DROP TABLE task_dependencies")
            .unwrap();
        ui.refresh(&db, 1);
        assert!(ui.report.is_some(), "same version does not reread graph");
        ui.key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::NONE), 18);
        ui.refresh(&db, 1);
        assert!(
            ui.report.is_none() && !ui.error.is_empty(),
            "option change invalidates cache"
        );
        db.conn
            .execute_batch(
                "CREATE TABLE task_dependencies(task_id INTEGER, prerequisite_id INTEGER)",
            )
            .unwrap();
        ui.refresh(&db, 1);
        assert!(
            ui.report.is_none(),
            "failed version remains cached until commit"
        );
        ui.refresh(&db, 2);
        assert!(ui.report.is_some() && ui.error.is_empty());
        assert!(
            !dir.path().join("images").exists(),
            "inspection never creates image storage"
        );
    }

    #[test]
    fn inspector_keys_bound_depth_and_scroll_without_accepting_draft_input() {
        let mut ui = Inspector::new(1);
        for _ in 0..150 {
            ui.key(KeyEvent::new(KeyCode::Char('+'), KeyModifiers::SHIFT), 18);
        }
        assert_eq!(ui.options.depth, 128);
        for _ in 0..150 {
            ui.key(KeyEvent::new(KeyCode::Char('-'), KeyModifiers::NONE), 18);
        }
        assert_eq!(ui.options.depth, 0);
        ui.error = "DB unavailable; local draft retained".repeat(20);
        ui.key(KeyEvent::new(KeyCode::End, KeyModifiers::NONE), 18);
        let end_rows = ui.rows(32, 12);
        assert!(end_rows.len() <= 12 && ui.top > 0);
        ui.key(KeyEvent::new(KeyCode::Home, KeyModifiers::NONE), 18);
        assert_eq!(ui.top, 0);
        assert!(!ui.key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL), 18));
        assert!(ui.key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL), 18));
        assert!(ui.key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE), 18));
    }
}
