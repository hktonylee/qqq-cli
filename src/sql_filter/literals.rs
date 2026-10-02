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
    let value = if source.starts_with("0x") || source.starts_with("0X") {
        let mut parts = source[2..].split(['p', 'P']);
        let mantissa = parts.next().context("missing hex mantissa")?;
        let exponent = parts
            .next()
            .map(str::parse::<i32>)
            .transpose()?
            .unwrap_or(0);
        ensure!(parts.next().is_none(), "invalid hexadecimal number");
        let mut value = 0.0;
        let mut scale = 1.0;
        let mut fraction = false;
        for byte in mantissa.bytes() {
            if byte == b'.' {
                ensure!(!fraction, "invalid hexadecimal number");
                fraction = true;
                continue;
            }
            let digit = char::from(byte)
                .to_digit(16)
                .context("invalid hexadecimal digit")?;
            if fraction {
                scale /= 16.0;
                value += f64::from(digit) * scale;
            } else {
                value = value * 16.0 + f64::from(digit);
            }
        }
        value * 2_f64.powi(exponent)
    } else {
        source.parse::<f64>()?
    };
    ensure!(value.is_finite(), "number must be finite");
    ensure!(
        value.fract() != 0.0 || value.abs() <= 9_007_199_254_740_992.0,
        "integer exceeds Luau's exact range (2^53)"
    );
    Ok(Value::Real(value))
}

pub(super) fn string(source: &str, quote: StringLiteralQuoteType) -> Result<String> {
    if quote == StringLiteralQuoteType::Brackets {
        let normalized = source.replace("\r\n", "\n").replace('\r', "\n");
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
            _ => bail!("unsupported string escape \\{escape}"),
        }
    }
    String::from_utf8(output).context("SQLite text requires UTF-8 string literals")
}
