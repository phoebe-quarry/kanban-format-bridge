// Minimal JSON reader/writer. Trello exports are plain JSON and the project
// has no third-party dependencies, so this hand-rolled parser stands in for
// serde. It only needs to round-trip the shapes a board export actually uses:
// objects, arrays, strings, numbers, bools and null.

#[derive(Debug, Clone)]
pub enum Json {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<Json>),
    Object(Vec<(String, Json)>),
}

impl Json {
    pub fn parse(input: &str) -> Result<Json, String> {
        let chars: Vec<char> = input.chars().collect();
        let mut pos = 0;
        skip_ws(&chars, &mut pos);
        let value = parse_value(&chars, &mut pos)?;
        skip_ws(&chars, &mut pos);
        Ok(value)
    }

    pub fn get(&self, key: &str) -> Option<&Json> {
        match self {
            Json::Object(pairs) => pairs.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Json::String(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&[Json]> {
        match self {
            Json::Array(a) => Some(a),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Json::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub fn to_pretty_string(&self) -> String {
        let mut out = String::new();
        write_value(self, 0, &mut out);
        out
    }
}

fn peek(chars: &[char], pos: usize) -> Option<char> {
    chars.get(pos).copied()
}

fn skip_ws(chars: &[char], pos: &mut usize) {
    while let Some(c) = chars.get(*pos) {
        if c.is_whitespace() {
            *pos += 1;
        } else {
            break;
        }
    }
}

fn expect(chars: &[char], pos: &mut usize, ch: char) -> Result<(), String> {
    if chars.get(*pos) == Some(&ch) {
        *pos += 1;
        Ok(())
    } else {
        Err(format!("expected '{}' at position {}", ch, pos))
    }
}

fn matches_literal(chars: &[char], pos: usize, lit: &str) -> bool {
    let lit_chars: Vec<char> = lit.chars().collect();
    if pos + lit_chars.len() > chars.len() {
        return false;
    }
    chars[pos..pos + lit_chars.len()] == lit_chars[..]
}

fn parse_value(chars: &[char], pos: &mut usize) -> Result<Json, String> {
    skip_ws(chars, pos);
    match chars.get(*pos) {
        Some('{') => parse_object(chars, pos),
        Some('[') => parse_array(chars, pos),
        Some('"') => parse_string(chars, pos).map(Json::String),
        Some('t') | Some('f') => parse_bool(chars, pos),
        Some('n') => parse_null(chars, pos),
        Some(c) if *c == '-' || c.is_ascii_digit() => parse_number(chars, pos),
        Some(c) => Err(format!("unexpected character '{}' at position {}", c, pos)),
        None => Err("unexpected end of input".to_string()),
    }
}

fn parse_object(chars: &[char], pos: &mut usize) -> Result<Json, String> {
    expect(chars, pos, '{')?;
    let mut pairs = Vec::new();
    skip_ws(chars, pos);
    if peek(chars, *pos) == Some('}') {
        *pos += 1;
        return Ok(Json::Object(pairs));
    }
    loop {
        skip_ws(chars, pos);
        let key = parse_string(chars, pos)?;
        skip_ws(chars, pos);
        expect(chars, pos, ':')?;
        let value = parse_value(chars, pos)?;
        pairs.push((key, value));
        skip_ws(chars, pos);
        match chars.get(*pos) {
            Some(',') => {
                *pos += 1;
            }
            Some('}') => {
                *pos += 1;
                break;
            }
            _ => return Err(format!("expected ',' or '}}' at position {}", pos)),
        }
    }
    Ok(Json::Object(pairs))
}

fn parse_array(chars: &[char], pos: &mut usize) -> Result<Json, String> {
    expect(chars, pos, '[')?;
    let mut items = Vec::new();
    skip_ws(chars, pos);
    if peek(chars, *pos) == Some(']') {
        *pos += 1;
        return Ok(Json::Array(items));
    }
    loop {
        let value = parse_value(chars, pos)?;
        items.push(value);
        skip_ws(chars, pos);
        match chars.get(*pos) {
            Some(',') => {
                *pos += 1;
                skip_ws(chars, pos);
            }
            Some(']') => {
                *pos += 1;
                break;
            }
            _ => return Err(format!("expected ',' or ']' at position {}", pos)),
        }
    }
    Ok(Json::Array(items))
}

fn parse_string(chars: &[char], pos: &mut usize) -> Result<String, String> {
    expect(chars, pos, '"')?;
    let mut s = String::new();
    loop {
        match chars.get(*pos) {
            Some('"') => {
                *pos += 1;
                break;
            }
            Some('\\') => {
                *pos += 1;
                match chars.get(*pos) {
                    Some('"') => {
                        s.push('"');
                        *pos += 1;
                    }
                    Some('\\') => {
                        s.push('\\');
                        *pos += 1;
                    }
                    Some('/') => {
                        s.push('/');
                        *pos += 1;
                    }
                    Some('n') => {
                        s.push('\n');
                        *pos += 1;
                    }
                    Some('t') => {
                        s.push('\t');
                        *pos += 1;
                    }
                    Some('r') => {
                        s.push('\r');
                        *pos += 1;
                    }
                    Some('b') => {
                        s.push('\u{08}');
                        *pos += 1;
                    }
                    Some('f') => {
                        s.push('\u{0C}');
                        *pos += 1;
                    }
                    Some('u') => {
                        *pos += 1;
                        let code = parse_hex4(chars, pos)?;
                        if let Some(c) = char::from_u32(code) {
                            s.push(c);
                        }
                    }
                    _ => return Err(format!("invalid escape sequence at position {}", pos)),
                }
            }
            Some(c) => {
                s.push(*c);
                *pos += 1;
            }
            None => return Err("unterminated string".to_string()),
        }
    }
    Ok(s)
}

fn parse_hex4(chars: &[char], pos: &mut usize) -> Result<u32, String> {
    if *pos + 4 > chars.len() {
        return Err("truncated unicode escape".to_string());
    }
    let hex: String = chars[*pos..*pos + 4].iter().collect();
    *pos += 4;
    u32::from_str_radix(&hex, 16).map_err(|_| format!("invalid unicode escape '{}'", hex))
}

fn parse_number(chars: &[char], pos: &mut usize) -> Result<Json, String> {
    let start = *pos;
    if peek(chars, *pos) == Some('-') {
        *pos += 1;
    }
    while peek(chars, *pos).map(|c| c.is_ascii_digit()).unwrap_or(false) {
        *pos += 1;
    }
    if peek(chars, *pos) == Some('.') {
        *pos += 1;
        while peek(chars, *pos).map(|c| c.is_ascii_digit()).unwrap_or(false) {
            *pos += 1;
        }
    }
    if matches!(peek(chars, *pos), Some('e') | Some('E')) {
        *pos += 1;
        if matches!(peek(chars, *pos), Some('+') | Some('-')) {
            *pos += 1;
        }
        while peek(chars, *pos).map(|c| c.is_ascii_digit()).unwrap_or(false) {
            *pos += 1;
        }
    }
    let s: String = chars[start..*pos].iter().collect();
    s.parse::<f64>()
        .map(Json::Number)
        .map_err(|_| format!("invalid number '{}'", s))
}

fn parse_bool(chars: &[char], pos: &mut usize) -> Result<Json, String> {
    if matches_literal(chars, *pos, "true") {
        *pos += 4;
        Ok(Json::Bool(true))
    } else if matches_literal(chars, *pos, "false") {
        *pos += 5;
        Ok(Json::Bool(false))
    } else {
        Err(format!("invalid literal at position {}", pos))
    }
}

fn parse_null(chars: &[char], pos: &mut usize) -> Result<Json, String> {
    if matches_literal(chars, *pos, "null") {
        *pos += 4;
        Ok(Json::Null)
    } else {
        Err(format!("invalid literal at position {}", pos))
    }
}

fn write_value(value: &Json, indent: usize, out: &mut String) {
    match value {
        Json::Null => out.push_str("null"),
        Json::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Json::Number(n) => out.push_str(&format_number(*n)),
        Json::String(s) => write_string(s, out),
        Json::Array(items) => {
            if items.is_empty() {
                out.push_str("[]");
                return;
            }
            out.push_str("[\n");
            for (i, item) in items.iter().enumerate() {
                push_indent(out, indent + 1);
                write_value(item, indent + 1, out);
                if i + 1 < items.len() {
                    out.push(',');
                }
                out.push('\n');
            }
            push_indent(out, indent);
            out.push(']');
        }
        Json::Object(pairs) => {
            if pairs.is_empty() {
                out.push_str("{}");
                return;
            }
            out.push_str("{\n");
            for (i, (key, val)) in pairs.iter().enumerate() {
                push_indent(out, indent + 1);
                write_string(key, out);
                out.push_str(": ");
                write_value(val, indent + 1, out);
                if i + 1 < pairs.len() {
                    out.push(',');
                }
                out.push('\n');
            }
            push_indent(out, indent);
            out.push('}');
        }
    }
}

fn write_string(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}

fn push_indent(out: &mut String, level: usize) {
    for _ in 0..level {
        out.push_str("  ");
    }
}

fn format_number(n: f64) -> String {
    if n.is_finite() && n.fract() == 0.0 && n.abs() < 1e15 {
        format!("{}", n as i64)
    } else {
        format!("{}", n)
    }
}
