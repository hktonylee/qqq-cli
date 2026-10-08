mod functions;
mod literals;

use anyhow::{Context, Result, bail, ensure};
use full_moon::{
    ast::{BinOp, Call, Expression, FunctionArgs, LastStmt, LuaVersion, Prefix, Suffix, UnOp, Var},
    tokenizer::{Lexer, LexerResult, Symbol, TokenType},
};
use rusqlite::types::Value;

pub struct CompiledFilter {
    sql: String,
    params: Vec<Value>,
}
impl CompiledFilter {
    pub fn sql(&self) -> &str {
        &self.sql
    }
    pub fn params(&self) -> &[Value] {
        &self.params
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Boolean,
    Number,
    Text,
    Nil,
}
struct Expr {
    sql: String,
    kind: Kind,
}

fn compatible(left: Kind, right: Kind) -> Result<Kind> {
    ensure!(
        left == right || left == Kind::Nil || right == Kind::Nil,
        "incompatible value types {left:?} and {right:?}"
    );
    Ok(if left == Kind::Nil { right } else { left })
}
fn truthy(value: &Expr) -> String {
    match value.kind {
        Kind::Boolean => format!("COALESCE(({}),0)", value.sql),
        Kind::Nil | Kind::Number | Kind::Text => format!("({}) IS NOT NULL", value.sql),
    }
}
fn require(value: &Expr, kind: Kind) -> Result<()> {
    ensure!(
        value.kind == kind || value.kind == Kind::Nil,
        "expected {kind:?}, got {:?}",
        value.kind
    );
    Ok(())
}

/// Compile one expression; all literal values become bound SQLite parameters.
pub fn compile(source: &str) -> Result<CompiledFilter> {
    compile_inner(source)
        .map_err(|error| anyhow::anyhow!("Invalid --filter: {error:#}. See docs/filter.md"))
}
fn compile_inner(source: &str) -> Result<CompiledFilter> {
    ensure!(!source.trim().is_empty(), "expression is empty");
    ensure!(source.len() <= 16_384, "expression exceeds 16 KiB");
    // Bound recursive parser work first; comments are excluded, strings count once.
    let tokens = match Lexer::new(source, LuaVersion::luau()).collect() {
        LexerResult::Ok(tokens) => tokens,
        LexerResult::Fatal(errors) | LexerResult::Recovered(_, errors) => {
            bail!(
                "invalid Luau syntax: {}",
                errors.first().context("lexer failed")?
            )
        }
    };
    ensure!(
        tokens
            .iter()
            .filter(|token| !token.token_type().is_trivia()
                && !matches!(token.token_type(), TokenType::Eof))
            .count()
            <= 128,
        "expression exceeds 128 tokens"
    );
    // Newlines prevent a trailing comment from consuming our closing delimiter.
    let wrapped = format!("return (\n{source}\n)");
    let ast = full_moon::parse_fallible(&wrapped, LuaVersion::luau())
        .into_result()
        .map_err(|errors| {
            anyhow::anyhow!(
                "invalid Luau syntax: {}",
                errors.first().map(ToString::to_string).unwrap_or_default()
            )
        })?;
    ensure!(
        ast.nodes().stmts().next().is_none(),
        "only one expression is allowed"
    );
    let Some(LastStmt::Return(returned)) = ast.nodes().last_stmt() else {
        bail!("only one expression is allowed")
    };
    let mut expressions = returned.returns().iter();
    let expression = expressions.next().context("expression is missing")?;
    ensure!(
        expressions.next().is_none(),
        "only one expression is allowed"
    );
    let mut compiler = Compiler {
        params: Vec::new(),
        nodes: 0,
    };
    let result = compiler.expression(expression, 0)?;
    let sql = truthy(&result);
    ensure!(
        sql.len() <= 65_536,
        "compiled expression exceeds 64 KiB SQL"
    );
    Ok(CompiledFilter {
        sql,
        params: compiler.params,
    })
}

struct Compiler {
    params: Vec<Value>,
    nodes: usize,
}
impl Compiler {
    fn literal(&mut self, value: Value, kind: Kind) -> Expr {
        self.params.push(value);
        Expr {
            sql: format!("?{}", self.params.len()),
            kind,
        }
    }
    fn has_tag(&mut self, args: Vec<Expr>) -> Result<Expr> {
        ensure!(
            args.len() == 1,
            "has_tag expects exactly one string literal"
        );
        let arg = &args[0];
        ensure!(arg.kind == Kind::Text, "has_tag requires a string literal");
        // Only a literal (including parentheses) compiles to one bound parameter.
        let index = arg
            .sql
            .strip_prefix('?')
            .and_then(|value| value.parse::<usize>().ok());
        let label = index
            .and_then(|index| index.checked_sub(1))
            .and_then(|index| self.params.get_mut(index));
        let Some(Value::Text(label)) = label else {
            bail!("has_tag requires a string literal");
        };
        *label = crate::tags::normalize(std::slice::from_ref(label))?.remove(0);
        Ok(Expr {
            sql: functions::tag_sql(&arg.sql),
            kind: Kind::Boolean,
        })
    }
    fn expression(&mut self, expression: &Expression, depth: usize) -> Result<Expr> {
        ensure!(depth <= 32, "expression exceeds 32 AST levels");
        self.nodes += 1;
        ensure!(self.nodes <= 128, "expression exceeds 128 AST nodes");
        let next = depth + 1;
        let result = match expression {
            Expression::Parentheses { expression, .. } => self.expression(expression, next)?,
            Expression::Number(token) => {
                let TokenType::Number { text } = token.token_type() else {
                    bail!("invalid number")
                };
                self.literal(literals::number(text.as_str())?, Kind::Number)
            }
            Expression::String(token) => {
                let TokenType::StringLiteral {
                    literal,
                    quote_type,
                    ..
                } = token.token_type()
                else {
                    bail!("invalid string")
                };
                self.literal(
                    Value::Text(literals::string(literal.as_str(), *quote_type)?),
                    Kind::Text,
                )
            }
            Expression::Symbol(token) => match token.token_type() {
                TokenType::Symbol {
                    symbol: Symbol::True,
                } => self.literal(Value::Integer(1), Kind::Boolean),
                TokenType::Symbol {
                    symbol: Symbol::False,
                } => self.literal(Value::Integer(0), Kind::Boolean),
                TokenType::Symbol {
                    symbol: Symbol::Nil,
                } => self.literal(Value::Null, Kind::Nil),
                _ => bail!("unsupported literal"),
            },
            Expression::Var(Var::Name(token)) => {
                let TokenType::Identifier { identifier } = token.token_type() else {
                    bail!("invalid variable")
                };
                variable(identifier.as_str())?
            }
            Expression::UnaryOperator { unop, expression } => {
                let value = self.expression(expression, next)?;
                match unop {
                    UnOp::Not(_) => Expr {
                        sql: format!("NOT ({})", truthy(&value)),
                        kind: Kind::Boolean,
                    },
                    UnOp::Minus(_) => {
                        require(&value, Kind::Number)?;
                        Expr {
                            sql: format!("-({})", value.sql),
                            kind: Kind::Number,
                        }
                    }
                    UnOp::Hash(_) => {
                        require(&value, Kind::Text)?;
                        Expr {
                            sql: format!("length(CAST(({}) AS BLOB))", value.sql),
                            kind: Kind::Number,
                        }
                    }
                    _ => bail!("unsupported unary operator"),
                }
            }
            Expression::BinaryOperator { lhs, binop, rhs } => {
                let left = self.expression(lhs, next)?;
                let right = self.expression(rhs, next)?;
                binary(left, binop, right)?
            }
            Expression::FunctionCall(call) => {
                let Prefix::Name(token) = call.prefix() else {
                    bail!("only named SQLite functions are allowed")
                };
                let TokenType::Identifier { identifier } = token.token_type() else {
                    bail!("invalid function name")
                };
                let mut suffixes = call.suffixes();
                let Some(Suffix::Call(Call::AnonymousCall(args))) = suffixes.next() else {
                    bail!("method/member calls are unavailable")
                };
                ensure!(
                    suffixes.next().is_none(),
                    "chained/member calls are unavailable"
                );
                let FunctionArgs::Parentheses { arguments, .. } = args.as_ref() else {
                    bail!("functions require parenthesized arguments")
                };
                let args = arguments
                    .iter()
                    .map(|arg| self.expression(arg, next))
                    .collect::<Result<Vec<_>>>()?;
                if identifier.as_str() == "has_tag" {
                    self.has_tag(args)?
                } else {
                    functions::compile(identifier.as_str(), args)?
                }
            }
            Expression::IfExpression(value) => {
                ensure!(value.binding().is_none(), "if bindings are unavailable");
                let condition = self.expression(value.condition(), next)?;
                let then_value = self.expression(value.if_expression(), next)?;
                let mut kind = then_value.kind;
                let mut sql = format!(
                    "CASE WHEN ({}) THEN ({})",
                    truthy(&condition),
                    then_value.sql
                );
                for branch in value.else_if_expressions().into_iter().flatten() {
                    ensure!(branch.binding().is_none(), "if bindings are unavailable");
                    let condition = self.expression(branch.condition(), next)?;
                    let value = self.expression(branch.expression(), next)?;
                    kind = compatible(kind, value.kind)?;
                    sql.push_str(&format!(
                        " WHEN ({}) THEN ({})",
                        truthy(&condition),
                        value.sql
                    ));
                }
                let else_value = self.expression(value.else_expression(), next)?;
                kind = compatible(kind, else_value.kind)?;
                sql.push_str(&format!(" ELSE ({}) END", else_value.sql));
                Expr { sql, kind }
            }
            _ => bail!(
                "unsupported Luau expression; use documented task variables, operators, and functions"
            ),
        };
        ensure!(
            result.sql.len() <= 65_536,
            "compiled expression exceeds 64 KiB SQL"
        );
        Ok(result)
    }
}

fn variable(name: &str) -> Result<Expr> {
    let (column, kind) = match name {
        "id" => ("id", Kind::Number),
        "priority" => ("priority", Kind::Number),
        "parent_id" => ("parent_id", Kind::Number),
        "archived" => ("archived", Kind::Boolean),
        "description" | "task_name" => ("description", Kind::Text),
        "created_at" | "created_time" => ("created_at", Kind::Text),
        "updated_at" | "updated_time" => ("updated_at", Kind::Text),
        "status" => ("status", Kind::Text),
        "harness_name" => ("harness_name", Kind::Text),
        "harness_session" => ("harness_session", Kind::Text),
        "orchestrator_name" => ("orchestrator_name", Kind::Text),
        "orchestrator_session" => ("orchestrator_session", Kind::Text),
        _ => bail!("unknown variable `{name}`; see docs/filter-variables.md"),
    };
    Ok(Expr {
        sql: format!("tasks.{column}"),
        kind,
    })
}

fn binary(left: Expr, operator: &BinOp, right: Expr) -> Result<Expr> {
    use BinOp::*;
    let sql = match operator {
        And(_) | Or(_) => {
            let kind = compatible(left.kind, right.kind)?;
            let is_and = matches!(operator, And(_));
            let sql = if is_and {
                format!(
                    "CASE WHEN ({}) THEN ({}) ELSE ({}) END",
                    truthy(&left),
                    right.sql,
                    left.sql
                )
            } else {
                format!(
                    "CASE WHEN ({}) THEN ({}) ELSE ({}) END",
                    truthy(&left),
                    left.sql,
                    right.sql
                )
            };
            return Ok(Expr { sql, kind });
        }
        TwoEqual(_) | TildeEqual(_) => {
            if left.kind != right.kind && left.kind != Kind::Nil && right.kind != Kind::Nil {
                let both_nil = format!("(({}) IS NULL AND ({}) IS NULL)", left.sql, right.sql);
                if matches!(operator, TwoEqual(_)) {
                    both_nil
                } else {
                    format!("NOT {both_nil}")
                }
            } else {
                format!(
                    "({}) {} ({})",
                    left.sql,
                    if matches!(operator, TwoEqual(_)) {
                        "IS"
                    } else {
                        "IS NOT"
                    },
                    right.sql
                )
            }
        }
        LessThan(_) | LessThanEqual(_) | GreaterThan(_) | GreaterThanEqual(_) => {
            let kind = compatible(left.kind, right.kind)?;
            ensure!(
                matches!(kind, Kind::Number | Kind::Text | Kind::Nil),
                "ordering requires numbers or strings"
            );
            let op = match operator {
                LessThan(_) => "<",
                LessThanEqual(_) => "<=",
                GreaterThan(_) => ">",
                _ => ">=",
            };
            format!("COALESCE((({}) {op} ({})),0)", left.sql, right.sql)
        }
        Plus(_) | Minus(_) | Star(_) | Slash(_) => {
            require(&left, Kind::Number)?;
            require(&right, Kind::Number)?;
            let op = match operator {
                Plus(_) => "+",
                Minus(_) => "-",
                Star(_) => "*",
                _ => "/",
            };
            let lhs = if matches!(operator, Slash(_)) {
                format!("CAST(({}) AS REAL)", left.sql)
            } else {
                format!("({})", left.sql)
            };
            return Ok(Expr {
                sql: format!("({lhs} {op} ({}))", right.sql),
                kind: Kind::Number,
            });
        }
        TwoDots(_) => {
            require(&left, Kind::Text)?;
            require(&right, Kind::Text)?;
            return Ok(Expr {
                sql: format!("(({}) || ({}))", left.sql, right.sql),
                kind: Kind::Text,
            });
        }
        _ => bail!("unsupported binary operator; see docs/filter.md"),
    };
    Ok(Expr {
        sql,
        kind: Kind::Boolean,
    })
}
