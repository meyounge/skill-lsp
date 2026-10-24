//! Local-only API Finder parser. No Cadence documentation is distributed here.
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fs,
    path::Path,
};
use walkdir::WalkDir;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Param {
    pub raw: String,
    pub name: String,
    pub type_str: String,
    pub optional: bool,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    pub name: String,
    pub signature_raw: String,
    pub description: String,
    pub params: Vec<Param>,
    pub return_type: Option<String>,
    pub source_file: String,
    pub keyword_category: u8,
}
impl Entry {
    pub fn signature_label(&self) -> String {
        format!(
            "{}({})",
            self.name,
            self.params
                .iter()
                .map(|p| if p.optional {
                    format!("[{}]", p.name)
                } else {
                    p.name.clone()
                })
                .collect::<Vec<_>>()
                .join(", ")
        )
    }
    pub fn markdown(&self) -> String {
        let mut lines = vec![format!("**{}**\n", self.name)];
        if !self.params.is_empty() {
            let name_w = self
                .params
                .iter()
                .map(|p| p.name.chars().count())
                .max()
                .unwrap()
                .min(28);
            let type_w = self
                .params
                .iter()
                .map(|p| p.type_str.chars().count())
                .max()
                .unwrap()
                .min(32);
            lines.push("Parameters:".into());
            for p in &self.params {
                lines.push(format!(
                    "  {:<name_w$}  {:<type_w$}{}",
                    p.name,
                    p.type_str,
                    if p.optional { "  [optional]" } else { "" }
                ));
            }
            lines.push(String::new());
        }
        if let Some(ret) = &self.return_type {
            lines.push(format!("Returns: {ret}\n"));
        }
        lines.extend(wrap_description(&self.description));
        lines.join("\n ")
    }
}
// Python textwrap's default width, whitespace replacement and long-word splitting.
fn wrap_description(text: &str) -> Vec<String> {
    let mut expanded = String::new();
    let mut column = 0;
    for c in text.chars() {
        if c == '\t' {
            let n = 8 - column % 8;
            expanded.push_str(&" ".repeat(n));
            column += n;
        } else {
            expanded.push(if matches!(c, '\n' | '\r' | '\x0b' | '\x0c') {
                ' '
            } else {
                c
            });
            column = if c == '\n' || c == '\r' {
                0
            } else {
                column + 1
            };
        }
    }
    let chars: Vec<_> = expanded.chars().collect();
    let mut chunks = std::collections::VecDeque::new();
    let mut begin = 0;
    let mut i = 0;
    let letter = |c: char| c.is_alphabetic() || c == '_';
    let wp = |c: char| c.is_alphanumeric() || "_!\"'&.,?".contains(c);
    while i < chars.len() {
        if chars[i] == '-' && i > begin && wp(chars[i - 1]) {
            let mut end = i;
            while chars.get(end) == Some(&'-') {
                end += 1;
            }
            if end - i >= 2
                && chars
                    .get(end)
                    .is_some_and(|c| c.is_alphanumeric() || *c == '_')
            {
                chunks.push_back(chars[begin..i].iter().collect());
                chunks.push_back(chars[i..end].iter().collect());
                begin = end;
                i = end;
                continue;
            }
        }
        i += 1;
        let hyphen = chars[i - 1] == '-'
            && ((i >= 3 && letter(chars[i - 2]) && letter(chars[i - 3]))
                || (i >= 4 && letter(chars[i - 2]) && chars[i - 3] == '-' && letter(chars[i - 4])))
            && chars.get(i).copied().is_some_and(letter)
            && (chars.get(i + 1).copied().is_some_and(letter)
                || chars.get(i + 1) == Some(&'-') && chars.get(i + 2).copied().is_some_and(letter));
        if i == chars.len() || (chars[i] == ' ') != (chars[i - 1] == ' ') || hyphen {
            chunks.push_back(chars[begin..i].iter().collect::<String>());
            begin = i;
        }
    }
    let mut lines = Vec::new();
    while !chunks.is_empty() {
        if !lines.is_empty() && chunks.front().is_some_and(|s| s.trim().is_empty()) {
            chunks.pop_front();
        }
        let mut line = String::new();
        while chunks
            .front()
            .is_some_and(|s| line.chars().count() + s.chars().count() <= 72)
        {
            line.push_str(&chunks.pop_front().unwrap());
        }
        if chunks.front().is_some_and(|s| s.chars().count() > 72) {
            let available = 72 - line.chars().count();
            let chunk = chunks.pop_front().unwrap();
            let mut count = available;
            let chars: Vec<_> = chunk.chars().collect();
            if let Some(hyphen) = chars[..available.min(chars.len())]
                .iter()
                .rposition(|&c| c == '-')
            {
                if hyphen > 0 && chars[..hyphen].iter().any(|&c| c != '-') {
                    count = hyphen + 1;
                }
            }
            line.extend(chunk.chars().take(count));
            let rest: String = chunk.chars().skip(count).collect();
            if !rest.is_empty() {
                chunks.push_front(rest);
            }
        }
        let line = line.trim_end_matches(' ').to_string();
        if !line.is_empty() {
            lines.push(line);
        }
    }
    lines
}
pub fn resolve_prefix(prefix: &str) -> String {
    if prefix.is_empty() {
        return "any".into();
    }
    prefix
        .chars()
        .map(|c| match c {
            'a' => "array".into(),
            'b' => "Boolean".into(),
            'c' => "OPF context".into(),
            'd' => "db object".into(),
            'e' => "environment".into(),
            'f' => "float".into(),
            'F' => "OPF file ID".into(),
            'g' => "any".into(),
            'G' => "gdm spec".into(),
            'h' => "hierarchical db object".into(),
            'l' => "list".into(),
            'm' => "nmpll user type".into(),
            'M' => "CDS eval object".into(),
            'n' => "number".into(),
            'o' => "user type".into(),
            'p' => "port".into(),
            'q' => "gdm spec list".into(),
            'r' => "defstruct".into(),
            'R' => "ROD object".into(),
            's' => "symbol".into(),
            'S' => "symbol | string".into(),
            't' => "string".into(),
            'u' => "function".into(),
            'U' => "function object".into(),
            'v' => "hierarchical db path".into(),
            'w' => "window".into(),
            'x' => "integer".into(),
            'y' => "binary function".into(),
            '&' => "pointer".into(),
            _ => format!("unknown({c})"),
        })
        .collect::<Vec<String>>()
        .join(" | ")
}
fn param(token: &str, optional: bool) -> Param {
    let (name, ty) = token
        .split_once('_')
        .map(|(p, n)| (n.to_string(), resolve_prefix(p)))
        .unwrap_or_else(|| (token.to_string(), "any".into()));
    Param {
        raw: token.into(),
        name,
        type_str: ty,
        optional,
    }
}
pub fn parse_signature(sig: &str) -> (Vec<Param>, Option<String>) {
    let mut raw = Vec::new();
    let mut ret = None;
    let mut in_optional = false;
    for line in sig.lines() {
        let line = line.trim();
        if let Some(r) = line.strip_prefix("=>") {
            let r = r.trim();
            let normalized = r.replace(' ', "").to_lowercase();
            if !["t/nil", "nil", "t", "", "void"].contains(&normalized.as_str()) {
                let first = r.split('/').next().unwrap_or("").trim();
                if let Some((p, n)) = first.split_once('_') {
                    ret = Some(format!("{} ({n})", resolve_prefix(p)));
                } else if !["t", "nil", ""].contains(&first) {
                    ret = Some(first.into());
                }
            }
            continue;
        }
        // Finder convention: first line is functionName(, subsequent lines hold parameters.
        if line.is_empty() || line == ")" || line.contains('(') {
            continue;
        }
        let opens = line.matches('[').count();
        let closes = line.matches(']').count();
        let optional = in_optional || opens > 0;
        let mut clean = String::new();
        let mut rest = line;
        while let Some(a) = rest.find('{') {
            clean.push_str(&rest[..a]);
            if let Some(b) = rest[a..].find('}') {
                rest = &rest[a + b + 1..];
            } else {
                rest = &rest[a..];
                break;
            }
        }
        clean.push_str(rest);
        let clean = clean.replace(['[', ']'], "");
        if clean.trim().is_empty() {
            in_optional = (in_optional || opens > 0) && closes == 0;
            continue;
        }
        let tokens: Vec<_> = clean
            .split_whitespace()
            .filter(|s| *s != "(" && *s != ")")
            .collect();
        if tokens.len() == 2 && tokens[0].starts_with('?') && tokens[1].contains('_') {
            let mut p = param(tokens[0], optional);
            p.name = tokens[0].into();
            p.type_str = param(tokens[1], optional).type_str;
            raw.push(p);
        } else {
            raw.extend(tokens.iter().map(|t| param(t, optional)));
        }
        if opens > closes {
            in_optional = true;
        } else if closes > opens {
            in_optional = false;
        }
    }
    let mut params = Vec::new();
    let mut i = 0;
    while i < raw.len() {
        let mut p = raw[i].clone();
        if p.raw.starts_with('?') && i + 1 < raw.len() {
            if let Some((_, base)) = raw[i + 1].raw.split_once('_') {
                if base
                    .trim_end_matches([')', ' '])
                    .eq_ignore_ascii_case(&p.raw[1..])
                {
                    p.name = p.raw.clone();
                    p.type_str = raw[i + 1].type_str.clone();
                    i += 1;
                }
            }
        }
        params.push(p);
        i += 1;
    }
    (params, ret)
}
fn name_prefix(name: &str) -> Option<&str> {
    let n = name.bytes().take_while(u8::is_ascii_lowercase).count();
    (n > 0 && name.as_bytes().get(n).is_some_and(u8::is_ascii_uppercase)).then(|| &name[..n])
}
fn file_category(path: &str) -> u8 {
    match path {
        "Custom_Layout/sklayoutref.fnd"
        | "Custom_Layout/vead.fnd"
        | "Custom_Layout/vvdrflow.fnd" => 4,
        "Pcells/skpcellref.fnd" => 5,
        "Virtuoso_ADE/maeSKILLref.fnd" => 6,
        "Relative_Object_Design/rodskillref.fnd" => 7,
        "OCEAN/oceanref.fnd" => 8,
        "Netlisting_and_Simulation/netlistsimulateref.fnd"
        | "Schematics/skcompref.fnd"
        | "VSVN/vsvn.fnd" => 9,
        _ => 10,
    }
}
fn category(name: &str, path: &str, core: &HashSet<String>) -> u8 {
    if Path::new(path).parent() == Some(Path::new("Core_SKILL")) {
        return if name_prefix(name).is_some() { 2 } else { 1 };
    }
    let mut prefixes: Vec<(&str, u8)> = vec![
        ("db", 3),
        ("dd", 3),
        ("dr", 3),
        ("get", 3),
        ("hnl", 3),
        ("le", 4),
        ("hi", 4),
        ("ge", 4),
        ("tech", 4),
        ("pc", 5),
        ("sch", 5),
        ("mae", 6),
        ("axl", 6),
        ("asi", 6),
        ("rod", 7),
    ];
    prefixes.extend(core.iter().map(|p| (p.as_str(), 2)));
    let lower = name.to_ascii_lowercase();
    let prefix_cat = prefixes
        .iter()
        .filter(|(p, _)| {
            lower.starts_with(*p)
                && name
                    .as_bytes()
                    .get(p.len())
                    .is_some_and(u8::is_ascii_uppercase)
        })
        .max_by_key(|(p, c)| (p.len(), std::cmp::Reverse(*c)))
        .map(|(_, c)| *c)
        .unwrap_or(10);
    file_category(path).min(prefix_cat)
}
/// Extract top-level records using the Python parser's quote and nesting rules.
/// Like the supplied Python parser, a backslash escapes the following character literally.
pub fn parse_fnd(text: &str, source: &str) -> std::result::Result<Vec<Entry>, String> {
    let mut depth = 0isize;
    let mut strings = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    let mut escaped = false;
    let mut entries = Vec::new();
    for ch in text.chars() {
        if quoted {
            if escaped {
                current.push(ch);
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                quoted = false;
                strings.push(std::mem::take(&mut current));
            } else {
                current.push(ch);
            }
            continue;
        }
        match ch {
            '"' => {
                quoted = true;
            }
            '(' => {
                if depth == 0 {
                    strings.clear();
                }
                depth += 1;
            }
            ')' => {
                depth -= 1;
                if depth == 0 && strings.len() >= 2 {
                    let name = strings[0].trim().to_string();
                    let sig = strings[1].trim().to_string();
                    let (params, return_type) = parse_signature(&sig);
                    entries.push(Entry {
                        name,
                        signature_raw: sig,
                        description: strings.get(2).map(|s| s.trim().into()).unwrap_or_default(),
                        params,
                        return_type,
                        source_file: source.into(),
                        keyword_category: 10,
                    });
                    strings.clear();
                }
            }
            _ => {}
        }
    }
    // Python silently ignores incomplete records and unmatched delimiters.
    Ok(entries)
}
pub fn load_dir(root: &Path) -> std::result::Result<Vec<Entry>, String> {
    if !root.is_dir() {
        eprintln!("[skill-lsp] Warning: {} is not a directory", root.display());
        return Ok(vec![]);
    }
    let mut paths = Vec::new();
    for item in WalkDir::new(root).follow_links(false) {
        let item = item.map_err(|e| e.to_string())?;
        if item.file_type().is_dir() {
            // pathlib.rglob yields matches in each directory before its children.
            for child in fs::read_dir(item.path()).map_err(|e| e.to_string())? {
                let child = child.map_err(|e| e.to_string())?;
                let path = child.path();
                if path.is_file() && path.extension().is_some_and(|x| x == "fnd") {
                    paths.push(path);
                }
            }
        }
    }
    let mut parsed = Vec::new();
    let mut core = HashSet::new();
    for path in paths {
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(e) => {
                eprintln!("[skill-lsp] Warning: {}: {e}", path.display());
                continue;
            }
        };
        let rel = path
            .strip_prefix(root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        let text = String::from_utf8_lossy(&bytes)
            .replace("\r\n", "\n")
            .replace('\r', "\n");
        if Path::new(&rel).parent() == Some(Path::new("Core_SKILL")) {
            // Match Python's quick regex scan of every quoted identifier, including descriptions.
            for (i, _) in text.match_indices('"') {
                let tail = &text[i + 1..];
                let n = tail
                    .bytes()
                    .take_while(|b| b.is_ascii_alphanumeric() || b"_?!*".contains(b))
                    .count();
                if n > 0
                    && tail.as_bytes()[0].is_ascii_alphabetic()
                    && tail.as_bytes().get(n) == Some(&b'"')
                {
                    if let Some(prefix) = name_prefix(&tail[..n]) {
                        core.insert(prefix.to_string());
                    }
                }
            }
        }
        parsed.push((rel, parse_fnd(&text, &path.to_string_lossy())?));
    }
    // Stable category ordering preserves first-discovered equal-category duplicates.
    parsed.sort_by_key(|(p, _)| {
        if Path::new(p).parent() == Some(Path::new("Core_SKILL")) {
            0
        } else {
            file_category(p)
        }
    });
    let mut db: BTreeMap<String, Entry> = BTreeMap::new();
    for (path, entries) in parsed {
        for mut e in entries {
            e.keyword_category = category(&e.name, &path, &core);
            if db
                .get(&e.name)
                .is_none_or(|old| e.keyword_category < old.keyword_category)
            {
                db.insert(e.name.clone(), e);
            }
        }
    }
    Ok(db.into_values().collect())
}
/// Immutable runtime index: O(1) exact lookup, O(log N + K) prefix lookup.
pub struct Database {
    pub entries: Vec<Entry>,
    exact: HashMap<String, usize>,
    sorted: Vec<(String, usize)>,
}
impl Database {
    pub fn new(entries: Vec<Entry>) -> Self {
        let exact = entries
            .iter()
            .enumerate()
            .map(|(i, e)| (e.name.clone(), i))
            .collect();
        let mut sorted: Vec<_> = entries
            .iter()
            .enumerate()
            .map(|(i, e)| (e.name.to_lowercase(), i))
            .collect();
        sorted.sort();
        Self {
            entries,
            exact,
            sorted,
        }
    }
    pub fn get(&self, name: &str) -> Option<&Entry> {
        self.exact.get(name).map(|&i| &self.entries[i])
    }
    pub fn prefix<'a>(&'a self, prefix: &str) -> impl Iterator<Item = &'a Entry> {
        let p = prefix.to_lowercase();
        let start = self
            .sorted
            .partition_point(|(name, _)| name.as_str() < p.as_str());
        self.sorted[start..]
            .iter()
            .take_while(move |(name, _)| name.starts_with(p.as_str()))
            .map(|(_, i)| &self.entries[*i])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_category_rules_and_camelcase_boundaries() {
        let core = HashSet::from(["str".to_owned()]);
        for (name, path, want) in [
            ("plain", "Core_SKILL/core.fnd", 1),
            ("strOther", "misc.fnd", 2),
            ("dbOther", "misc.fnd", 3),
            ("techOther", "misc.fnd", 4),
            ("pcOther", "misc.fnd", 5),
            ("schOther", "misc.fnd", 5),
            ("axlOther", "misc.fnd", 6),
            ("rodOther", "misc.fnd", 7),
            ("plain", "OCEAN/oceanref.fnd", 8),
            ("plain", "VSVN/vsvn.fnd", 9),
            ("plain", "misc.fnd", 10),
            ("dbother", "misc.fnd", 10),
            ("get", "misc.fnd", 10),
            ("getOther", "OCEAN/oceanref.fnd", 3),
            ("plain", "Pcells/skpcellref.fnd", 5),
            ("plain", "Virtuoso_ADE/maeSKILLref.fnd", 6),
            ("plain", "Relative_Object_Design/rodskillref.fnd", 7),
            (
                "plain",
                "Netlisting_and_Simulation/netlistsimulateref.fnd",
                9,
            ),
            ("plain", "Schematics/skcompref.fnd", 9),
        ] {
            assert_eq!(category(name, path, &core), want, "{name} in {path}");
        }
    }
}
