//! Структурное отделение production-кода от `#[cfg(test)]` без интерпретации
//! скобок и `;` внутри строк, char-литералов и комментариев.

#[derive(Default)]
struct LexState {
    block_comment_depth: usize,
    string: bool,
    escaped: bool,
    raw_hashes: Option<usize>,
}

impl LexState {
    fn is_code(&self) -> bool {
        self.block_comment_depth == 0 && !self.string && self.raw_hashes.is_none()
    }
}

#[derive(Clone, Copy)]
enum TokenKind {
    Open,
    Close,
    Semicolon,
}

#[derive(Clone, Copy)]
struct Token {
    at: usize,
    kind: TokenKind,
}

fn char_literal_end(line: &str, quote: usize) -> Option<usize> {
    let rest = line.get(quote + 1..)?;
    let mut chars = rest.char_indices();
    let (_, first) = chars.next()?;
    if first == '\\' {
        let (escape_at, escape) = chars.next()?;
        let escape_end = quote + 1 + escape_at + escape.len_utf8();
        let closing = match escape {
            'x' => {
                let digits = line.get(escape_end..escape_end + 2)?;
                if !digits.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                    return None;
                }
                escape_end + 2
            }
            'u' => {
                if line.as_bytes().get(escape_end) != Some(&b'{') {
                    return None;
                }
                let end = line.get(escape_end + 1..)?.find('}')? + escape_end + 1;
                let digits = line.get(escape_end + 1..end)?;
                if digits.is_empty()
                    || !digits
                        .bytes()
                        .all(|byte| byte.is_ascii_hexdigit() || byte == b'_')
                {
                    return None;
                }
                end + 1
            }
            _ => escape_end,
        };
        return (line.as_bytes().get(closing) == Some(&b'\'')).then_some(closing + 1);
    }

    if first == '\'' || first == '\r' || first == '\n' {
        return None;
    }
    let closing = quote + 1 + first.len_utf8();
    (line.as_bytes().get(closing) == Some(&b'\'')).then_some(closing + 1)
}

fn raw_string_start(line: &str, at: usize) -> Option<(usize, usize)> {
    if line.as_bytes().get(at) != Some(&b'r') {
        return None;
    }
    let mut cursor = at + 1;
    while line.as_bytes().get(cursor) == Some(&b'#') {
        cursor += 1;
    }
    (line.as_bytes().get(cursor) == Some(&b'"')).then_some((cursor + 1, cursor - at - 1))
}

fn blank(masked: &mut Option<&mut Vec<u8>>, start: usize, end: usize) {
    if let Some(bytes) = masked.as_deref_mut() {
        bytes[start..end].fill(b' ');
    }
}

fn scan_line(
    line: &str,
    state: &mut LexState,
    mut uncommented: Option<&mut Vec<u8>>,
    mut syntax: Option<&mut Vec<u8>>,
) -> Vec<Token> {
    let bytes = line.as_bytes();
    let mut tokens = Vec::new();
    let mut cursor = 0;
    while cursor < bytes.len() {
        if state.block_comment_depth > 0 {
            if bytes.get(cursor..cursor + 2) == Some(b"/*") {
                blank(&mut uncommented, cursor, cursor + 2);
                blank(&mut syntax, cursor, cursor + 2);
                state.block_comment_depth += 1;
                cursor += 2;
            } else if bytes.get(cursor..cursor + 2) == Some(b"*/") {
                blank(&mut uncommented, cursor, cursor + 2);
                blank(&mut syntax, cursor, cursor + 2);
                state.block_comment_depth -= 1;
                cursor += 2;
            } else {
                blank(&mut uncommented, cursor, cursor + 1);
                blank(&mut syntax, cursor, cursor + 1);
                cursor += 1;
            }
            continue;
        }

        if let Some(hashes) = state.raw_hashes {
            if bytes[cursor] == b'"'
                && bytes
                    .get(cursor + 1..cursor + 1 + hashes)
                    .is_some_and(|suffix| suffix.iter().all(|byte| *byte == b'#'))
            {
                blank(&mut syntax, cursor, cursor + hashes + 1);
                state.raw_hashes = None;
                cursor += hashes + 1;
            } else {
                blank(&mut syntax, cursor, cursor + 1);
                cursor += 1;
            }
            continue;
        }

        if state.string {
            blank(&mut syntax, cursor, cursor + 1);
            if state.escaped {
                state.escaped = false;
            } else if bytes[cursor] == b'\\' {
                state.escaped = true;
            } else if bytes[cursor] == b'"' {
                state.string = false;
            }
            cursor += 1;
            continue;
        }

        if bytes.get(cursor..cursor + 2) == Some(b"//") {
            blank(&mut uncommented, cursor, bytes.len());
            blank(&mut syntax, cursor, bytes.len());
            break;
        }
        if bytes.get(cursor..cursor + 2) == Some(b"/*") {
            blank(&mut uncommented, cursor, cursor + 2);
            blank(&mut syntax, cursor, cursor + 2);
            state.block_comment_depth = 1;
            cursor += 2;
            continue;
        }
        if let Some((content, hashes)) = raw_string_start(line, cursor) {
            blank(&mut syntax, cursor, content);
            state.raw_hashes = Some(hashes);
            cursor = content;
            continue;
        }
        if bytes[cursor] == b'"' {
            blank(&mut syntax, cursor, cursor + 1);
            state.string = true;
            cursor += 1;
            continue;
        }
        if bytes[cursor] == b'\'' {
            if let Some(end) = char_literal_end(line, cursor) {
                blank(&mut syntax, cursor, end);
                cursor = end;
                continue;
            }
        }
        match bytes[cursor] {
            b'{' => tokens.push(Token {
                at: cursor,
                kind: TokenKind::Open,
            }),
            b'}' => tokens.push(Token {
                at: cursor,
                kind: TokenKind::Close,
            }),
            b';' => tokens.push(Token {
                at: cursor,
                kind: TokenKind::Semicolon,
            }),
            _ => {}
        }
        cursor += 1;
    }
    state.escaped = false;
    tokens
}

fn structural_tokens(line: &str, state: &mut LexState) -> Vec<Token> {
    scan_line(line, state, None, None)
}

fn only_comments_or_whitespace(text: &str) -> bool {
    let mut state = LexState::default();
    let mut code = text.as_bytes().to_vec();
    scan_line(text, &mut state, Some(&mut code), None);
    state.is_code() && code.iter().all(u8::is_ascii_whitespace)
}

fn standalone_cfg_marker(line: &str) -> bool {
    let Some(suffix) = line.trim_start().strip_prefix("#[cfg(test)]") else {
        return false;
    };
    only_comments_or_whitespace(suffix)
}

fn harmless_item_tail(line: &str, end: usize) -> bool {
    let mut tail = line.get(end..).unwrap_or_default().trim_start();
    if tail.starts_with(';') || tail.starts_with(',') {
        tail = tail.get(1..).unwrap_or_default().trim_start();
    }
    only_comments_or_whitespace(tail)
}

/// Возвращает строки production-кода с исходной нумерацией, исключая каждый
/// item под `#[cfg(test)]`. Один lexer служит двум Rust integration-аудитам Core,
/// чтобы их представление границы cfg не расходилось. Неканоническая запись,
/// где атрибут или остаток после item делят строку с кодом, сохраняется целиком:
/// лучше ложный RED, чем скрытый production-суффикс.
pub fn production_lines(source: &str) -> Vec<(usize, String)> {
    let lines: Vec<&str> = source.lines().collect();
    let mut out = Vec::new();
    let mut i = 0;
    let mut outer_state = LexState::default();
    while i < lines.len() {
        if outer_state.is_code() && standalone_cfg_marker(lines[i]) {
            let mut j = i + 1;
            while j < lines.len() && lines[j].trim().is_empty() {
                j += 1;
            }

            let mut depth = 0i32;
            let mut opened = false;
            let mut k = j;
            let mut state = LexState::default();
            let mut boundary = None;
            while k < lines.len() {
                let tokens = structural_tokens(lines[k], &mut state);
                for token in tokens {
                    match token.kind {
                        TokenKind::Open => {
                            depth += 1;
                            opened = true;
                        }
                        TokenKind::Close if opened => {
                            depth -= 1;
                            if depth == 0 {
                                boundary = Some((k, token.at + 1));
                                break;
                            }
                        }
                        TokenKind::Semicolon if !opened => {
                            boundary = Some((k, token.at + 1));
                            break;
                        }
                        TokenKind::Close | TokenKind::Semicolon => {}
                    }
                }
                if boundary.is_some() {
                    break;
                }
                k += 1;
            }
            if let Some((end_line, end)) = boundary {
                if harmless_item_tail(lines[end_line], end) {
                    i = end_line + 1;
                    outer_state = LexState::default();
                    continue;
                }
            }
        }
        out.push((i + 1, lines[i].to_string()));
        structural_tokens(lines[i], &mut outer_state);
        i += 1;
    }
    out
}

/// Одна доказанная production-строка в трёх согласованных представлениях:
/// `raw` хранит provenance-комментарии, `code` — тот же текст с комментариями,
/// заменёнными пробелами, `syntax` дополнительно маскирует содержимое литералов.
/// Номер относится к исходному файлу.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProductionLine {
    pub number: usize,
    pub raw: String,
    pub code: String,
    pub syntax: String,
}

/// Та же production-поверхность, но с лексически доказанными комментариями,
/// заменёнными пробелами. Строковые/raw/byte-литералы остаются байт-в-байт:
/// `//` внутри URL не может скрыть следующий production-токен. Пробелы, а не
/// склейка, не превращают `foo/*…*/bar` в ложный идентификатор `foobar`.
pub fn production_records(source: &str) -> Vec<ProductionLine> {
    let mut state = LexState::default();
    production_lines(source)
        .into_iter()
        .map(|(number, raw)| {
            let mut code = raw.as_bytes().to_vec();
            let mut syntax = code.clone();
            scan_line(&raw, &mut state, Some(&mut code), Some(&mut syntax));
            let code = String::from_utf8(code)
                .expect("замена байтов комментария на ASCII-пробелы сохраняет UTF-8");
            let syntax = String::from_utf8(syntax)
                .expect("маскирование литералов ASCII-пробелами сохраняет UTF-8");
            ProductionLine {
                number,
                raw,
                code,
                syntax,
            }
        })
        .collect()
}

/// Компактное представление для гейтов, которым provenance-комментарии не нужны.
pub fn production_code_lines(source: &str) -> Vec<(usize, String)> {
    production_records(source)
        .into_iter()
        .map(|line| (line.number, line.code))
        .collect()
}

/// Представление без комментариев и содержимого литералов для числовых гейтов.
pub fn production_syntax_lines(source: &str) -> Vec<(usize, String)> {
    production_records(source)
        .into_iter()
        .map(|line| (line.number, line.syntax))
        .collect()
}
