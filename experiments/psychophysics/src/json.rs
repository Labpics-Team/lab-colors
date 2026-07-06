//! Минимальный, зависимость-нулевой JSON (RFC 8259) — ровно то, что нужно харнессу.
//!
//! Зачем свой, а не `serde_json`: крейт держит правило нулевых зависимостей репо
//! (issue #29), а `cargo audit --deny warnings` в CI не должен получить новую
//! supply-chain-поверхность ради чтения паспорта и экспорта результатов. Парсер —
//! рекурсивный спуск, сериализатор сохраняет порядок ключей (детерминированный
//! вывод для воспроизводимых манифестов).
//!
//! Поддержано: объекты, массивы, строки (со всеми escape и `\uXXXX`), числа
//! (целые/дробные/экспонента/знак), `true`/`false`/`null`. Достаточно и для
//! замороженного паспорта labui, и для сырого экспорта раннера.

use std::fmt::Write as _;

/// Значение JSON. Объект хранит ключи в порядке вставки (детерминизм вывода).
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<Value>),
    /// Пары ключ-значение в порядке вставки.
    Object(Vec<(String, Value)>),
}

impl Value {
    /// Ссылка на значение по ключу объекта (иначе `None`).
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Value::Object(entries) => entries.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    /// Число как `f64` (иначе `None`).
    #[must_use]
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Number(n) => Some(*n),
            _ => None,
        }
    }

    /// Строка (иначе `None`).
    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::String(s) => Some(s.as_str()),
            _ => None,
        }
    }

    /// Булево (иначе `None`).
    #[must_use]
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            _ => None,
        }
    }

    /// Элементы массива (иначе `None`).
    #[must_use]
    pub fn as_array(&self) -> Option<&[Value]> {
        match self {
            Value::Array(items) => Some(items.as_slice()),
            _ => None,
        }
    }

    /// Компактная сериализация в одну строку.
    #[must_use]
    pub fn to_compact(&self) -> String {
        let mut out = String::new();
        write_value(&mut out, self, None, 0);
        out
    }

    /// Сериализация с отступами (2 пробела) — читаемый манифест/результат.
    #[must_use]
    pub fn to_pretty(&self) -> String {
        let mut out = String::new();
        write_value(&mut out, self, Some(2), 0);
        out
    }
}

/// Удобный конструктор объекта из упорядоченных пар.
#[must_use]
pub fn obj(entries: Vec<(&str, Value)>) -> Value {
    Value::Object(
        entries
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect(),
    )
}

/// Разобрать текст JSON. Ошибка — человекочитаемая позиция и причина.
///
/// # Errors
/// Возвращает `Err` при синтаксической ошибке или лишних символах после значения.
pub fn parse(input: &str) -> Result<Value, String> {
    let chars: Vec<char> = input.chars().collect();
    let mut p = Parser {
        chars: &chars,
        pos: 0,
    };
    p.skip_ws();
    let v = p.parse_value()?;
    p.skip_ws();
    if p.pos != p.chars.len() {
        return Err(format!(
            "лишние символы после значения на позиции {}",
            p.pos
        ));
    }
    Ok(v)
}

struct Parser<'a> {
    chars: &'a [char],
    pos: usize,
}

impl Parser<'_> {
    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.chars.get(self.pos).copied();
        if c.is_some() {
            self.pos += 1;
        }
        c
    }

    fn skip_ws(&mut self) {
        while let Some(c) = self.peek() {
            if c == ' ' || c == '\t' || c == '\n' || c == '\r' {
                self.pos += 1;
            } else {
                break;
            }
        }
    }

    fn parse_value(&mut self) -> Result<Value, String> {
        self.skip_ws();
        match self.peek() {
            Some('{') => self.parse_object(),
            Some('[') => self.parse_array(),
            Some('"') => Ok(Value::String(self.parse_string()?)),
            Some('t') | Some('f') => self.parse_bool(),
            Some('n') => self.parse_null(),
            Some(c) if c == '-' || c.is_ascii_digit() => self.parse_number(),
            Some(c) => Err(format!("неожиданный символ '{c}' на позиции {}", self.pos)),
            None => Err("неожиданный конец ввода".to_string()),
        }
    }

    fn expect(&mut self, want: char) -> Result<(), String> {
        match self.bump() {
            Some(c) if c == want => Ok(()),
            Some(c) => Err(format!(
                "ожидался '{want}', встречен '{c}' на позиции {}",
                self.pos - 1
            )),
            None => Err(format!("ожидался '{want}', встречен конец ввода")),
        }
    }

    fn parse_object(&mut self) -> Result<Value, String> {
        self.expect('{')?;
        let mut entries: Vec<(String, Value)> = Vec::new();
        self.skip_ws();
        if self.peek() == Some('}') {
            self.pos += 1;
            return Ok(Value::Object(entries));
        }
        loop {
            self.skip_ws();
            let key = self.parse_string()?;
            self.skip_ws();
            self.expect(':')?;
            let val = self.parse_value()?;
            entries.push((key, val));
            self.skip_ws();
            match self.bump() {
                Some(',') => continue,
                Some('}') => break,
                other => {
                    return Err(format!(
                        "ожидался ',' или '}}', встречено {other:?} на позиции {}",
                        self.pos - 1
                    ));
                }
            }
        }
        Ok(Value::Object(entries))
    }

    fn parse_array(&mut self) -> Result<Value, String> {
        self.expect('[')?;
        let mut items = Vec::new();
        self.skip_ws();
        if self.peek() == Some(']') {
            self.pos += 1;
            return Ok(Value::Array(items));
        }
        loop {
            let val = self.parse_value()?;
            items.push(val);
            self.skip_ws();
            match self.bump() {
                Some(',') => continue,
                Some(']') => break,
                other => {
                    return Err(format!(
                        "ожидался ',' или ']', встречено {other:?} на позиции {}",
                        self.pos - 1
                    ));
                }
            }
        }
        Ok(Value::Array(items))
    }

    fn parse_string(&mut self) -> Result<String, String> {
        self.expect('"')?;
        let mut s = String::new();
        loop {
            match self.bump() {
                None => return Err("незакрытая строка".to_string()),
                Some('"') => break,
                Some('\\') => {
                    let esc = self.bump().ok_or("оборванный escape")?;
                    match esc {
                        '"' => s.push('"'),
                        '\\' => s.push('\\'),
                        '/' => s.push('/'),
                        'n' => s.push('\n'),
                        't' => s.push('\t'),
                        'r' => s.push('\r'),
                        'b' => s.push('\u{0008}'),
                        'f' => s.push('\u{000C}'),
                        'u' => {
                            let cp = self.parse_hex4()?;
                            // Суррогатная пара UTF-16.
                            if (0xD800..=0xDBFF).contains(&cp) {
                                if self.bump() != Some('\\') || self.bump() != Some('u') {
                                    return Err("ожидалась низкая суррогатная половина".to_string());
                                }
                                let lo = self.parse_hex4()?;
                                if !(0xDC00..=0xDFFF).contains(&lo) {
                                    return Err(
                                        "некорректная низкая суррогатная половина".to_string()
                                    );
                                }
                                let c = 0x1_0000 + ((cp - 0xD800) << 10) + (lo - 0xDC00);
                                s.push(char::from_u32(c).ok_or("некорректный код-пойнт")?);
                            } else {
                                s.push(char::from_u32(cp).ok_or("некорректный код-пойнт")?);
                            }
                        }
                        other => return Err(format!("неизвестный escape '\\{other}'")),
                    }
                }
                Some(c) => s.push(c),
            }
        }
        Ok(s)
    }

    fn parse_hex4(&mut self) -> Result<u32, String> {
        let mut v = 0u32;
        for _ in 0..4 {
            let c = self.bump().ok_or("оборванный \\u")?;
            let d = c
                .to_digit(16)
                .ok_or_else(|| format!("не hex-цифра '{c}' в \\u"))?;
            v = v * 16 + d;
        }
        Ok(v)
    }

    fn parse_bool(&mut self) -> Result<Value, String> {
        if self.starts_with("true") {
            self.pos += 4;
            Ok(Value::Bool(true))
        } else if self.starts_with("false") {
            self.pos += 5;
            Ok(Value::Bool(false))
        } else {
            Err(format!("ожидался литерал bool на позиции {}", self.pos))
        }
    }

    fn parse_null(&mut self) -> Result<Value, String> {
        if self.starts_with("null") {
            self.pos += 4;
            Ok(Value::Null)
        } else {
            Err(format!("ожидался null на позиции {}", self.pos))
        }
    }

    fn starts_with(&self, lit: &str) -> bool {
        let lit: Vec<char> = lit.chars().collect();
        if self.pos + lit.len() > self.chars.len() {
            return false;
        }
        self.chars[self.pos..self.pos + lit.len()] == lit[..]
    }

    fn parse_number(&mut self) -> Result<Value, String> {
        let start = self.pos;
        if self.peek() == Some('-') {
            self.pos += 1;
        }
        while let Some(c) = self.peek() {
            if c.is_ascii_digit() || c == '.' || c == 'e' || c == 'E' || c == '+' || c == '-' {
                self.pos += 1;
            } else {
                break;
            }
        }
        let text: String = self.chars[start..self.pos].iter().collect();
        text.parse::<f64>()
            .map(Value::Number)
            .map_err(|e| format!("некорректное число '{text}': {e}"))
    }
}

fn write_value(out: &mut String, v: &Value, indent: Option<usize>, depth: usize) {
    match v {
        Value::Null => out.push_str("null"),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Number(n) => write_number(out, *n),
        Value::String(s) => write_string(out, s),
        Value::Array(items) => write_array(out, items, indent, depth),
        Value::Object(entries) => write_object(out, entries, indent, depth),
    }
}

fn write_number(out: &mut String, n: f64) {
    if n.is_finite() {
        // `{}` для f64 — кратчайшее round-trip-представление (Ryū): 0.3 → "0.3",
        // 1.0 → "1". Даёт чистый детерминированный JSON без плавучего мусора.
        let _ = write!(out, "{n}");
    } else {
        // JSON не имеет NaN/Inf; в харнессе такие значения не возникают, но на
        // всякий случай пишем null, чтобы вывод оставался валидным JSON.
        out.push_str("null");
    }
}

fn write_string(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            '\u{0008}' => out.push_str("\\b"),
            '\u{000C}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

fn write_array(out: &mut String, items: &[Value], indent: Option<usize>, depth: usize) {
    if items.is_empty() {
        out.push_str("[]");
        return;
    }
    out.push('[');
    for (i, item) in items.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        newline_indent(out, indent, depth + 1);
        write_value(out, item, indent, depth + 1);
    }
    newline_indent(out, indent, depth);
    out.push(']');
}

fn write_object(
    out: &mut String,
    entries: &[(String, Value)],
    indent: Option<usize>,
    depth: usize,
) {
    if entries.is_empty() {
        out.push_str("{}");
        return;
    }
    out.push('{');
    for (i, (k, val)) in entries.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        newline_indent(out, indent, depth + 1);
        write_string(out, k);
        out.push(':');
        if indent.is_some() {
            out.push(' ');
        }
        write_value(out, val, indent, depth + 1);
    }
    newline_indent(out, indent, depth);
    out.push('}');
}

fn newline_indent(out: &mut String, indent: Option<usize>, depth: usize) {
    if let Some(step) = indent {
        out.push('\n');
        for _ in 0..(step * depth) {
            out.push(' ');
        }
    }
}

/// Собрать `BTreeMap` семейство→hex из массива объектов — утилита для тестов.
#[cfg(test)]
pub(crate) fn debug_map(entries: &[(String, Value)]) -> std::collections::BTreeMap<String, String> {
    entries
        .iter()
        .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_scalars() {
        for src in ["null", "true", "false", "0", "-3.5", "1e3", "\"hi\""] {
            let v = parse(src).expect("parse");
            let back = v.to_compact();
            let v2 = parse(&back).expect("reparse");
            assert_eq!(v, v2, "src={src}");
        }
    }

    #[test]
    fn parse_object_preserves_order() {
        let v = parse(r#"{"b":1,"a":2,"c":3}"#).expect("parse");
        if let Value::Object(entries) = &v {
            let keys: Vec<&str> = entries.iter().map(|(k, _)| k.as_str()).collect();
            assert_eq!(keys, vec!["b", "a", "c"]);
        } else {
            panic!("не объект");
        }
    }

    #[test]
    fn parse_nested() {
        let src = r#"{ "arr": [1, 2, {"x": true}], "s": "a\nb" }"#;
        let v = parse(src).expect("parse");
        assert_eq!(v.get("arr").unwrap().as_array().unwrap().len(), 3);
        assert_eq!(v.get("s").unwrap().as_str().unwrap(), "a\nb");
    }

    #[test]
    fn parse_unicode_escape_and_surrogate() {
        let v = parse(r#""A😀""#).expect("parse");
        assert_eq!(v.as_str().unwrap(), "A\u{1F600}");
    }

    #[test]
    fn number_formatting_is_clean() {
        assert_eq!(Value::Number(0.3).to_compact(), "0.3");
        assert_eq!(Value::Number(1.0).to_compact(), "1");
        assert_eq!(Value::Number(-2.5).to_compact(), "-2.5");
    }

    #[test]
    fn pretty_is_reparseable() {
        let src = r#"{"a":[1,2,3],"b":{"c":"d"},"e":[]}"#;
        let v = parse(src).expect("parse");
        let pretty = v.to_pretty();
        assert!(pretty.contains('\n'));
        assert_eq!(parse(&pretty).expect("reparse"), v);
    }

    #[test]
    fn trailing_garbage_rejected() {
        assert!(parse("1 2").is_err());
        assert!(parse("{}}").is_err());
    }

    #[test]
    fn string_escapes_roundtrip() {
        let original = "tab\tnew\nquote\"slash\\ctl\u{1}";
        let v = Value::String(original.to_string());
        let s = v.to_compact();
        assert_eq!(parse(&s).unwrap().as_str().unwrap(), original);
    }

    #[test]
    fn debug_map_extracts_strings() {
        let entries = vec![
            ("k1".to_string(), Value::String("v1".to_string())),
            ("k2".to_string(), Value::Number(3.0)),
        ];
        let m = debug_map(&entries);
        assert_eq!(m.get("k1").map(String::as_str), Some("v1"));
        assert!(!m.contains_key("k2"));
    }
}
