use anyhow::{Context, Result, bail, ensure};
use full_moon::tokenizer::StringLiteralQuoteType;
use rusqlite::types::Value;

pub(super) fn number(source: &str) -> Result<Value> {
    let source = source.replace('_', "");
    let integer = if source.starts_with("0b") || source.starts_with("0B") {
        Some(u64::from_str_radix(&source[2..], 2)?)
    } else if (source.starts_with("0x") || source.starts_with("0X"))
        && !source.contains(['.', 'p', 'P'])
    {
        Some(u64::from_str_radix(&source[2..], 16)?)
    } else if source.bytes().all(|byte| byte.is_ascii_digit()) {
        Some(source.parse::<u64>()?)
    } else {
        None
    };
    if let Some(value) = integer {
        ensure!(
            value <= 9_007_199_254_740_992,
            "integer exceeds Luau's exact range (2^53)"
        );
        return Ok(Value::Integer(value as i64));
    }
    let value = source.parse::<f64>()?;
    ensure!(value.is_finite(), "number must be finite");
    ensure!(
        value.abs() <= 9_007_199_254_740_992.0
            && (value != 9_007_199_254_740_992.0 || !above_exact_range(&source)?),
        "integer exceeds Luau's exact range (2^53)"
    );
    Ok(Value::Real(value))
}

// At 2^53, f64 rounding can hide an out-of-range literal. Compare source digits
// at that boundary instead of trusting the already-rounded floating value.
fn above_exact_range(source: &str) -> Result<bool> {
    let (mantissa, exponent) = source.split_once(['e', 'E']).unwrap_or((source, "0"));
    let exponent = exponent.parse::<i32>()?;
    let fractional = mantissa
        .split_once('.')
        .map_or(0, |(_, digits)| digits.len());
    let digits = mantissa.replace('.', "");
    let digits = digits.trim_start_matches('0');
    let integer_digits = digits.len() as i64 - fractional as i64 + i64::from(exponent);
    if integer_digits != 16 {
        return Ok(integer_digits > 16);
    }
    let mut integer = digits.chars().take(16).collect::<String>();
    while integer.len() < 16 {
        integer.push('0');
    }
    Ok(integer.as_str() > "9007199254740992"
        || (integer == "9007199254740992"
            && digits
                .get(16..)
                .is_some_and(|rest| rest.bytes().any(|digit| digit != b'0'))))
}

pub(super) fn string(source: &str, quote: StringLiteralQuoteType) -> Result<String> {
    if quote == StringLiteralQuoteType::Brackets {
        let normalized = source.replace("\r\n", "\n");
        return Ok(normalized
            .strip_prefix('\n')
            .unwrap_or(&normalized)
            .to_owned());
    }
    let mut output = Vec::new();
    let mut chars = source.chars().peekable();
    while let Some(character) = chars.next() {
        if character != '\\' {
            let mut buffer = [0; 4];
            output.extend_from_slice(character.encode_utf8(&mut buffer).as_bytes());
            continue;
        }
        let escape = chars.next().context("unfinished string escape")?;
        match escape {
            'a' => output.push(7),
            'b' => output.push(8),
            'f' => output.push(12),
            'n' => output.push(10),
            'r' => output.push(13),
            't' => output.push(9),
            'v' => output.push(11),
            '\\' => output.push(b'\\'),
            '\'' => output.push(b'\''),
            '"' => output.push(b'"'),
            '\n' | '\r' => {
                if escape == '\r' && chars.peek() == Some(&'\n') {
                    chars.next();
                }
                output.push(b'\n');
            }
            'z' => {
                while chars.peek().is_some_and(char::is_ascii_whitespace) {
                    chars.next();
                }
            }
            'x' => {
                let mut value = 0;
                for _ in 0..2 {
                    value = value * 16
                        + chars
                            .next()
                            .and_then(|c| c.to_digit(16))
                            .context("\\x needs two hex digits")?;
                }
                output.push(value as u8);
            }
            'u' => {
                ensure!(chars.next() == Some('{'), "\\u needs braces");
                let mut digits = String::new();
                loop {
                    match chars.next() {
                        Some('}') => break,
                        Some(c) if c.is_ascii_hexdigit() => digits.push(c),
                        _ => bail!("invalid Unicode escape"),
                    }
                }
                let value = u32::from_str_radix(&digits, 16)?;
                let character = char::from_u32(value).context("invalid Unicode scalar")?;
                let mut buffer = [0; 4];
                output.extend_from_slice(character.encode_utf8(&mut buffer).as_bytes());
            }
            c if c.is_ascii_digit() => {
                let mut value = c.to_digit(10).unwrap();
                for _ in 0..2 {
                    if let Some(c) = chars.peek().copied().filter(char::is_ascii_digit) {
                        chars.next();
                        value = value * 10 + c.to_digit(10).unwrap();
                    } else {
                        break;
                    }
                }
                ensure!(value <= 255, "decimal string escape exceeds 255");
                output.push(value as u8);
            }
            _ => {
                let mut buffer = [0; 4];
                output.extend_from_slice(escape.encode_utf8(&mut buffer).as_bytes());
            }
        }
    }
    String::from_utf8(output).context("SQLite text requires UTF-8 string literals")
}
