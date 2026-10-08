#[allow(dead_code)]
#[path = "../src/errors.rs"]
mod errors;
#[allow(dead_code)]
#[path = "../src/sql_filter/mod.rs"]
mod sql_filter;
#[allow(dead_code)]
#[path = "../src/tags.rs"]
mod tags;

use rusqlite::{Connection, params_from_iter, types::Value};

fn evaluate(source: &str) -> bool {
    let filter = sql_filter::compile(source).unwrap();
    let db = Connection::open_in_memory().unwrap();
    db.execute_batch("CREATE TABLE tasks(id INTEGER,description TEXT,status TEXT,priority INTEGER,archived INTEGER,parent_id INTEGER,created_at TEXT,updated_at TEXT,harness_name TEXT,harness_session TEXT,orchestrator_name TEXT,orchestrator_session TEXT,tags TEXT); INSERT INTO tasks VALUES(1,'Fix auth','new',3,0,NULL,'2026-10-02T00:00:00Z','2026-10-02T00:00:00Z',NULL,NULL,NULL,NULL,'[\"frontend\",\"界 面\"]')").unwrap();
    db.query_row(
        &format!("SELECT {} FROM tasks", filter.sql()),
        params_from_iter(filter.params()),
        |row| row.get(0),
    )
    .unwrap()
}

#[test]
fn has_tag_matches_exact_normalized_literals() {
    for expression in [
        "has_tag('frontend')",
        "has_tag(' frontend ')",
        "has_tag('界 面') and priority == 3 and status == 'new'",
        "has_tag(('frontend'))",
        "not has_tag('Frontend')",
        "not has_tag('front')",
        "not has_tag('auth')",
        "has_tag('absent') or has_tag('frontend')",
        "(if has_tag('frontend') then priority else 0) == 3",
    ] {
        assert!(evaluate(expression), "{expression}");
    }
    let filter = sql_filter::compile("has_tag(\"' OR 1=1 --\")").unwrap();
    assert!(!filter.sql().contains("OR 1=1"));
    assert_eq!(filter.params(), &[Value::Text("' OR 1=1 --".into())]);
}

#[test]
fn has_tag_rejects_nonliteral_or_invalid_labels() {
    for expression in [
        "has_tag()",
        "has_tag('a','b')",
        "has_tag(nil)",
        "has_tag(1)",
        "has_tag(description)",
        "has_tag(lower('UI'))",
        "has_tag('UI' .. ' review')",
        "has_tag('')",
        "has_tag(' ')",
        "has_tag('a,b')",
        "has_tag('[ui]')",
        r"has_tag('a\nb')",
        "has_tag('frontend') + 1",
    ] {
        assert!(sql_filter::compile(expression).is_err(), "{expression}");
    }
}

#[test]
fn literals_are_bound() {
    let filter = sql_filter::compile("description == \"' OR 1=1 --\"").unwrap();
    assert!(!filter.sql().contains("OR 1=1"));
    assert_eq!(filter.params(), &[Value::Text("' OR 1=1 --".into())]);
    assert!(!evaluate("description == \"' OR 1=1 --\""));
}

#[test]
fn luau_precedence_nil_and_truthiness() {
    for expression in [
        "priority + 2 * 3 == 9",
        "parent_id == nil",
        "not parent_id",
        "not false and not nil",
        "(nil and true) == nil",
        "(nil or false) == false",
        "(nullif(archived, false) and true) == nil",
        "parent_id == harness_name",
        "not not 0",
        "not not ''",
        "(0 or 1) == 0",
        "('' or 'fallback') == ''",
        "(parent_id or 0) == 0",
        "(if archived then 0 else priority) == 3",
        "(if priority < 0 then 1 elseif priority == 3 then 2 else 3) == 2",
        "#'é' == 2",
        "priority / 2 == 1.5",
        "'Fix' .. ' auth' == description",
    ] {
        assert!(evaluate(expression), "{expression}");
    }
    for expression in [
        "false == 0",
        "nil ~= parent_id",
        "parent_id > 0",
        "not 0",
        "not ''",
        "false and true",
    ] {
        assert!(!evaluate(expression), "{expression}");
    }
}

#[test]
fn literals_decode_luau_strings_and_numbers() {
    for expression in [
        r#"'a\n\r\t\a\b\f\v' == '\097\010\013\009\007\008\012\011'"#,
        r#"'\x46ix\z   auth' == 'Fixauth'"#,
        r#"'\u{e9}' == 'é'"#,
        "[=[\nFix auth]=] == description",
        "'Fix\\\nauth' == 'Fix\\n' .. 'auth'",
        "0xA == 10 and 0b1_1 == 3 and 1_000 == 1000",
        "1e-1 == 0.1",
        "-- comment\n priority == 3 -- trailing comment",
    ] {
        assert!(evaluate(expression), "{expression}");
    }
    for expression in [
        r#"'\255' == description"#,
        "1e999 == 1",
        "9007199254740993 == id",
    ] {
        assert!(sql_filter::compile(expression).is_err(), "{expression}");
    }
}

#[test]
fn string_escape_edges_follow_luau_lexer() {
    for expression in [
        "[=[a\rb]=] == 'a\\rb'",
        "[=[\ra]=] == '\\ra'",
        "[=[\r\na\r\nb]=] == 'a\\nb'",
        r#"'\/' == '/' and '\q' == 'q'"#,
    ] {
        assert!(evaluate(expression), "{expression}");
    }
}

#[test]
fn rounded_integer_spellings_cannot_bypass_literal_bound() {
    for expression in [
        "9007199254740993e0 == id",
        "9007199254740993.0 == id",
        "9007199254740992.1 == id",
        "9.007199254740993e15 == id",
        "0x20000000000001p0 == id",
    ] {
        assert!(sql_filter::compile(expression).is_err(), "{expression}");
    }
    for expression in [
        "9007199254740992.0 == 9007199254740992",
        "9.007199254740992e15 == 9007199254740992",
        "0x20000000000000 == 9007199254740992",
    ] {
        assert!(evaluate(expression), "{expression}");
    }
}

#[test]
fn sqlite_functions_and_aliases_work() {
    for expression in [
        "like(task_name, '%auth%')",
        "like('a_b', 'a!_b', '!')",
        "glob(description, '*auth')",
        "lower('AUTH') == 'auth' and upper('auth') == 'AUTH'",
        "length('é') == 1 and substr(description, 5, 4) == 'auth'",
        "trim(' x ') == 'x' and ltrim('xxa', 'x') == 'a' and rtrim('axx', 'x') == 'a'",
        "replace(description, 'auth', 'DB') == 'Fix DB' and instr(description, 'auth') == 5",
        "abs(-3) == 3 and round(1.234, 2) == 1.23",
        "coalesce(parent_id, priority, 0) == 3 and nullif(priority, 3) == nil",
        "min(priority, 5) == 3 and max('a', 'b') == 'b'",
        "date(created_time) == '2026-10-02' and time(updated_time) == '00:00:00'",
        "datetime(created_at, '+1 day') == '2026-10-03 00:00:00'",
        "strftime('%Y', created_at) == '2026'",
        "julianday('1970-01-01') == 2440587.5 and unixepoch('1970-01-01') == 0",
        "coalesce(harness_name, '') == '' and not orchestrator_session",
    ] {
        assert!(evaluate(expression), "{expression}");
    }
}

#[test]
fn invalid_or_unsupported_expressions_fail_closed() {
    for expression in [
        "",
        "id = 1",
        "id ==",
        "return true",
        "true); return false --",
        "unknown == 1",
        "claim_key == nil",
        "context_only",
        "os.execute('echo bad')",
        "description:lower() == 'fix auth'",
        "{1,2}",
        "function() return true end",
        "id :: number",
        "`task {id}`",
        "priority % 2 == 1",
        "priority // 2 == 1",
        "2 ^ 3 == 8",
        "like(description)",
        "like(id, '%')",
        "lower(1) == '1'",
        "min(priority)",
        "coalesce(priority, description) == 0",
        "archived + 1 == 1",
        "priority > description",
        "true or 3",
        "if archived then 1 else 'text'",
    ] {
        let error = match sql_filter::compile(expression) {
            Ok(_) => panic!("accepted {expression}"),
            Err(error) => error.to_string(),
        };
        assert!(error.contains("--filter"), "{expression}: {error}");
    }
}

#[test]
fn compiler_bounds_apply_before_recursive_parsing() {
    for expression in [
        "(".repeat(10_000),
        format!("{}true{}", "(".repeat(100), ")".repeat(100)),
        format!("{}true", "not ".repeat(40)),
        format!("'{}' == description", "x".repeat(16_384)),
    ] {
        assert!(sql_filter::compile(&expression).is_err());
    }
    assert!(evaluate("(((priority == 3)))"));
}
