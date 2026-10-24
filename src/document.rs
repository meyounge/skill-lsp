//! One lexical scan per document revision; all requests share the resulting snapshot.
//! UTF-8 byte offsets are internal; protocol ranges always use UTF-16 code units.
use std::collections::HashMap;
use tower_lsp::lsp_types::*;

pub const TOKEN_NAMES: [&str; 19] = [
    "keyword",
    "keyword1",
    "keyword2",
    "keyword3",
    "keyword4",
    "keyword5",
    "keyword6",
    "keyword7",
    "keyword8",
    "keyword9",
    "keyword10",
    "function",
    "variable",
    "comment",
    "string",
    "number",
    "operator",
    "kwarg",
    "literal",
];
pub const KEYWORDS: &[&str] = &[
    "if",
    "when",
    "unless",
    "else",
    "then",
    "cond",
    "case",
    "caseq",
    "foreach",
    "for",
    "while",
    "loop",
    "defun",
    "procedure",
    "lambda",
    "nlambda",
    "mprocedure",
    "nprocedure",
    "defmacro",
    "defmethod",
    "defgeneric",
    "defclass",
    "defstruct",
    "defvar",
    "let",
    "let*",
    "letseq",
    "letrec",
    "prog",
    "prog1",
    "prog2",
    "progn",
    "setq",
    "set",
    "set!",
    "define",
    "return",
    "go",
    "break",
    "continue",
    "and",
    "or",
    "not",
    "quote",
    "quasiquote",
    "begin",
    "t",
    "nil",
];
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Ident,
    Kwarg,
    Number,
    String,
    Comment,
    Open,
    Close,
    Comma,
    Operator,
}
#[derive(Clone, Debug)]
pub struct Token {
    pub start: usize,
    pub end: usize,
    pub kind: Kind,
    pub range: Range,
}
#[derive(Clone, Debug)]
pub struct Symbol {
    pub name: String,
    pub token: usize,
    pub function: bool,
    pub end: usize,
}
#[derive(Clone, Debug)]
pub struct Call {
    pub name: String,
    pub active: usize,
    pub keyword: Option<String>,
}
pub struct Document {
    pub text: String,
    pub version: i32,
    pub tokens: Vec<Token>,
    pub symbols: Vec<Symbol>,
    pub occurrences: HashMap<String, Vec<usize>>,
    pub diagnostics: Vec<Diagnostic>,
    lines: Vec<usize>,
    matching: HashMap<usize, usize>,
    symbol_index: HashMap<String, usize>,
}
fn ident_start(c: char) -> bool {
    c.is_alphabetic() || c == '_'
}
pub fn ident_continue(c: char) -> bool {
    c.is_alphanumeric() || "_?!*".contains(c)
}
fn diagnostic(range: Range, message: &str) -> Diagnostic {
    Diagnostic {
        range,
        severity: Some(DiagnosticSeverity::ERROR),
        source: Some("skill-lsp".into()),
        message: message.into(),
        ..Default::default()
    }
}
impl Document {
    pub fn new(text: String, version: i32) -> Self {
        let mut lines = vec![0];
        for (i, b) in text.bytes().enumerate() {
            if b == b'\n' {
                lines.push(i + 1);
            }
        }
        let mut doc = Self {
            text,
            version,
            tokens: Vec::new(),
            symbols: Vec::new(),
            occurrences: HashMap::new(),
            diagnostics: Vec::new(),
            lines,
            matching: HashMap::new(),
            symbol_index: HashMap::new(),
        };
        doc.scan();
        doc.index();
        doc
    }
    pub fn position(&self, offset: usize) -> Position {
        let mut offset = offset.min(self.text.len());
        while !self.text.is_char_boundary(offset) {
            offset -= 1;
        }
        let line = self
            .lines
            .partition_point(|&s| s <= offset)
            .saturating_sub(1);
        Position::new(
            line as u32,
            self.text[self.lines[line]..offset].encode_utf16().count() as u32,
        )
    }
    pub fn offset(&self, p: Position) -> usize {
        let Some(&start) = self.lines.get(p.line as usize) else {
            return self.text.len();
        };
        let end = self
            .lines
            .get(p.line as usize + 1)
            .copied()
            .unwrap_or(self.text.len());
        let mut units = 0;
        for (i, ch) in self.text[start..end].char_indices() {
            if units + ch.len_utf16() > p.character as usize || ch == '\r' || ch == '\n' {
                return start + i;
            }
            units += ch.len_utf16();
        }
        end
    }
    pub fn range(&self, start: usize, end: usize) -> Range {
        Range::new(self.position(start), self.position(end))
    }
    pub fn value(&self, t: usize) -> &str {
        let t = &self.tokens[t];
        &self.text[t.start..t.end]
    }
    pub fn token_at(&self, p: Position) -> Option<usize> {
        let offset = self.offset(p);
        let i = self
            .tokens
            .partition_point(|t| t.start <= offset)
            .checked_sub(1)?;
        let t = &self.tokens[i];
        (t.kind == Kind::Ident && offset <= t.end).then_some(i)
    }
    pub fn prefix(&self, p: Position) -> String {
        let end = self.offset(p);
        let mut start = end;
        for (i, ch) in self.text[..end].char_indices().rev() {
            if ident_continue(ch) {
                start = i;
            } else {
                break;
            }
        }
        self.text[start..end].to_string()
    }
    pub fn in_literal(&self, p: Position) -> bool {
        let offset = self.offset(p);
        self.tokens
            .partition_point(|t| t.start < offset)
            .checked_sub(1)
            .and_then(|i| self.tokens.get(i))
            .is_some_and(|t| offset < t.end && matches!(t.kind, Kind::String | Kind::Comment))
    }
    fn push(&mut self, start: usize, end: usize, kind: Kind) {
        self.tokens.push(Token {
            start,
            end,
            kind,
            range: self.range(start, end),
        });
    }
    fn scan(&mut self) {
        let mut i = 0;
        let mut stack: Vec<usize> = Vec::new();
        while i < self.text.len() {
            let start = i;
            let c = self.text[i..].chars().next().unwrap();
            if c.is_whitespace() {
                i += c.len_utf8();
                continue;
            }
            let bytes = self.text.as_bytes();
            let kind;
            if c == ';' {
                i = self.text[i..]
                    .find('\n')
                    .map(|n| i + n)
                    .unwrap_or(self.text.len());
                kind = Kind::Comment;
            } else if bytes[i..].starts_with(b"/*") {
                i += 2;
                if let Some(n) = self.text[i..].find("*/") {
                    i += n + 2;
                } else {
                    i = self.text.len();
                    self.diagnostics.push(diagnostic(
                        self.range(start, start + 2),
                        "Unterminated block comment",
                    ));
                }
                kind = Kind::Comment;
            } else if c == '"' {
                i += 1;
                let mut closed = false;
                while i < self.text.len() {
                    let c = self.text[i..].chars().next().unwrap();
                    i += c.len_utf8();
                    if c == '\\' {
                        if i < self.text.len() {
                            i += self.text[i..].chars().next().unwrap().len_utf8();
                        }
                    } else if c == '"' {
                        closed = true;
                        break;
                    }
                }
                if !closed {
                    self.diagnostics.push(diagnostic(
                        self.range(start, start + 1),
                        "Unterminated string",
                    ));
                }
                kind = Kind::String;
            } else if ident_start(c)
                || c == '?' && self.text[i + 1..].chars().next().is_some_and(ident_start)
            {
                kind = if c == '?' { Kind::Kwarg } else { Kind::Ident };
                i += c.len_utf8();
                while i < self.text.len() {
                    let c = self.text[i..].chars().next().unwrap();
                    if !ident_continue(c) {
                        break;
                    }
                    i += c.len_utf8();
                }
            } else if c.is_ascii_digit()
                || (c == '-' || c == '.') && bytes.get(i + 1).is_some_and(u8::is_ascii_digit)
            {
                kind = Kind::Number;
                if bytes[i..].starts_with(b"0x") || bytes[i..].starts_with(b"-0x") {
                    i += if c == '-' { 3 } else { 2 };
                    while bytes.get(i).is_some_and(u8::is_ascii_hexdigit) {
                        i += 1;
                    }
                } else {
                    if c == '-' {
                        i += 1;
                    }
                    while bytes.get(i).is_some_and(u8::is_ascii_digit) {
                        i += 1;
                    }
                    if bytes.get(i) == Some(&b'.') {
                        i += 1;
                        while bytes.get(i).is_some_and(u8::is_ascii_digit) {
                            i += 1;
                        }
                    }
                    if bytes.get(i).is_some_and(|b| *b == b'e' || *b == b'E') {
                        let exp = i;
                        i += 1;
                        if bytes.get(i).is_some_and(|b| *b == b'+' || *b == b'-') {
                            i += 1;
                        }
                        let digits = i;
                        while bytes.get(i).is_some_and(u8::is_ascii_digit) {
                            i += 1;
                        }
                        if digits == i {
                            i = exp;
                        }
                    }
                }
            } else {
                kind = match c {
                    '(' => Kind::Open,
                    ')' => Kind::Close,
                    ',' => Kind::Comma,
                    _ => Kind::Operator,
                };
                i += c.len_utf8();
            }
            let index = self.tokens.len();
            self.push(start, i, kind);
            if kind == Kind::Open {
                stack.push(index);
            } else if kind == Kind::Close {
                if let Some(open) = stack.pop() {
                    self.matching.insert(open, index);
                    self.matching.insert(index, open);
                } else {
                    self.diagnostics.push(diagnostic(
                        self.range(start, i),
                        "Unmatched closing parenthesis",
                    ));
                }
            }
        }
        for t in stack {
            self.diagnostics.push(diagnostic(
                self.tokens[t].range,
                "Unclosed opening parenthesis",
            ));
        }
    }
    fn index(&mut self) {
        let code: Vec<usize> = self
            .tokens
            .iter()
            .enumerate()
            .filter(|(_, t)| t.kind != Kind::Comment)
            .map(|(i, _)| i)
            .collect();
        for (k, &i) in code.iter().enumerate() {
            if self.tokens[i].kind != Kind::Ident {
                continue;
            }
            let word = self.value(i).to_string();
            self.occurrences.entry(word.clone()).or_default().push(i);
            let function = [
                "procedure",
                "mprocedure",
                "nprocedure",
                "defun",
                "defmacro",
                "defmethod",
                "defgeneric",
            ]
            .contains(&word.as_str());
            let variable = ["defvar", "setq", "define"].contains(&word.as_str());
            if !function && !variable {
                continue;
            }
            // Both procedure(foo(...)) and (defun foo (...) ...) / (setq x ...) work.
            let next = if code
                .get(k + 1)
                .is_some_and(|&j| self.tokens[j].kind == Kind::Open)
            {
                k + 2
            } else {
                k + 1
            };
            if let Some(&name) = code.get(next) {
                if self.tokens[name].kind == Kind::Ident {
                    let open = code
                        .get(k + 1)
                        .copied()
                        .filter(|&j| self.tokens[j].kind == Kind::Open)
                        .or_else(|| {
                            k.checked_sub(1)
                                .and_then(|p| code.get(p).copied())
                                .filter(|&j| self.tokens[j].kind == Kind::Open)
                        });
                    let end = open
                        .and_then(|j| self.matching.get(&j).copied())
                        .map(|j| self.tokens[j].end)
                        .unwrap_or(self.tokens[name].end);
                    self.symbols.push(Symbol {
                        name: self.value(name).into(),
                        token: name,
                        function,
                        end,
                    });
                }
            }
        }
        // Also recognize ordinary SKILL variable assignment, excluding equality and accessors.
        for (k, pair) in code.windows(3).enumerate() {
            let (a, b, c) = (pair[0], pair[1], pair[2]);
            if self.tokens[a].kind == Kind::Ident && self.value(b) == "=" && self.value(c) != "=" {
                let prev = k.checked_sub(1).map(|k| code[k]);
                if prev.is_none_or(|p| !["=", "!", "<", ">", "~", "->"].contains(&self.value(p))) {
                    let name = self.value(a).to_string();
                    if !self.symbols.iter().any(|s| s.name == name) {
                        self.symbols.push(Symbol {
                            name,
                            token: a,
                            function: false,
                            end: self.tokens[a].end,
                        });
                    }
                }
            }
        }
        self.symbols.sort_by_key(|s| s.token);
        for (i, s) in self.symbols.iter().enumerate() {
            self.symbol_index.entry(s.name.clone()).or_insert(i);
        }
    }
    pub fn symbol(&self, name: &str) -> Option<&Symbol> {
        self.symbol_index.get(name).map(|&i| &self.symbols[i])
    }
    pub fn definition(&self, name: &str) -> Option<Range> {
        self.symbol(name).map(|s| self.tokens[s.token].range)
    }
    pub fn document_symbols(&self) -> Vec<DocumentSymbol> {
        self.symbols
            .iter()
            .map(|s| {
                let t = &self.tokens[s.token];
                #[allow(deprecated)]
                DocumentSymbol {
                    name: s.name.clone(),
                    detail: Some("user defined".into()),
                    kind: if s.function {
                        SymbolKind::FUNCTION
                    } else {
                        SymbolKind::VARIABLE
                    },
                    tags: None,
                    deprecated: None,
                    range: self.range(t.start, s.end),
                    selection_range: t.range,
                    children: None,
                }
            })
            .collect()
    }
    pub fn semantic_tokens(&self, db: &crate::fnd::Database) -> Vec<SemanticToken> {
        let mut result = Vec::new();
        let mut prev = Position::new(0, 0);
        for (i, t) in self.tokens.iter().enumerate() {
            let ty = match t.kind {
                Kind::Comment => Some(13),
                Kind::String => Some(14),
                Kind::Number => Some(15),
                Kind::Kwarg => Some(17),
                Kind::Ident => {
                    let name = self.value(i);
                    if name == "t" || name == "nil" {
                        None
                    } else if let Some(s) = self.symbol(name) {
                        Some(if s.function { 11 } else { 12 })
                    } else if let Some(e) = db.get(name) {
                        Some(e.keyword_category as u32)
                    } else if KEYWORDS.contains(&name) {
                        Some(0)
                    } else {
                        None
                    }
                }
                _ => None,
            };
            let Some(token_type) = ty else {
                continue;
            };
            // LSP semantic tokens cannot span lines. Split comments and multiline strings.
            let mut start = t.start;
            for part in self.text[t.start..t.end].split_inclusive('\n') {
                let value = part.trim_end_matches(['\r', '\n']);
                if !value.is_empty() {
                    let pos = self.position(start);
                    result.push(SemanticToken {
                        delta_line: pos.line - prev.line,
                        delta_start: if pos.line == prev.line {
                            pos.character - prev.character
                        } else {
                            pos.character
                        },
                        length: value.encode_utf16().count() as u32,
                        token_type,
                        token_modifiers_bitset: 0,
                    });
                    prev = pos;
                }
                start += part.len();
            }
        }
        result
    }
    /// Find the innermost still-open call in either SKILL syntax, across lines.
    pub fn call_at(&self, p: Position) -> Option<Call> {
        let offset = self.offset(p);
        if self.in_literal(p) {
            return None;
        }
        let code: Vec<usize> = self
            .tokens
            .iter()
            .enumerate()
            .take_while(|(_, t)| t.start < offset)
            .filter(|(_, t)| t.kind != Kind::Comment)
            .map(|(i, _)| i)
            .collect();
        let mut opens = Vec::new();
        for (k, &i) in code.iter().enumerate() {
            match self.tokens[i].kind {
                Kind::Open => opens.push(k),
                Kind::Close => {
                    opens.pop();
                }
                _ => {}
            }
        }
        for k in opens.into_iter().rev() {
            let open = code[k];
            let before = k.checked_sub(1).map(|j| code[j]).filter(|&j| {
                self.tokens[j].kind == Kind::Ident && self.tokens[j].end == self.tokens[open].start
            });
            let after = code
                .get(k + 1)
                .copied()
                .filter(|&j| self.tokens[j].kind == Kind::Ident);
            let Some(name) = before.or(after) else {
                continue;
            };
            let first = if before.is_some() { k + 1 } else { k + 2 };
            let mut depth = 0usize;
            let mut args = 0usize;
            let mut keyword = None;
            let mut last_end = None;
            let mut commas = 0;
            for &i in code.iter().skip(first) {
                let t = &self.tokens[i];
                if depth == 0 {
                    if t.kind == Kind::Comma {
                        commas += 1;
                    } else if t.kind != Kind::Close {
                        // Adjacent foo(...) is one argument, not two; whitespace separates atoms.
                        if !(t.kind == Kind::Open && last_end == Some(t.start)) {
                            args += 1;
                        }
                        if t.kind == Kind::Kwarg {
                            keyword = Some(self.value(i).into());
                        }
                    }
                }
                match t.kind {
                    Kind::Open => depth += 1,
                    Kind::Close => depth = depth.saturating_sub(1),
                    _ => {}
                }
                last_end = Some(t.end);
            }
            let trailing = last_end.is_some_and(|end| {
                end < offset && self.text[end..offset].chars().any(char::is_whitespace)
            });
            let active = if commas > 0 {
                commas
            } else if trailing {
                args
            } else {
                args.saturating_sub(1)
            };
            return Some(Call {
                name: self.value(name).into(),
                active,
                keyword,
            });
        }
        None
    }
    /// Conservative parenthesis indentation. Never changes bytes inside multiline literals/comments.
    pub fn format(&self, options: &FormattingOptions) -> String {
        let unit = if options.insert_spaces {
            " ".repeat((options.tab_size as usize).min(32))
        } else {
            "\t".into()
        };
        let mut result = String::new();
        let mut depth = 0usize;
        let mut token_cursor = 0;
        for (line, &start) in self.lines.iter().enumerate() {
            let end = self.lines.get(line + 1).copied().unwrap_or(self.text.len());
            if start == end {
                continue;
            }
            let raw = &self.text[start..end];
            let content = raw.trim_end_matches(['\r', '\n']);
            let newline = &raw[content.len()..];
            while token_cursor < self.tokens.len() && self.tokens[token_cursor].end <= start {
                token_cursor += 1;
            }
            let protected = self
                .tokens
                .get(token_cursor)
                .is_some_and(|t| t.start < start && matches!(t.kind, Kind::String | Kind::Comment));
            let mut j = token_cursor;
            let mut leading_close = 0;
            let mut only_close = true;
            let mut delta = 0i64;
            while let Some(t) = self.tokens.get(j) {
                if t.start >= end {
                    break;
                }
                if t.start >= start {
                    match t.kind {
                        Kind::Open => {
                            delta += 1;
                            only_close = false;
                        }
                        Kind::Close => {
                            delta -= 1;
                            if only_close {
                                leading_close += 1;
                            }
                        }
                        _ => {
                            only_close = false;
                        }
                    }
                }
                j += 1;
            }
            if protected {
                result.push_str(raw);
            } else if content.trim().is_empty() {
                result.push_str(newline);
            } else {
                result.push_str(&unit.repeat(depth.saturating_sub(leading_close)));
                result.push_str(content.trim());
                result.push_str(newline);
            }
            depth = (depth as i64 + delta).max(0) as usize;
        }
        result
    }
}
