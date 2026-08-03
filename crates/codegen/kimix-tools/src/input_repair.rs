//! 工具输入修复层（validate-then-repair）。
//!
//! 开源模型在工具调用上有一组有限、可复现的形状失败。本模块只在
//! **反序列化已失败** 时，按 serde 报错路径与 expected 类型做定点修复。
//!
//! # 原则
//!
//! - 合法输入永不触碰（先 parse，失败才修）。
//! - 只改与报错路径相关的值；无路径时仅做语义守恒的 markdown 退化链接解包。
//! - 依赖 expected 类型约束危险转换（裸字符串 → 数组），避免把 `path: "foo"`
//!   误包成 `["foo"]`。

use std::borrow::Cow;

use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::input_repair_telemetry;

/// 单次修复结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RepairOutcome {
    /// 原样解析成功，未修改。
    NoChangeNeeded,
    /// 应用了至少一个修复。
    Repaired(Vec<RepairKind>),
    /// 无法修复（或修复后仍无效）。
    Unrepairable,
}

/// 应用的修复类型（遥测聚合用）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RepairKind {
    /// 可选字段的 `null` 被移除（字段删除，供 `Option`/`default` 吃缺省）。
    NullRemoved,
    /// 字符串形式的 JSON 数组被解析为真实数组。
    StringifiedArrayParsed,
    /// 单参数包在 `{}` 中、schema 期望数组 → 解包为数组。
    ObjectPlaceholderUnwrapped,
    /// 裸字符串被包裹为单元素数组（仅当 expected 为 sequence）。
    BareStringWrapped,
    /// markdown 退化自动链接被解包为纯路径。
    MarkdownLinkUnwrapped,
}

/// 来自 serde 报错的修复提示。
#[derive(Debug, Clone, Default)]
pub struct RepairHint {
    /// serde_path 形态：`patterns`、`args.list`、`items[0]`；`"."` / 空视为顶层。
    pub path: Option<String>,
    /// 从报错信息提取的 expected 片段，如 `a sequence`。
    pub expected: Option<String>,
}

/// 尝试按 hint 修复工具参数 JSON。
pub fn repair_input(raw: &Value, hint: &RepairHint) -> (Value, RepairOutcome) {
    let mut repaired = raw.clone();
    let mut applied = Vec::new();

    let path = hint
        .path
        .as_deref()
        .map(str::trim)
        .filter(|p| !p.is_empty() && *p != ".");

    if let Some(path) = path {
        let tokens = tokenize_serde_path(path);
        if let Some(kind) = repair_at_path(&mut repaired, &tokens, hint.expected.as_deref()) {
            applied.push(kind);
        }
    } else {
        // 无字段路径：只做全局语义守恒修复（退化 markdown 链接）。
        fix_markdown_links_in_value(&mut repaired, &mut applied);
    }

    // 路径定点修完后，再扫一遍退化 markdown（路径字段常被包进更长字符串）。
    if path.is_some() {
        let before = applied.len();
        fix_markdown_links_in_value(&mut repaired, &mut applied);
        // 去重：同 kind 多次只保留一次出现顺序中的首次
        if applied.len() > before {
            dedup_kinds(&mut applied);
        }
    }

    if applied.is_empty() {
        (repaired, RepairOutcome::Unrepairable)
    } else {
        (repaired, RepairOutcome::Repaired(applied))
    }
}

/// 统一入口：path-aware 反序列化 → 失败则修复 → 再反序列化。
///
/// 返回 `(typed, Some(kinds))` 表示发生了修复；`None` 表示原样成功。
/// 遥测在此记录，调用方无需再记。
pub fn deserialize_with_repair<T: DeserializeOwned>(
    json: Value,
    tool_name: &str,
) -> Result<(T, Option<Vec<RepairKind>>), serde_json::Error> {
    match serde_path_to_error::deserialize::<_, T>(json.clone()) {
        Ok(typed) => Ok((typed, None)),
        Err(first_err) => {
            let path = first_err.path().to_string();
            let message = first_err.inner().to_string();
            let hint = RepairHint {
                path: normalize_path_opt(&path),
                expected: extract_expected(&message),
            };
            let (repaired, outcome) = repair_input(&json, &hint);
            match outcome {
                RepairOutcome::Repaired(kinds) => match serde_json::from_value::<T>(repaired) {
                    Ok(typed) => {
                        input_repair_telemetry::metrics().record(
                            tool_name,
                            "",
                            "repaired",
                            &kinds,
                        );
                        Ok((typed, Some(kinds)))
                    }
                    Err(second_err) => {
                        input_repair_telemetry::metrics().record(
                            tool_name,
                            "",
                            "invalid",
                            &[],
                        );
                        Err(second_err)
                    }
                },
                RepairOutcome::NoChangeNeeded | RepairOutcome::Unrepairable => {
                    input_repair_telemetry::metrics().record(tool_name, "", "invalid", &[]);
                    Err(first_err.into_inner())
                }
            }
        }
    }
}

fn normalize_path_opt(path: &str) -> Option<String> {
    let p = path.trim();
    if p.is_empty() || p == "." {
        None
    } else {
        Some(p.to_string())
    }
}

/// 从 serde 报错信息提取 expected 片段。
pub fn extract_expected(message: &str) -> Option<String> {
    let marker = ", expected ";
    message
        .split_once(marker)
        .map(|(_, expected)| expected.trim_end_matches('.').to_owned())
}

fn expected_is_sequence(expected: Option<&str>) -> bool {
    let Some(e) = expected else {
        // 无 expected 时：仅允许安全修复（stringified array / markdown / null），
        // 不允许 bare-string-wrap。
        return false;
    };
    let e = e.to_ascii_lowercase();
    e.contains("sequence") || e.contains("array") || e.contains("list")
}

/// 将 `a.b[0].c` / `a.b` 解析为 path token。
fn tokenize_serde_path(path: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut key = String::new();
    let mut chars = path.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '.' => {
                if !key.is_empty() {
                    tokens.push(std::mem::take(&mut key));
                }
            }
            '[' => {
                if !key.is_empty() {
                    tokens.push(std::mem::take(&mut key));
                }
                let mut index = String::new();
                for next in chars.by_ref() {
                    if next == ']' {
                        break;
                    }
                    index.push(next);
                }
                if !index.is_empty() {
                    tokens.push(index);
                }
            }
            _ => key.push(ch),
        }
    }
    if !key.is_empty() {
        tokens.push(key);
    }
    tokens
}

fn repair_at_path(
    root: &mut Value,
    tokens: &[String],
    expected: Option<&str>,
) -> Option<RepairKind> {
    if tokens.is_empty() {
        return None;
    }

    // null 叶子：从父对象删除字段，而不是「标记但不改值」。
    // split_last → (最后一个 token = 字段名, 前面 = 父路径)
    if let Some((field, parent_tokens)) = tokens.split_last() {
        if parent_tokens.is_empty() {
            if let Value::Object(map) = root {
                if map.get(field).is_some_and(|v| v.is_null()) {
                    map.remove(field);
                    return Some(RepairKind::NullRemoved);
                }
            }
        } else if let Some(parent) = value_at_path(root, parent_tokens) {
            if let Value::Object(map) = parent {
                if map.get(field).is_some_and(|v| v.is_null()) {
                    map.remove(field);
                    return Some(RepairKind::NullRemoved);
                }
            }
        }
    }

    let target = value_at_path(root, tokens)?;
    repair_value_at(target, expected)
}

fn repair_value_at(value: &mut Value, expected: Option<&str>) -> Option<RepairKind> {
    // 1. 对象上残留的 null 字段（路径指到对象本身时）
    if let Some(kind) = strip_null_fields(value) {
        return Some(kind);
    }
    // 2. markdown 退化链接（短路：像链接但非退化 → 不再 bare-wrap）
    if looks_like_markdown_link(value) {
        return fix_markdown_link(value);
    }
    // 3. 字符串 JSON 数组 → 真数组（语义明确，不依赖 expected）
    if let Some(kind) = fix_stringified_array(value) {
        return Some(kind);
    }
    // 4/5. 仅当 expected 为 sequence 时：object 占位 / 裸字符串
    if expected_is_sequence(expected) {
        if let Some(kind) = fix_object_placeholder(value) {
            return Some(kind);
        }
        if let Some(kind) = fix_bare_string(value) {
            return Some(kind);
        }
    }
    None
}

fn strip_null_fields(value: &mut Value) -> Option<RepairKind> {
    let Value::Object(map) = value else {
        return None;
    };
    let mut removed = false;
    map.retain(|_, v| {
        if v.is_null() {
            removed = true;
            false
        } else {
            true
        }
    });
    removed.then_some(RepairKind::NullRemoved)
}

fn looks_like_markdown_link(value: &Value) -> bool {
    let Value::String(s) = value else {
        return false;
    };
    s.contains("](")
        && (s.contains("http://") || s.contains("https://") || s.contains("file://"))
}

fn fix_stringified_array(value: &mut Value) -> Option<RepairKind> {
    let Value::String(s) = value else {
        return None;
    };
    let trimmed = s.trim();
    if !trimmed.starts_with('[') || !trimmed.ends_with(']') {
        return None;
    }
    match serde_json::from_str::<Value>(trimmed) {
        Ok(Value::Array(arr)) => {
            *value = Value::Array(arr);
            Some(RepairKind::StringifiedArrayParsed)
        }
        _ => None,
    }
}

fn fix_object_placeholder(value: &mut Value) -> Option<RepairKind> {
    let Value::Object(map) = value else {
        return None;
    };
    if map.is_empty() {
        *value = Value::Array(Vec::new());
        return Some(RepairKind::ObjectPlaceholderUnwrapped);
    }
    if map.len() == 1 {
        let (k, v) = map.iter().next()?;
        if k.is_empty() || k.parse::<usize>().is_ok() {
            let inner = v.clone();
            *value = Value::Array(vec![inner]);
            return Some(RepairKind::ObjectPlaceholderUnwrapped);
        }
    }
    None
}

fn fix_bare_string(value: &mut Value) -> Option<RepairKind> {
    let Value::String(s) = value else {
        return None;
    };
    *value = Value::Array(vec![Value::String(s.clone())]);
    Some(RepairKind::BareStringWrapped)
}

fn fix_markdown_link(value: &mut Value) -> Option<RepairKind> {
    let Value::String(s) = value else {
        return None;
    };
    let unwrapped = unwrap_degenerate_markdown_link(s)?;
    *value = Value::String(unwrapped);
    Some(RepairKind::MarkdownLinkUnwrapped)
}

fn fix_markdown_links_in_value(value: &mut Value, applied: &mut Vec<RepairKind>) {
    match value {
        Value::String(s) => {
            if let Some(unwrapped) = unwrap_degenerate_markdown_link(s) {
                *s = unwrapped;
                applied.push(RepairKind::MarkdownLinkUnwrapped);
            }
        }
        Value::Array(arr) => {
            for item in arr {
                fix_markdown_links_in_value(item, applied);
            }
        }
        Value::Object(map) => {
            for v in map.values_mut() {
                fix_markdown_links_in_value(v, applied);
            }
        }
        _ => {}
    }
}

/// 路径字段用：若整段是退化 markdown 链接（或路径中嵌有），解包为纯路径。
/// 反序列化不会因 markdown 链接失败（仍是合法 string），故须在工具执行前调用。
pub fn sanitize_path_string(path: &str) -> Cow<'_, str> {
    match unwrap_degenerate_markdown_link(path) {
        Some(s) => Cow::Owned(s),
        None => Cow::Borrowed(path),
    }
}

/// 解包退化 markdown 链接：`[text](url)` 且 text == 去协议 url。
fn unwrap_degenerate_markdown_link(s: &str) -> Option<String> {
    let s = s.trim();
    let start = s.find('[')?;
    let bracket_end = s[start + 1..].find(']')? + start + 1;
    if !s[bracket_end..].starts_with("](") {
        return None;
    }
    let paren_start = bracket_end + 2;
    let paren_end_rel = s[paren_start..].find(')')?;
    let paren_end = paren_start + paren_end_rel;
    let before = &s[..start];
    let after = &s[paren_end + 1..];
    let text = &s[start + 1..bracket_end];
    let url = &s[paren_start..paren_end];
    if is_degenerate_link(text, url) {
        Some(format!("{before}{}{after}", text.trim()))
    } else {
        None
    }
}

fn is_degenerate_link(text: &str, url: &str) -> bool {
    if text.is_empty() || url.is_empty() {
        return false;
    }
    let url_no_proto = url
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_start_matches("file://");
    let text_normalized = text.trim();
    text_normalized == url_no_proto
        || text_normalized.trim_start_matches("www.")
            == url_no_proto.trim_start_matches("www.")
        || text_normalized == url_no_proto.trim_start_matches('/')
}

fn value_at_path<'a>(root: &'a mut Value, tokens: &[String]) -> Option<&'a mut Value> {
    let mut current = root;
    for token in tokens {
        match current {
            Value::Object(map) => {
                current = map.get_mut(token)?;
            }
            Value::Array(arr) => {
                let idx = token.parse::<usize>().ok()?;
                current = arr.get_mut(idx)?;
            }
            _ => return None,
        }
    }
    Some(current)
}

fn dedup_kinds(kinds: &mut Vec<RepairKind>) {
    let mut seen = Vec::new();
    kinds.retain(|k| {
        if seen.contains(k) {
            false
        } else {
            seen.push(*k);
            true
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;
    use serde_json::json;

    #[derive(Debug, Deserialize, PartialEq)]
    struct PathsArgs {
        paths: Vec<String>,
    }

    #[derive(Debug, Deserialize, PartialEq)]
    struct ReadArgs {
        path: String,
        #[serde(default)]
        limit: Option<usize>,
        #[serde(default)]
        offset: Option<i64>,
    }

    #[test]
    fn null_optional_field_is_removed() {
        let raw = json!({ "path": "/tmp/x", "limit": null });
        let hint = RepairHint {
            path: Some("limit".into()),
            expected: Some("u64".into()),
        };
        let (out, outcome) = repair_input(&raw, &hint);
        assert_eq!(
            outcome,
            RepairOutcome::Repaired(vec![RepairKind::NullRemoved])
        );
        assert!(out.get("limit").is_none(), "null field must be deleted");
        let typed: ReadArgs = serde_json::from_value(out).unwrap();
        assert_eq!(typed.limit, None);
    }

    #[test]
    fn stringified_array_is_parsed() {
        let raw = json!({ "paths": "[\"a\",\"b\"]" });
        let hint = RepairHint {
            path: Some("paths".into()),
            expected: Some("a sequence".into()),
        };
        let (out, outcome) = repair_input(&raw, &hint);
        assert!(matches!(
            outcome,
            RepairOutcome::Repaired(kinds) if kinds.contains(&RepairKind::StringifiedArrayParsed)
        ));
        assert_eq!(out["paths"], json!(["a", "b"]));
    }

    #[test]
    fn bare_string_wrapped_only_when_sequence_expected() {
        let raw = json!({ "paths": "foo" });
        let with_seq = RepairHint {
            path: Some("paths".into()),
            expected: Some("a sequence".into()),
        };
        let (out, outcome) = repair_input(&raw, &with_seq);
        assert!(matches!(
            outcome,
            RepairOutcome::Repaired(kinds) if kinds.contains(&RepairKind::BareStringWrapped)
        ));
        assert_eq!(out["paths"], json!(["foo"]));

        // 无 sequence expected：不把字符串 path 误包成数组
        let as_string_field = json!({ "path": "foo" });
        let no_seq = RepairHint {
            path: Some("path".into()),
            expected: Some("a string".into()),
        };
        let (out2, outcome2) = repair_input(&as_string_field, &no_seq);
        assert_eq!(outcome2, RepairOutcome::Unrepairable);
        assert_eq!(out2["path"], json!("foo"));
    }

    #[test]
    fn object_placeholder_unwrapped_for_sequence() {
        let raw = json!({ "paths": { "0": "/tmp/a" } });
        let hint = RepairHint {
            path: Some("paths".into()),
            expected: Some("a sequence".into()),
        };
        let (out, outcome) = repair_input(&raw, &hint);
        assert!(matches!(
            outcome,
            RepairOutcome::Repaired(kinds)
                if kinds.contains(&RepairKind::ObjectPlaceholderUnwrapped)
        ));
        assert_eq!(out["paths"], json!(["/tmp/a"]));
    }

    #[test]
    fn empty_object_placeholder_becomes_empty_array() {
        let raw = json!({ "paths": {} });
        let hint = RepairHint {
            path: Some("paths".into()),
            expected: Some("a sequence".into()),
        };
        let (out, outcome) = repair_input(&raw, &hint);
        assert!(matches!(
            outcome,
            RepairOutcome::Repaired(kinds)
                if kinds.contains(&RepairKind::ObjectPlaceholderUnwrapped)
        ));
        assert_eq!(out["paths"], json!([]));
    }

    #[test]
    fn stringified_array_before_bare_string() {
        let raw = json!({ "paths": "[\"a\",\"b\"]" });
        let hint = RepairHint {
            path: Some("paths".into()),
            expected: Some("a sequence".into()),
        };
        let (out, outcome) = repair_input(&raw, &hint);
        assert!(matches!(
            outcome,
            RepairOutcome::Repaired(kinds) if kinds.contains(&RepairKind::StringifiedArrayParsed)
        ));
        assert_eq!(out["paths"], json!(["a", "b"]));
    }

    #[test]
    fn markdown_degenerate_link_unwrapped() {
        let raw = json!({ "path": "/Users/x/proj/[notes.md](http://notes.md)" });
        let hint = RepairHint {
            path: Some("path".into()),
            expected: Some("a string".into()),
        };
        let (out, outcome) = repair_input(&raw, &hint);
        assert!(matches!(
            outcome,
            RepairOutcome::Repaired(kinds) if kinds.contains(&RepairKind::MarkdownLinkUnwrapped)
        ));
        assert_eq!(out["path"], "/Users/x/proj/notes.md");
    }

    #[test]
    fn real_markdown_link_not_unwrapped() {
        let raw = json!({ "path": "[click](https://example.com)" });
        let hint = RepairHint {
            path: Some("path".into()),
            expected: Some("a string".into()),
        };
        let (out, _) = repair_input(&raw, &hint);
        assert_eq!(out["path"], "[click](https://example.com)");
    }

    #[test]
    fn global_markdown_without_path() {
        let raw = json!({ "file_path": "/x/[a.md](http://a.md)" });
        let (out, outcome) = repair_input(&raw, &RepairHint::default());
        assert!(matches!(
            outcome,
            RepairOutcome::Repaired(kinds) if kinds.contains(&RepairKind::MarkdownLinkUnwrapped)
        ));
        assert_eq!(out["file_path"], "/x/a.md");
    }

    #[test]
    fn valid_input_untouched_without_path_or_markdown() {
        let raw = json!({ "a": 1, "b": [1, 2] });
        let (out, outcome) = repair_input(&raw, &RepairHint::default());
        assert_eq!(out, raw);
        assert_eq!(outcome, RepairOutcome::Unrepairable);
    }

    #[test]
    fn nested_path_repair() {
        let raw = json!({ "args": { "list": "[\"x\"]" } });
        let hint = RepairHint {
            path: Some("args.list".into()),
            expected: Some("a sequence".into()),
        };
        let (out, outcome) = repair_input(&raw, &hint);
        assert!(matches!(
            outcome,
            RepairOutcome::Repaired(kinds) if kinds.contains(&RepairKind::StringifiedArrayParsed)
        ));
        assert_eq!(out["args"]["list"], json!(["x"]));
    }

    #[test]
    fn tokenize_handles_index_and_dots() {
        assert_eq!(
            tokenize_serde_path("items[0].name"),
            vec!["items", "0", "name"]
        );
        assert_eq!(tokenize_serde_path("args.list"), vec!["args", "list"]);
    }

    #[test]
    fn deserialize_with_repair_end_to_end_bare_string() {
        let raw = json!({ "paths": "foo" });
        let (typed, kinds) =
            deserialize_with_repair::<PathsArgs>(raw, "test:paths").expect("repaired");
        assert_eq!(typed.paths, vec!["foo".to_string()]);
        assert!(kinds.is_some());
    }

    #[test]
    fn deserialize_with_repair_end_to_end_null_option() {
        // 标准 `Option` 本身接受 JSON null → 可能无需 repair；关键是 limit=None。
        // 真正拒绝 null 的字段见 `null_optional_field_is_removed`（显式 path 修复）。
        let raw = json!({ "path": "/tmp/x", "limit": null });
        let (typed, _) =
            deserialize_with_repair::<ReadArgs>(raw, "test:read").expect("ok");
        assert_eq!(typed.path, "/tmp/x");
        assert_eq!(typed.limit, None);
    }

    #[derive(Debug, Deserialize, PartialEq)]
    struct StrictLimit {
        path: String,
        limit: usize,
    }

    #[test]
    fn deserialize_with_repair_null_on_required_numeric_is_unrepairable() {
        // 必填 usize + null：删字段后仍缺字段，无法修成合法值 → invalid。
        let raw = json!({ "path": "/tmp/x", "limit": null });
        let err = deserialize_with_repair::<StrictLimit>(raw, "test:strict");
        assert!(err.is_err());
    }

    #[test]
    fn deserialize_with_repair_clean_input() {
        let raw = json!({ "path": "/tmp/x", "limit": 10 });
        let (typed, kinds) =
            deserialize_with_repair::<ReadArgs>(raw, "test:read").expect("clean");
        assert_eq!(typed.limit, Some(10));
        assert!(kinds.is_none());
    }

    #[test]
    fn unwrap_degenerate_link_variants() {
        assert_eq!(
            unwrap_degenerate_markdown_link("[notes.md](http://notes.md)"),
            Some("notes.md".to_string())
        );
        assert_eq!(
            unwrap_degenerate_markdown_link("[click](https://example.com)"),
            None
        );
        assert_eq!(
            unwrap_degenerate_markdown_link("/x/[b.md](http://b.md)/tail"),
            Some("/x/b.md/tail".to_string())
        );
    }

    #[test]
    fn extract_expected_from_serde_message() {
        let msg = "invalid type: string \"foo\", expected a sequence";
        assert_eq!(extract_expected(msg).as_deref(), Some("a sequence"));
    }
}
