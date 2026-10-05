#[allow(dead_code)]
#[path = "../src/tui/panel.rs"]
mod panel;

use panel::FilterTask;
use std::collections::HashSet;

#[test]
fn tag_ranges_follow_real_unicode_labels_and_skip_bracket_descriptions() {
    let tasks: Vec<_> = (1..=4)
        .map(|id| FilterTask {
            id,
            parent_id: None,
            status: "new",
            description: "",
        })
        .collect();
    let mut rows = panel::rows(
        concat!(
            "1      New          [界 面] \n",
            "                    [bug] [fake]\n",
            "                    Description\n",
            "2      New          [fake]\n",
            "3      New          [archived] [bug] [fake]\n",
            "4      New          [archived] Literal",
        ),
        72,
    );
    panel::set_dirty_markers(&mut rows, &tasks, &HashSet::new(), true);
    let labels = ["界 面".into(), "bug".into()];
    let bug = ["bug".into()];
    let archived_label = ["archived".into()];
    panel::set_tag_ranges(
        &mut rows,
        &[
            panel::TagTask {
                id: 1,
                tags: &labels,
                archived: false,
            },
            panel::TagTask {
                id: 2,
                tags: &[],
                archived: false,
            },
            panel::TagTask {
                id: 3,
                tags: &bug,
                archived: true,
            },
            panel::TagTask {
                id: 4,
                tags: &archived_label,
                archived: false,
            },
        ],
    );
    let actual: Vec<_> = rows
        .iter()
        .map(|row| row.tag_range.map(|(start, end)| &row.text[start..end]))
        .collect();
    assert_eq!(
        actual,
        [
            Some("[界 面] "),
            Some("[bug]"),
            None,
            None,
            Some("[bug]"),
            Some("[archived]")
        ]
    );
    panel::set_tag_ranges(&mut rows, &[]);
    assert!(rows.iter().all(|row| row.tag_range.is_none()));
}

#[test]
fn tag_ranges_stop_before_preview_ellipsis() {
    for suffix in ["012", "..."] {
        let tree = format!(
            "1      New          [abcdefghi\n                    jklmnopqrs\n                    tuvwxyz{suffix}\n                    345] Desc"
        );
        let mut rows = panel::rows(&tree, 30);
        panel::set_dirty_markers(
            &mut rows,
            &[FilterTask {
                id: 1,
                parent_id: None,
                status: "new",
                description: "Desc",
            }],
            &HashSet::new(),
            true,
        );
        panel::set_tag_ranges(
            &mut rows,
            &[panel::TagTask {
                id: 1,
                tags: &[format!("abcdefghijklmnopqrstuvwxyz{suffix}345")],
                archived: false,
            }],
        );
        assert_eq!(rows.len(), 3);
        let (start, end) = rows[2].tag_range.unwrap();
        assert_eq!(&rows[2].text[start..end], "tuvwxyz");
        assert_eq!(&rows[2].text[end..], "...");
    }
}

fn tasks() -> Vec<FilterTask<'static>> {
    vec![
        FilterTask {
            status: "new",
            id: 1,
            parent_id: None,
            description: "Parent",
        },
        FilterTask {
            status: "new",
            id: 2,
            parent_id: Some(1),
            description: "Child\nHidden CaSe match",
        },
        FilterTask {
            status: "new",
            id: 3,
            parent_id: Some(2),
            description: "Grandchild",
        },
        FilterTask {
            status: "new",
            id: 4,
            parent_id: None,
            description: "Other root",
        },
        FilterTask {
            status: "new",
            id: 5,
            parent_id: Some(4),
            description: "Match sibling",
        },
    ]
}

#[test]
fn filter_matches_full_description_case_insensitively_and_keeps_ancestors() {
    let filtered = panel::filter_tasks(&tasks(), "cAsE MaTcH", true);
    assert_eq!(filtered.included_ids, HashSet::from([1, 2]));
    assert!(filtered.included_ids.contains(&1));
    assert!(!filtered.included_ids.contains(&3));
    assert!(!filtered.included_ids.contains(&4));
}

#[test]
fn filter_empty_query_restores_all_and_no_match_is_empty() {
    assert_eq!(
        panel::filter_tasks(&tasks(), "", true).included_ids,
        HashSet::from([1, 2, 3, 4, 5])
    );
    assert!(
        panel::filter_tasks(&tasks(), "absent", true)
            .included_ids
            .is_empty()
    );
}

#[test]
fn completed_visibility_excludes_ancestors_without_hiding_unfinished_children() {
    let mut tasks = tasks();
    tasks[0].status = "completed";
    tasks[2].status = "completed";
    tasks[3].status = "error";
    tasks[4].status = "in_progress";
    assert_eq!(
        panel::filter_tasks(&tasks, "", false).included_ids,
        HashSet::from([2, 4, 5])
    );
    assert_eq!(
        panel::filter_tasks(&tasks, "match", false).included_ids,
        HashSet::from([2, 4, 5])
    );
    assert_eq!(
        panel::filter_tasks(&tasks, "case", false).included_ids,
        HashSet::from([2])
    );
    assert!(
        panel::filter_tasks(&tasks, "parent", false)
            .included_ids
            .is_empty()
    );
    assert_eq!(
        panel::filter_tasks(&tasks, "case", true).included_ids,
        HashSet::from([1, 2])
    );
    let filtered = panel::filter_tasks(&tasks, "", false);
    let ids: Vec<_> = tasks
        .iter()
        .filter(|task| filtered.included_ids.contains(&task.id))
        .map(|task| task.id)
        .collect();
    assert_eq!(panel::adjacent_visible_id(&ids, None, true), Some(5));
    assert_eq!(panel::adjacent_visible_id(&ids, Some(5), true), Some(4));
    assert_eq!(panel::adjacent_visible_id(&ids, Some(4), true), Some(2));
    assert_eq!(panel::adjacent_visible_id(&ids, Some(2), true), None);
}

#[test]
fn filtered_navigation_skips_hidden_ids_and_reaches_new_draft() {
    let filtered = panel::filter_tasks(&tasks(), "match", true);
    let ids: Vec<_> = tasks()
        .iter()
        .filter(|task| filtered.included_ids.contains(&task.id))
        .map(|task| task.id)
        .collect();
    assert_eq!(ids, vec![1, 2, 4, 5]);
    assert_eq!(panel::adjacent_visible_id(&ids, None, true), Some(5));
    assert_eq!(panel::adjacent_visible_id(&ids, Some(5), true), Some(4));
    assert_eq!(panel::adjacent_visible_id(&ids, Some(3), true), Some(5));
    assert_eq!(panel::adjacent_visible_id(&ids, Some(3), false), Some(1));
    assert_eq!(panel::adjacent_visible_id(&ids, Some(2), false), Some(4));
    assert_eq!(panel::adjacent_visible_id(&ids, Some(5), false), None);
    assert_eq!(panel::adjacent_visible_id(&ids, None, false), None);
}

#[test]
fn navigation_follows_tree_order_instead_of_numeric_ids() {
    let ids = [1, 4, 2, 3];
    assert_eq!(panel::adjacent_visible_id(&ids, None, true), Some(3));
    assert_eq!(panel::adjacent_visible_id(&ids, Some(3), true), Some(2));
    assert_eq!(panel::adjacent_visible_id(&ids, Some(2), true), Some(4));
    assert_eq!(panel::adjacent_visible_id(&ids, Some(4), false), Some(2));
    assert_eq!(panel::adjacent_visible_id(&ids, Some(1), true), None);
    assert_eq!(panel::adjacent_visible_id(&ids, Some(3), false), None);
}

#[test]
fn visible_navigation_uses_displayed_rows_once_per_task() {
    let rows = panel::rows(
        "ID STATUS TASK\n1 New Parent\n4 New Child\n  Wrapped detail\n2 New Other\n3 New Last",
        80,
    );
    assert_eq!(panel::visible_ids(&rows), vec![1, 4, 2, 3]);
}

#[test]
fn dirty_description_offsets_use_hierarchy_with_literal_symbols_and_large_ids() {
    let tasks = [
        FilterTask {
            status: "new",
            id: 1,
            parent_id: None,
            description: "└── Literal root",
        },
        FilterTask {
            status: "new",
            id: 2,
            parent_id: Some(1),
            description: "│   Literal child λ",
        },
        FilterTask {
            status: "new",
            id: 1_234_567,
            parent_id: Some(2),
            description: "Deep",
        },
        FilterTask {
            status: "new",
            id: 4,
            parent_id: Some(99),
            description: "",
        },
    ];
    let tree = "1      New          └── Literal root\n                    Root continuation\n2      Completed    └── │   Literal child λ\n1234567 In progress      └── Deep\n4      New          ";
    let mut rows = panel::rows(tree, 70);
    panel::set_dirty_markers(
        &mut rows,
        &tasks,
        &HashSet::from([1, 2, 1_234_567, 4]),
        true,
    );
    assert_eq!(
        &rows[0].text[rows[0].description_start.unwrap()..],
        "└── Literal root"
    );
    assert!(rows[1].description_start.is_none());
    assert_eq!(
        &rows[2].text[rows[2].description_start.unwrap()..],
        "│   Literal child λ"
    );
    assert_eq!(&rows[3].text[rows[3].description_start.unwrap()..], "Deep");
    assert_eq!(rows[4].description_start, Some(rows[4].text.len()));
    assert!(rows.iter().all(|row| row.dirty));
    let text: Vec<_> = rows.iter().map(|row| row.text.clone()).collect();
    panel::set_dirty_markers(&mut rows, &tasks, &HashSet::new(), true);
    assert!(rows.iter().all(|row| !row.dirty));
    assert_eq!(
        rows.iter().map(|row| row.text.clone()).collect::<Vec<_>>(),
        text
    );
}
