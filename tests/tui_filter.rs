#[allow(dead_code)]
#[path = "../src/tui/panel.rs"]
mod panel;

use panel::FilterTask;

fn tasks() -> Vec<FilterTask<'static>> {
    vec![
        FilterTask {
            id: 1,
            parent_id: None,
            description: "Parent",
        },
        FilterTask {
            id: 2,
            parent_id: Some(1),
            description: "Child\nHidden CaSe match",
        },
        FilterTask {
            id: 3,
            parent_id: Some(2),
            description: "Grandchild",
        },
        FilterTask {
            id: 4,
            parent_id: None,
            description: "Other root",
        },
        FilterTask {
            id: 5,
            parent_id: Some(4),
            description: "Match sibling",
        },
    ]
}

#[test]
fn filter_matches_full_description_case_insensitively_and_keeps_ancestors() {
    let filtered = panel::filter_tasks(&tasks(), "cAsE MaTcH");
    assert_eq!(filtered.ordered_ids, vec![1, 2]);
    assert!(filtered.included_ids.contains(&1));
    assert!(!filtered.included_ids.contains(&3));
    assert!(!filtered.included_ids.contains(&4));
}

#[test]
fn filter_empty_query_restores_all_and_no_match_is_empty() {
    assert_eq!(
        panel::filter_tasks(&tasks(), "").ordered_ids,
        vec![1, 2, 3, 4, 5]
    );
    assert!(
        panel::filter_tasks(&tasks(), "absent")
            .ordered_ids
            .is_empty()
    );
}

#[test]
fn filtered_navigation_skips_hidden_ids_and_reaches_new_draft() {
    let ids = panel::filter_tasks(&tasks(), "match").ordered_ids;
    assert_eq!(ids, vec![1, 2, 4, 5]);
    assert_eq!(panel::adjacent_visible_id(&ids, None, true), Some(5));
    assert_eq!(panel::adjacent_visible_id(&ids, Some(5), true), Some(4));
    assert_eq!(panel::adjacent_visible_id(&ids, Some(3), true), Some(2));
    assert_eq!(panel::adjacent_visible_id(&ids, Some(2), false), Some(4));
    assert_eq!(panel::adjacent_visible_id(&ids, Some(5), false), None);
    assert_eq!(panel::adjacent_visible_id(&ids, None, false), None);
}
