//! Kimix resource URL scheme.
//!
//! Provides a unified resource addressing mechanism similar to OpenMinis's
//! `minis://` scheme. Tool outputs can reference persistent resources via
//! `kimix://` URLs, enabling multi-turn resource references.
//!
//! # Format
//!
//! ```text
//! kimix://<namespace>/<path>
//! ```
//!
//! # Namespaces
//!
//! - `workspace` — Working files (scripts, data, configs)
//! - `output` — Tool-generated artifacts (reports, charts)
//! - `memory` — Persistent memory across sessions
//!
//! # Examples
//!
//! ```text
//! kimix://workspace/report.md
//! kimix://output/chart.png
//! kimix://memory/notes.json
//! ```

use std::fmt;

/// Kimix resource URL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KimixUrl {
    /// Namespace (workspace, output, memory).
    pub namespace: String,
    /// Path within the namespace.
    pub path: String,
}

impl KimixUrl {
    /// Create a new Kimix URL.
    pub fn new(namespace: impl Into<String>, path: impl Into<String>) -> Self {
        Self {
            namespace: namespace.into(),
            path: path.into(),
        }
    }

    /// Create a workspace URL.
    pub fn workspace(path: impl Into<String>) -> Self {
        Self::new("workspace", path)
    }

    /// Create an output URL.
    pub fn output(path: impl Into<String>) -> Self {
        Self::new("output", path)
    }

    /// Create a memory URL.
    pub fn memory(path: impl Into<String>) -> Self {
        Self::new("memory", path)
    }

    /// Parse a kimix:// URL string.
    ///
    /// Returns `None` if the string is not a valid kimix:// URL.
    pub fn parse(url: &str) -> Option<Self> {
        let url = url.strip_prefix("kimix://")?;
        let (namespace, path) = url.split_once('/')?;
        if namespace.is_empty() || path.is_empty() {
            return None;
        }
        Some(Self {
            namespace: namespace.to_string(),
            path: path.to_string(),
        })
    }

    /// Check if this is a valid kimix:// URL string.
    pub fn is_valid_url(url: &str) -> bool {
        Self::parse(url).is_some()
    }

    /// Convert to the tool output hint format.
    ///
    /// Returns a string like `kimix_resource: kimix://workspace/report.md`
    /// that can be appended to tool outputs.
    pub fn to_resource_hint(&self) -> String {
        format!("kimix_resource: {}", self)
    }
}

impl fmt::Display for KimixUrl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "kimix://{}/{}", self.namespace, self.path)
    }
}

impl std::str::FromStr for KimixUrl {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s).ok_or_else(|| format!("invalid kimix URL: {}", s))
    }
}

/// Append a kimix:// resource hint to a tool output string.
///
/// If the path is within a known workspace directory, appends a
/// `kimix_resource:` line to the output.
pub fn append_resource_hint(
    output: &str,
    workspace_root: &std::path::Path,
    file_path: &std::path::Path,
) -> String {
    if let Ok(relative) = file_path.strip_prefix(workspace_root) {
        let url = KimixUrl::workspace(relative.to_string_lossy());
        format!("{}\n{}", output, url.to_resource_hint())
    } else {
        output.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kimix_url_display() {
        let url = KimixUrl::workspace("report.md");
        assert_eq!(url.to_string(), "kimix://workspace/report.md");
    }

    #[test]
    fn test_kimix_url_parse() {
        let url = KimixUrl::parse("kimix://workspace/report.md").unwrap();
        assert_eq!(url.namespace, "workspace");
        assert_eq!(url.path, "report.md");
    }

    #[test]
    fn test_kimix_url_parse_invalid() {
        assert!(KimixUrl::parse("not-a-url").is_none());
        assert!(KimixUrl::parse("kimix://").is_none());
        assert!(KimixUrl::parse("kimix:///path").is_none());
    }

    #[test]
    fn test_kimix_url_from_str() {
        let url: KimixUrl = "kimix://output/chart.png".parse().unwrap();
        assert_eq!(url.namespace, "output");
        assert_eq!(url.path, "chart.png");
    }

    #[test]
    fn test_resource_hint() {
        let url = KimixUrl::workspace("report.md");
        assert_eq!(
            url.to_resource_hint(),
            "kimix_resource: kimix://workspace/report.md"
        );
    }

    #[test]
    fn test_append_resource_hint() {
        let workspace = std::path::Path::new("/home/user/project");
        let file = std::path::Path::new("/home/user/project/src/main.rs");
        let output = "File written successfully";
        let result = append_resource_hint(output, workspace, file);
        assert!(result.contains("kimix_resource: kimix://workspace/src/main.rs"));
    }

    #[test]
    fn test_append_resource_hint_outside_workspace() {
        let workspace = std::path::Path::new("/home/user/project");
        let file = std::path::Path::new("/tmp/other.rs");
        let output = "File written successfully";
        let result = append_resource_hint(output, workspace, file);
        assert_eq!(result, output);
    }
}
