use super::{Expr, Kind, compatible};
use anyhow::{Result, bail, ensure};

pub(super) fn tag_sql(parameter: &str) -> String {
    format!(
        "EXISTS (SELECT 1 FROM json_each(tasks.tags) AS task_tag
         WHERE task_tag.value COLLATE BINARY = {parameter})"
    )
}

fn arity(name: &str, args: &[Expr], min: usize, max: usize) -> Result<()> {
    ensure!(
        (min..=max).contains(&args.len()),
        "{name} expects {min}..={max} arguments, got {}",
        args.len()
    );
    Ok(())
}
fn require(name: &str, args: &[Expr], kind: Kind) -> Result<()> {
    ensure!(
        args.iter()
            .all(|arg| arg.kind == kind || arg.kind == Kind::Nil),
        "{name} requires {kind:?} arguments"
    );
    Ok(())
}

pub(super) fn compile(name: &str, args: Vec<Expr>) -> Result<Expr> {
    use Kind::{Boolean, Number, Text};
    let kind = match name {
        "like" | "glob" => {
            arity(name, &args, 2, if name == "like" { 3 } else { 2 })?;
            require(name, &args, Text)?;
            let operator = if name == "like" { "LIKE" } else { "GLOB" };
            let escape = args
                .get(2)
                .map(|arg| format!(" ESCAPE ({})", arg.sql))
                .unwrap_or_default();
            return Ok(Expr {
                sql: format!(
                    "COALESCE((({}) {operator} ({}){escape}),0)",
                    args[0].sql, args[1].sql
                ),
                kind: Boolean,
            });
        }
        "lower" | "upper" | "length" => {
            arity(name, &args, 1, 1)?;
            require(name, &args, Text)?;
            if name == "length" { Number } else { Text }
        }
        "trim" | "ltrim" | "rtrim" => {
            arity(name, &args, 1, 2)?;
            require(name, &args, Text)?;
            Text
        }
        "replace" => {
            arity(name, &args, 3, 3)?;
            require(name, &args, Text)?;
            Text
        }
        "instr" => {
            arity(name, &args, 2, 2)?;
            require(name, &args, Text)?;
            Number
        }
        "substr" => {
            arity(name, &args, 2, 3)?;
            require(name, &args[..1], Text)?;
            require(name, &args[1..], Number)?;
            Text
        }
        "abs" | "round" => {
            arity(name, &args, 1, if name == "abs" { 1 } else { 2 })?;
            require(name, &args, Number)?;
            Number
        }
        "coalesce" | "nullif" | "min" | "max" => {
            arity(name, &args, 2, if name == "nullif" { 2 } else { 127 })?;
            let mut kind = Kind::Nil;
            for arg in &args {
                kind = compatible(kind, arg.kind)?;
            }
            if matches!(name, "min" | "max") {
                ensure!(kind != Boolean, "{name} requires numbers or strings");
            }
            if name == "nullif" { args[0].kind } else { kind }
        }
        "date" | "time" | "datetime" | "julianday" | "unixepoch" | "strftime" => {
            let start = usize::from(name == "strftime");
            arity(name, &args, start, 127)?;
            if start == 1 {
                require(name, &args[..1], Text)?;
            }
            if let Some(time) = args.get(start) {
                ensure!(
                    matches!(time.kind, Text | Number | Kind::Nil),
                    "{name} time value requires text or number"
                );
            }
            if args.len() > start + 1 {
                require(name, &args[start + 1..], Text)?;
            }
            if matches!(name, "julianday" | "unixepoch") {
                Number
            } else {
                Text
            }
        }
        _ => bail!("unknown function `{name}`; see docs/filter-functions.md"),
    };
    // Every accepted name above is a literal allowlist entry, never caller SQL.
    let sql = format!(
        "{name}({})",
        args.iter()
            .map(|arg| arg.sql.as_str())
            .collect::<Vec<_>>()
            .join(",")
    );
    Ok(Expr { sql, kind })
}
