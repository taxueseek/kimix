//! Single-file symbol outline via tree-sitter definition queries.
//!
//! Lightweight alternative to full workspace indexing / LSP `documentSymbol`:
//! parse one source file and list definition names with kind + 1-based line.
use std::fmt;
use std::path::Path;
use std::sync::Arc;

use once_cell::sync::Lazy;
use tree_sitter::StreamingIterator;

use crate::languages::LanguageRegistry;
use crate::MAX_INDEXABLE_FILE_SIZE;

static LANG_REGISTRY: Lazy<LanguageRegistry> = Lazy::new(LanguageRegistry::new);

/// Soft cap so a pathological file cannot flood the model context.
pub const MAX_OUTLINE_ENTRIES: usize = 400;

/// One definition in a file outline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutlineEntry {
    /// Symbol kind from the tree-sitter capture (e.g. `function`, `class`, `method`).
    pub kind: Arc<str>,
    /// Definition name as it appears in source.
    pub name: Arc<str>,
    /// 1-based line number.
    pub line: usize,
}

/// Errors from outline extraction.
#[derive(Debug)]
pub enum OutlineError {
    UnsupportedLanguage,
    FileTooLarge,
    EmptyFile,
    Io(std::io::Error),
    BinaryOrInvalidUtf8,
    Query(String),
    Language(String),
    ParseFailed,
}

impl fmt::Display for OutlineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedLanguage => write!(
                f,
                "unsupported language for path (known: rust, go, python, ts/js)"
            ),
            Self::FileTooLarge => {
                write!(f, "file too large (>{MAX_INDEXABLE_FILE_SIZE} bytes)")
            }
            Self::EmptyFile => write!(f, "empty file"),
            Self::Io(e) => write!(f, "io: {e}"),
            Self::BinaryOrInvalidUtf8 => write!(f, "binary or non-utf8 content"),
            Self::Query(e) => write!(f, "tree-sitter query: {e}"),
            Self::Language(e) => write!(f, "tree-sitter language: {e}"),
            Self::ParseFailed => write!(f, "parse failed"),
        }
    }
}

impl std::error::Error for OutlineError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for OutlineError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

/// Extract definition outline from an already-read source buffer.
pub fn outline_source(path: &Path, src: &[u8]) -> Result<Vec<OutlineEntry>, OutlineError> {
    if src.is_empty() {
        return Err(OutlineError::EmptyFile);
    }
    if src.len() as u64 > MAX_INDEXABLE_FILE_SIZE {
        return Err(OutlineError::FileTooLarge);
    }
    if crate::is_binary_content(src) {
        return Err(OutlineError::BinaryOrInvalidUtf8);
    }

    let lang = LANG_REGISTRY
        .for_file_path(path)
        .ok_or(OutlineError::UnsupportedLanguage)?;

    let query = lang
        .compile_query()
        .map_err(|e| OutlineError::Query(e.to_string()))?;

    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&lang.language())
        .map_err(|e| OutlineError::Language(e.to_string()))?;

    let tree = parser.parse(src, None).ok_or(OutlineError::ParseFailed)?;
    let root = tree.root_node();

    let capture_names = query.capture_names();
    // Map capture index → kind string for definition captures only.
    let mut def_kind: Vec<Option<Arc<str>>> = vec![None; capture_names.len()];
    for (i, name) in capture_names.iter().enumerate() {
        if let Some(rest) = name.strip_prefix("name.definition.") {
            def_kind[i] = Some(Arc::<str>::from(rest));
        }
    }

    let mut entries: Vec<OutlineEntry> = Vec::with_capacity(64);
    let mut cursor = tree_sitter::QueryCursor::new();
    let mut matches = cursor.matches(&query, root, src);

    while let Some(match_) = matches.next() {
        for capture in match_.captures {
            let idx = capture.index as usize;
            let Some(kind) = def_kind.get(idx).and_then(|k| k.as_ref()) else {
                continue;
            };
            let node = capture.node;
            let byte_range = node.byte_range();
            if byte_range.end > src.len() || byte_range.start >= byte_range.end {
                continue;
            }
            let name: Arc<str> = String::from_utf8_lossy(&src[byte_range]).into();
            if name.is_empty() {
                continue;
            }
            entries.push(OutlineEntry {
                kind: Arc::clone(kind),
                name,
                line: node.start_position().row + 1,
            });
            if entries.len() >= MAX_OUTLINE_ENTRIES {
                return Ok(dedupe_sort(entries));
            }
        }
    }

    Ok(dedupe_sort(entries))
}

/// Read `path` from disk and extract its definition outline.
pub fn outline_file(path: &Path) -> Result<Vec<OutlineEntry>, OutlineError> {
    let meta = std::fs::metadata(path)?;
    if meta.len() == 0 {
        return Err(OutlineError::EmptyFile);
    }
    if meta.len() > MAX_INDEXABLE_FILE_SIZE {
        return Err(OutlineError::FileTooLarge);
    }
    let src = std::fs::read(path)?;
    outline_source(path, &src)
}

/// Stable sort by line, then kind, then name; drop exact duplicates.
fn dedupe_sort(mut entries: Vec<OutlineEntry>) -> Vec<OutlineEntry> {
    entries.sort_by(|a, b| {
        a.line
            .cmp(&b.line)
            .then_with(|| a.kind.cmp(&b.kind))
            .then_with(|| a.name.cmp(&b.name))
    });
    entries.dedup_by(|a, b| a.line == b.line && a.kind == b.kind && a.name == b.name);
    entries
}

/// Render outline for the model / CLI (compact, line-oriented).
pub fn format_outline(display_path: &str, language_id: &str, entries: &[OutlineEntry]) -> String {
    if entries.is_empty() {
        return format!("outline: {display_path} ({language_id})\n(no definitions found)");
    }
    let mut out = String::with_capacity(entries.len() * 40 + 64);
    out.push_str(&format!(
        "outline: {display_path} ({language_id}) — {} definition(s)\n",
        entries.len()
    ));
    // Align kinds to a modest column for scanability.
    let kind_w = entries
        .iter()
        .map(|e| e.kind.len())
        .max()
        .unwrap_or(8)
        .min(16);
    for e in entries {
        out.push_str(&format!(
            "L{:<5} {:width$} {}\n",
            e.line,
            e.kind.as_ref(),
            e.name.as_ref(),
            width = kind_w
        ));
    }
    while out.ends_with('\n') {
        out.pop();
    }
    out
}

/// Primary language id string for formatting.
pub fn language_label_for_path(path: &Path) -> String {
    LANG_REGISTRY
        .for_file_path(path)
        .map(|c| c.primary_language_id().to_string())
        .unwrap_or_else(|| "unknown".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn outline_rust_extracts_fn_and_struct() {
        let src = br#"
pub struct Foo {
    x: i32,
}

impl Foo {
    pub fn bar(&self) {}
}

fn free() {}
"#;
        let path = PathBuf::from("sample.rs");
        let entries = outline_source(&path, src).expect("outline");
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_ref()).collect();
        assert!(
            names.iter().any(|n| *n == "Foo"),
            "expected struct Foo, got {names:?}"
        );
        assert!(
            names.iter().any(|n| *n == "bar" || *n == "free"),
            "expected methods/fns, got {names:?}"
        );
        assert!(entries.iter().all(|e| e.line >= 1));
    }

    #[test]
    fn outline_unsupported_ext() {
        let err = outline_source(Path::new("x.md"), b"# hi").unwrap_err();
        assert!(matches!(err, OutlineError::UnsupportedLanguage));
    }

    #[test]
    fn outline_empty() {
        let err = outline_source(Path::new("x.rs"), b"").unwrap_err();
        assert!(matches!(err, OutlineError::EmptyFile));
    }

    #[test]
    fn format_outline_lists_lines() {
        let entries = vec![
            OutlineEntry {
                kind: "function".into(),
                name: "main".into(),
                line: 10,
            },
            OutlineEntry {
                kind: "class".into(),
                name: "App".into(),
                line: 3,
            },
        ];
        let s = format_outline("src/main.rs", "rust", &entries);
        assert!(s.contains("L10"));
        assert!(s.contains("main"));
        assert!(s.contains("outline: src/main.rs"));
    }
}
