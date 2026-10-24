//! Observable Python v3 behavior, including its line-based parsing conventions.
//! Work is cached per revision rather than rescanning on every request.
use crate::{document::KEYWORDS, fnd::Database};
use std::collections::{HashMap, HashSet};
use tower_lsp::lsp_types::*;

fn start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
}
fn ident(c: char) -> bool {
    c.is_ascii_alphanumeric() || "_?!*".contains(c)
}
fn word(c: char) -> bool {
    c.is_alphanumeric() || "_?!*".contains(c)
}
fn boundary(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}
fn range(line: usize, col: usize, len: usize) -> Range {
    Range::new(
        Position::new(line as u32, col as u32),
        Position::new(line as u32, (col + len) as u32),
    )
}
fn diagnostic(r: Range, message: &str) -> Diagnostic {
    Diagnostic {
        range: r,
        message: message.into(),
        severity: Some(DiagnosticSeverity::ERROR),
        source: Some("skill-lsp".into()),
        ..Default::default()
    }
}
pub struct Symbol {
    pub name: String,
    pub function: bool,
    pub range: Range,
}
pub struct Document {
    pub text: String,
    lines: Vec<Vec<char>>,
    pub symbols: Vec<Symbol>,
    pub diagnostics: Vec<Diagnostic>,
    pub semantic: Vec<SemanticToken>,
    pub semantic_utf16: Vec<SemanticToken>,
    utf16_columns: Vec<Option<Vec<u32>>>,
    occurrences: HashMap<String, Vec<Range>>,
}
impl Document {
    pub fn new(text: String, db: &Database) -> Self {
        // str.splitlines() does not add an empty line after a final newline.
        let mut lines = Vec::new();
        let mut current = Vec::new();
        let mut cr = false;
        for c in text.chars() {
            if cr && c == '\n' {
                cr = false;
                continue;
            }
            cr = false;
            if matches!(
                c,
                '\n' | '\r'
                    | '\x0b'
                    | '\x0c'
                    | '\x1c'
                    | '\x1d'
                    | '\x1e'
                    | '\u{85}'
                    | '\u{2028}'
                    | '\u{2029}'
            ) {
                lines.push(std::mem::take(&mut current));
                cr = c == '\r';
            } else {
                current.push(c);
            }
        }
        if !current.is_empty() {
            lines.push(current);
        }
        let chars: Vec<char> = text.chars().collect();
        let mut symbols = Vec::new();
        // Python applies the function regex first, then the variable regex, to raw text.
        for function in [true, false] {
            let names: &[&str] = if function {
                &["procedure", "defun", "defmacro", "defmethod", "defgeneric"]
            } else {
                &["defvar", "setq"]
            };
            let mut i = 0;
            let mut line = 0;
            let mut col = 0;
            while i < chars.len() {
                let found = names.iter().find_map(|name| {
                    let n = name.len();
                    if !chars.get(i..i + n)?.iter().copied().eq(name.chars()) {
                        return None;
                    }
                    let mut j = i + n;
                    if !function && !chars.get(j).is_some_and(|c| c.is_whitespace()) {
                        return None;
                    }
                    while chars.get(j).is_some_and(|c| c.is_whitespace()) {
                        j += 1;
                    }
                    if function {
                        if chars.get(j) != Some(&'(') {
                            return None;
                        }
                        j += 1;
                        while chars.get(j).is_some_and(|c| c.is_whitespace()) {
                            j += 1;
                        }
                    }
                    if !chars.get(j).copied().is_some_and(start) {
                        return None;
                    }
                    let begin = j;
                    j += 1;
                    while chars.get(j).copied().is_some_and(ident) {
                        j += 1;
                    }
                    Some((chars[begin..j].iter().collect::<String>(), j))
                });
                if let Some((name, end)) = found {
                    let r = range(line, col, name.chars().count());
                    symbols.push(Symbol {
                        name,
                        function,
                        range: r,
                    });
                    while i < end {
                        if chars[i] == '\n' {
                            line += 1;
                            col = 0;
                        } else {
                            col += 1;
                        }
                        i += 1;
                    }
                } else {
                    if chars[i] == '\n' {
                        line += 1;
                        col = 0;
                    } else {
                        col += 1;
                    }
                    i += 1;
                }
            }
        }
        let utf16_columns = lines
            .iter()
            .map(|line| {
                if line.iter().all(|c| c.len_utf16() == 1) {
                    return None;
                }
                let mut offsets = vec![0];
                for c in line {
                    offsets.push(offsets.last().unwrap() + c.len_utf16() as u32);
                }
                Some(offsets)
            })
            .collect();
        let mut d = Self {
            text,
            lines,
            symbols,
            diagnostics: vec![],
            semantic: vec![],
            semantic_utf16: vec![],
            utf16_columns,
            occurrences: HashMap::new(),
        };
        d.analyze(db);
        let mut line = 0;
        let mut col = 0;
        let mut previous = Position::new(0, 0);
        d.semantic_utf16 = d
            .semantic
            .iter()
            .map(|token| {
                line += token.delta_line;
                col = if token.delta_line == 0 {
                    col + token.delta_start
                } else {
                    token.delta_start
                };
                let start = d.to_utf16(Position::new(line, col));
                let end = d.to_utf16(Position::new(line, col + token.length));
                let converted = SemanticToken {
                    delta_line: start.line - previous.line,
                    delta_start: if start.line == previous.line {
                        start.character - previous.character
                    } else {
                        start.character
                    },
                    length: end.character - start.character,
                    ..*token
                };
                previous = start;
                converted
            })
            .collect();
        d
    }
    pub fn to_utf16(&self, mut pos: Position) -> Position {
        if let Some(Some(columns)) = self.utf16_columns.get(pos.line as usize) {
            let character = pos.character as usize;
            pos.character = columns.get(character).copied().unwrap_or_else(|| {
                columns.last().unwrap() + character.saturating_sub(columns.len() - 1) as u32
            });
        }
        pos
    }
    pub fn from_utf16(&self, mut pos: Position) -> Position {
        if let Some(Some(columns)) = self.utf16_columns.get(pos.line as usize) {
            pos.character = columns
                .partition_point(|&col| col <= pos.character)
                .saturating_sub(1) as u32;
        }
        pos
    }
    pub fn symbol(&self, name: &str) -> Option<&Symbol> {
        self.symbols.iter().find(|s| s.name == name)
    }
    pub fn word(&self, p: Position) -> String {
        let Some(line) = self.lines.get(p.line as usize) else {
            return String::new();
        };
        let mut a = (p.character as usize).min(line.len());
        let mut b = a;
        while a > 0 && word(line[a - 1]) {
            a -= 1;
        }
        while b < line.len() && word(line[b]) {
            b += 1;
        }
        line[a..b].iter().collect()
    }
    pub fn prefix(&self, p: Position) -> String {
        let Some(line) = self.lines.get(p.line as usize) else {
            return String::new();
        };
        let b = (p.character as usize).min(line.len());
        let mut a = b;
        while a > 0 && word(line[a - 1]) {
            a -= 1;
        }
        line[a..b].iter().collect()
    }
    pub fn document_symbols(&self) -> Vec<DocumentSymbol> {
        self.symbols
            .iter()
            .filter(|s| s.function)
            .map(|s| {
                #[allow(deprecated)]
                DocumentSymbol {
                    name: s.name.clone(),
                    kind: SymbolKind::FUNCTION,
                    range: s.range,
                    selection_range: s.range,
                    detail: None,
                    tags: None,
                    deprecated: None,
                    children: None,
                }
            })
            .collect()
    }
    pub fn references(&self, name: &str) -> Vec<Range> {
        self.occurrences.get(name).cloned().unwrap_or_default()
    }
    fn analyze(&mut self, db: &Database) {
        let functions: HashSet<&str> = self
            .symbols
            .iter()
            .filter(|s| s.function)
            .map(|s| s.name.as_str())
            .collect();
        let variables: HashSet<&str> = self
            .symbols
            .iter()
            .filter(|s| !s.function)
            .map(|s| s.name.as_str())
            .collect();
        let mut stack = Vec::new();
        let mut previous = Position::new(0, 0);
        for (ln, line) in self.lines.iter().enumerate() {
            // Lint intentionally resets quote state on each line, like Python.
            let mut quoted = false;
            let mut escaped = false;
            for (col, &c) in line.iter().enumerate() {
                if escaped {
                    escaped = false;
                    continue;
                }
                if c == '\\' && quoted {
                    escaped = true;
                    continue;
                }
                if c == '"' {
                    quoted = !quoted;
                    continue;
                }
                if quoted {
                    continue;
                }
                if c == ';' {
                    break;
                }
                if c == '(' {
                    stack.push((ln, col));
                }
                if c == ')' && stack.pop().is_none() {
                    self.diagnostics.push(diagnostic(
                        range(ln, col, 1),
                        "Unmatched closing parenthesis",
                    ));
                }
            }
            // Cache regex-style references, including strings and comments.
            for a in 0..line.len() {
                if (a > 0 && boundary(line[a - 1])) == boundary(line[a]) {
                    continue;
                }
                let mut b = a;
                while b < line.len() && word(line[b]) {
                    b += 1;
                }
                // A regex word boundary after punctuation may terminate a match early.
                for end in a + 1..=b {
                    let left = boundary(line[end - 1]);
                    let right = line.get(end).copied().is_some_and(boundary);
                    if left != right {
                        let name: String = line[a..end].iter().collect();
                        self.occurrences
                            .entry(name)
                            .or_default()
                            .push(range(ln, a, end - a));
                    }
                }
            }
            let mut col = 0;
            while col < line.len() {
                let a = col;
                let c = line[col];
                let mut ty = None;
                if c == ';' {
                    break;
                }
                if c == '"' {
                    col += 1;
                    while col < line.len() {
                        if line[col] == '\\' {
                            col += 2;
                        } else if line[col] == '"' {
                            col += 1;
                            break;
                        } else {
                            col += 1;
                        }
                    }
                    ty = Some(14);
                } else if c == '@' && line.get(col + 1).copied().is_some_and(start) {
                    col += 2;
                    while line
                        .get(col)
                        .is_some_and(|c| c.is_ascii_alphanumeric() || *c == '_')
                    {
                        col += 1;
                    }
                    ty = Some(7);
                } else if c == '?' && line.get(col + 1).copied().is_some_and(start) {
                    col += 2;
                    while line
                        .get(col)
                        .is_some_and(|c| c.is_ascii_alphanumeric() || *c == '_')
                    {
                        col += 1;
                    }
                    ty = Some(17);
                } else if let Some(n) = number_len(&line[col..]) {
                    col += n;
                    ty = Some(15);
                } else if start(c) {
                    col += 1;
                    while line.get(col).copied().is_some_and(ident) {
                        col += 1;
                    }
                    let name: String = line[a..col].iter().collect();
                    ty = if crate::car_cdr::is_accessor(&name) {
                        Some(crate::car_cdr::TOKEN_TYPE)
                    } else if let Some(e) = db.get(&name).filter(|_| name != "t" && name != "nil") {
                        Some(e.keyword_category as u32)
                    } else if KEYWORDS.contains(&name.as_str()) && name != "t" && name != "nil" {
                        Some(0)
                    } else if functions.contains(name.as_str()) {
                        Some(11)
                    } else if variables.contains(name.as_str()) {
                        Some(12)
                    } else {
                        None
                    };
                } else {
                    col += 1;
                }
                if let Some(token_type) = ty {
                    let pos = Position::new(ln as u32, a as u32);
                    self.semantic.push(SemanticToken {
                        delta_line: pos.line - previous.line,
                        delta_start: if pos.line == previous.line {
                            pos.character - previous.character
                        } else {
                            pos.character
                        },
                        length: (col - a) as u32,
                        token_type,
                        token_modifiers_bitset: 0,
                    });
                    previous = pos;
                }
            }
        }
        for (ln, col) in stack {
            self.diagnostics.push(diagnostic(
                range(ln, col, 1),
                "Unclosed opening parenthesis",
            ));
        }
    }
    pub fn call(&self, p: Position) -> Option<(String, usize)> {
        let line = self.lines.get(p.line as usize)?;
        let end = (p.character as usize).min(line.len());
        let mut depth = 0;
        for col in (0..end).rev() {
            if line[col] == ')' {
                depth += 1;
            } else if line[col] == '(' {
                if depth > 0 {
                    depth -= 1;
                    continue;
                }
                let mut a = col + 1;
                while line.get(a) == Some(&' ') {
                    a += 1;
                }
                if !line.get(a).copied().is_some_and(start) {
                    return None;
                }
                let mut b = a + 1;
                while line.get(b).copied().is_some_and(ident) {
                    b += 1;
                }
                return Some((
                    line[a..b].iter().collect(),
                    line[col + 1..end].iter().filter(|&&c| c == ',').count(),
                ));
            }
        }
        None
    }
    pub fn format(&self, options: &FormattingOptions) -> Vec<TextEdit> {
        let unit = " ".repeat(options.tab_size as usize);
        let mut indent = 0usize;
        let mut output = String::new();
        for line in &self.lines {
            let raw: String = line.iter().collect();
            let stripped = raw.trim();
            if stripped.is_empty() {
                output.push('\n');
                continue;
            }
            let close = stripped.chars().take_while(|&c| c == ')').count();
            output.push_str(&unit.repeat(indent.saturating_sub(close)));
            output.push_str(stripped);
            output.push('\n');
            let mut quoted = false;
            let mut delta = 0isize;
            for c in stripped.chars() {
                if c == '"' {
                    quoted = !quoted;
                }
                if quoted {
                    continue;
                }
                if c == ';' {
                    break;
                }
                if c == '(' {
                    delta += 1;
                } else if c == ')' {
                    delta -= 1;
                }
            }
            indent = (indent as isize + delta).max(0) as usize;
        }
        vec![TextEdit {
            range: Range::new(
                Position::new(0, 0),
                Position::new(self.lines.len() as u32, 0),
            ),
            new_text: output,
        }]
    }
}
fn number_len(s: &[char]) -> Option<usize> {
    let mut i = usize::from(s.first() == Some(&'-'));
    if s.get(i) == Some(&'0')
        && s.get(i + 1) == Some(&'x')
        && s.get(i + 2).is_some_and(|c| c.is_ascii_hexdigit())
    {
        i += 2;
        while s.get(i).is_some_and(|c| c.is_ascii_hexdigit()) {
            i += 1;
        }
        return Some(i);
    }
    let begin = i;
    while s.get(i).is_some_and(|c| c.is_numeric()) {
        i += 1;
    }
    if i == begin {
        return None;
    }
    if s.get(i) == Some(&'.') {
        i += 1;
        while s.get(i).is_some_and(|c| c.is_numeric()) {
            i += 1;
        }
    }
    if s.get(i).is_some_and(|c| *c == 'e' || *c == 'E') {
        let before = i;
        i += 1;
        if s.get(i).is_some_and(|c| *c == '+' || *c == '-') {
            i += 1;
        }
        let digits = i;
        while s.get(i).is_some_and(|c| c.is_numeric()) {
            i += 1;
        }
        if i == digits {
            i = before;
        }
    }
    Some(i)
}
