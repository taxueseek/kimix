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
    /// 数字字段收到数字字符串（`"42"`）→ 解析为数字。
    CoerceStringToNumber,
    /// 布尔字段收到 `"true"` / `"false"` → 解析为布尔。
    CoerceStringToBoolean,
    /// 数组字段收到非容器标量（数字/布尔）→ 包装为单元素数组（仅当 expected 为 sequence）。
    WrapBareScalarAsArray,
    /// 路径字段收到完整 `[text](url)` markdown 链接 → 提取 url 为纯路径。
    StripMarkdownLinkFromPath,
    /// 未知字段名匹配该工具的别名表 → 重命名为规范字段名。
    RenameAliasedField,
    /// 字段不在工具参数 schema 的 properties 中 → 删除。
    DropUnknownKey,
}

/// 来自 serde 报错的修复提示。
#[derive(Debug, Clone, Default)]
pub struct RepairHint {
    /// serde_path 形态：`patterns`、`args.list`、`items[0]`；`"."` / 空视为顶层。
    pub path: Option<String>,
    /// 从报错信息提取的 expected 片段，如 `a sequence`。
    pub expected: Option<String>,
}

/// 修复上下文：工具全名与合法字段集。由 `deserialize_with_repair` 在每次
/// 工具调用时构造；`repair_input` 的纯值入口不带上下文（保持向后兼容）。
#[derive(Debug, Clone, Default)]
pub struct RepairContext<'a> {
    /// 工具全名（`kimix:read_file`），用于字段别名表匹配。
    pub tool_name: Option<&'a str>,
    /// 该工具参数 schema 的 properties（合法字段名集合）。
    pub known_fields: Option<&'a std::collections::HashSet<String>>,
}

/// 字段名别名表：`(tool_name) -> (alias → canonical)`。
///
/// 覆盖跨命名约定的常见字段漂移（Claude/opencode 风格 `file_path`/`filePath`
/// vs Kimix 的 `target_file` 等）。规则值必须是 Kimix 工具 schema 中**真实存在**
/// 的字段名——否则别名表自身会成为 bug 来源。只收录有把握的映射，宁可漏不可错。
static FIELD_ALIASES: std::sync::LazyLock<
    std::collections::HashMap<&'static str, &'static [(&'static str, &'static str)]>,
> = std::sync::LazyLock::new(|| {
    let mut m: std::collections::HashMap<&'static str, &'static [(&'static str, &'static str)]> =
        std::collections::HashMap::new();
    // read_file 的 schema 字段是 `target_file`（serde rename）。
    m.insert(
        "kimix:read_file",
        &[
            ("path", "target_file"),
            ("file_path", "target_file"),
            ("filePath", "target_file"),
            ("file", "target_file"),
            ("start_line", "offset"),
            ("line", "offset"),
            ("lines", "limit"),
            ("num_lines", "limit"),
        ][..],
    );
    // list_dir 的 schema 字段是 `target_directory`。
    m.insert(
        "kimix:list_dir",
        &[
            ("path", "target_directory"),
            ("directory", "target_directory"),
            ("dir", "target_directory"),
            ("folder", "target_directory"),
        ][..],
    );
    // grep 的 schema 字段与常见别名同形（pattern/path/glob），仅补命名差异。
    m.insert(
        "kimix:grep",
        &[
            ("regex", "pattern"),
            ("query", "pattern"),
            ("file_pattern", "glob"),
            ("file_glob", "glob"),
        ][..],
    );
    // search_replace 字段名规范，但模型常发 opencode 风格 camelCase。
    m.insert(
        "kimix:search_replace",
        &[
            ("filePath", "file_path"),
            ("oldString", "old_string"),
            ("newString", "new_string"),
            ("replaceAll", "replace_all"),
        ][..],
    );
    m
});

/// 按工具名查别名表，返回 `(alias, canonical)` 对。
fn field_aliases_for(tool: &str) -> Option<&'static [(&'static str, &'static str)]> {
    FIELD_ALIASES.get(tool).copied()
}

/// 尝试按工具别名表把未知字段重命名为规范字段名。
fn alias_canonical(tool: &str, key: &str) -> Option<&'static str> {
    field_aliases_for(tool)
        .and_then(|rows| rows.iter().find(|(alias, _)| *alias == key).map(|(_, c)| *c))
}

/// 查询某工具的字段别名（供错误反馈等跨 crate 消费）。
///
/// `tool` 接受 `kimix:read_file` 或 `read_file` 两种形态；`field_aliases_for`
/// 未命中时尝试补 `kimix:` 前缀。返回该字段的规范名（别名命中）或 `None`。
pub fn canonical_field_name(tool: &str, field: &str) -> Option<&'static str> {
    alias_canonical(tool, field).or_else(|| {
        if tool.starts_with("kimix:") {
            None
        } else {
            alias_canonical(&format!("kimix:{tool}"), field)
        }
    })
}

/// 尝试按 hint 修复工具参数 JSON（无上下文的纯值入口，兼容旧调用方）。
pub fn repair_input(raw: &Value, hint: &RepairHint) -> (Value, RepairOutcome) {
    repair_input_with_context(raw, hint, &RepairContext::default())
}

/// 尝试按 hint 修复工具参数 JSON；`ctx` 携带工具名与合法字段集时启用
/// 字段别名重命名与未知字段删除。
pub fn repair_input_with_context(
    raw: &Value,
    hint: &RepairHint,
    ctx: &RepairContext<'_>,
) -> (Value, RepairOutcome) {
    let mut repaired = raw.clone();
    let mut applied = Vec::new();

    let path = hint
        .path
        .as_deref()
        .map(str::trim)
        .filter(|p| !p.is_empty() && *p != ".");

    // 字段名修复（别名/未知字段删除）：作用于顶层对象与 serde 报错路径的
    // 父对象。二者都可能带未知字段（serde 只报告第一个）。不递归嵌套对象，
    // 避免误伤 schema 中「值本身就是任意对象」的参数（如 `args`）。
    if let (Some(tool), Some(known)) = (ctx.tool_name, ctx.known_fields) {
        if let Some(path) = path {
            let tokens = tokenize_serde_path(path);
            if let Some(parent_tokens) = tokens.split_last().map(|(_, p)| p) {
                if !parent_tokens.is_empty() {
                    if let Some(parent) = value_at_path(&mut repaired, parent_tokens) {
                        fix_unknown_fields_in_object(parent, tool, known, &mut applied);
                    }
                }
            }
        }
        // 顶层对象总是可能带未知字段；非对象（数组/标量）时自然跳过。
        fix_unknown_fields_in_object(&mut repaired, tool, known, &mut applied);
    }

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
pub fn deserialize_with_repair<T>(json: Value, tool_name: &str) -> Result<(T, Option<Vec<RepairKind>>), serde_json::Error>
where
    T: DeserializeOwned + schemars::JsonSchema,
{
    match serde_path_to_error::deserialize::<_, T>(json.clone()) {
        Ok(typed) => Ok((typed, None)),
        Err(first_err) => {
            let path = first_err.path().to_string();
            let message = first_err.inner().to_string();
            let hint = RepairHint {
                path: normalize_path_opt(&path),
                expected: extract_expected(&message),
            };
            // 提前绑定 schema 字段集，避免临时值生命周期问题。
            let known_fields = schema_field_names::<T>();
            let ctx = RepairContext {
                tool_name: Some(tool_name),
                known_fields: known_fields.as_ref(),
            };
            let (repaired, outcome) = repair_input_with_context(&json, &hint, &ctx);
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

/// 从 `T` 的 schemars schema 提取顶层合法字段名集合（properties 键）。
/// schema 生成失败（自定义 schema 或缺省）时返回 `None` → 调用方跳过
/// 别名/未知字段修复（保守）。
fn schema_field_names<T: schemars::JsonSchema>() -> Option<std::collections::HashSet<String>> {
    let schema = schemars::schema_for!(T);
    // 走 JSON 值提取，避免依赖 schemars 内部 Schema 结构形态。
    let schema_json = serde_json::to_value(&schema).ok()?;
    let props = schema_json.get("properties")?.as_object()?;
    Some(props.keys().cloned().collect())
}

/// 处理单个对象（`Value::Object` 时）顶层的未知字段：
/// - 匹配工具别名表 → 重命名为规范字段名（不覆盖已存在的规范字段）；
/// - 否则 → 删除（`deny_unknown_fields` 下该字段本就不可能被接受）。
/// 返回实际命中的修复类型列表（遥测聚合用）；`value` 非对象时无操作。
fn fix_unknown_fields_in_object(
    value: &mut Value,
    tool: &str,
    known: &std::collections::HashSet<String>,
    applied: &mut Vec<RepairKind>,
) {
    let Value::Object(map) = value else {
        return;
    };
    // 先收集待处理键，避免在迭代 borrow 中修改 map。
    let keys: Vec<String> = map.keys().cloned().collect();
    for key in keys {
        if known.contains(&key) {
            continue;
        }
        if let Some(canonical) = alias_canonical(tool, &key) {
            if !map.contains_key(canonical) && map.contains_key(&key) {
                let val = map.remove(&key).expect("key present");
                map.insert(canonical.to_string(), val);
                applied.push(RepairKind::RenameAliasedField);
            } else {
                // 规范字段已存在（冗余别名）或 key 已消失：直接丢弃别名副本。
                map.remove(&key);
                applied.push(RepairKind::DropUnknownKey);
            }
        } else {
            map.remove(&key);
            applied.push(RepairKind::DropUnknownKey);
        }
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

    // 路径字段收到完整 `[text](url)` markdown 链接 → 提取 url。
    // 需要字段名上下文（key 是 path 类）才能安全强解包，故放在 repair_value_at 之前。
    if let Some((field, _)) = tokens.split_last() {
        if is_path_field_name(field) {
            if let Some(target) = value_at_path(root, tokens) {
                if let Some(kind) = fix_path_markdown_link(target) {
                    return Some(kind);
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
    // 4. 数字字段收到数字字符串（`"42"` → 42），依赖 expected 约束类型。
    if expected_is_number(expected) {
        if let Some(kind) = coerce_string_to_number(value) {
            return Some(kind);
        }
    }
    // 5. 布尔字段收到 `"true"` / `"false"`。
    if expected_is_boolean(expected) {
        if let Some(kind) = coerce_string_to_boolean(value) {
            return Some(kind);
        }
    }
    // 6/7. 仅当 expected 为 sequence 时：object 占位 / 裸标量（字符串/数字/布尔）
    if expected_is_sequence(expected) {
        if let Some(kind) = fix_object_placeholder(value) {
            return Some(kind);
        }
        if let Some(kind) = wrap_bare_scalar(value) {
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

fn wrap_bare_scalar(value: &mut Value) -> Option<RepairKind> {
    // 字符串 → BareStringWrapped（向后兼容）；数字/布尔 → WrapBareScalarAsArray。
    let kind = match value {
        Value::String(_) => RepairKind::BareStringWrapped,
        Value::Number(_) | Value::Bool(_) => RepairKind::WrapBareScalarAsArray,
        _ => return None,
    };
    let scalar = value.clone();
    *value = Value::Array(vec![scalar]);
    Some(kind)
}

/// expected 片段是否指向数字类型（serde 报错形态：`u64` / `an integer` /
/// `a floating point number` 等）。
fn expected_is_number(expected: Option<&str>) -> bool {
    let Some(e) = expected else {
        return false;
    };
    let e = e.to_ascii_lowercase();
    [
        "int", "float", "number", "u8", "u16", "u32", "u64", "usize", "i8", "i16", "i32",
        "i64", "isize", "f32", "f64",
    ]
    .iter()
    .any(|needle| e.contains(needle))
}

/// expected 片段是否指向布尔类型（`a boolean`）。
fn expected_is_boolean(expected: Option<&str>) -> bool {
    expected.is_some_and(|e| e.to_ascii_lowercase().contains("bool"))
}

/// 数字字段收到数字字符串 → 解析为数字（整数优先，失败退回浮点）。
fn coerce_string_to_number(value: &mut Value) -> Option<RepairKind> {
    let Value::String(s) = value else {
        return None;
    };
    let t = s.trim();
    if let Ok(n) = t.parse::<i64>() {
        *value = Value::Number(n.into());
        return Some(RepairKind::CoerceStringToNumber);
    }
    if let Ok(f) = t.parse::<f64>() {
        if f.is_finite() {
            if let Some(num) = serde_json::Number::from_f64(f) {
                *value = Value::Number(num);
                return Some(RepairKind::CoerceStringToNumber);
            }
        }
    }
    None
}

/// 布尔字段收到 `"true"` / `"false"`（大小写不敏感）→ 解析为布尔。
fn coerce_string_to_boolean(value: &mut Value) -> Option<RepairKind> {
    let Value::String(s) = value else {
        return None;
    };
    match s.trim().to_ascii_lowercase().as_str() {
        "true" => {
            *value = Value::Bool(true);
            Some(RepairKind::CoerceStringToBoolean)
        }
        "false" => {
            *value = Value::Bool(false);
            Some(RepairKind::CoerceStringToBoolean)
        }
        _ => None,
    }
}

/// 字段名是否为路径类（完整 markdown 链接可安全解包的 key 集合）。
fn is_path_field_name(key: &str) -> bool {
    matches!(
        key,
        "path" | "file_path" | "filePath" | "target_file" | "target_directory" | "directory"
            | "dir" | "file" | "folder"
    )
}

/// 完整 `[text](url)` markdown 链接 → 提取 url 为纯路径（仅路径类字段）。
///
/// 与退化链接（`text == url`，见 `unwrap_degenerate_markdown_link`）不同：路径字段
/// 收到纯链接形态时，若 url 是**路径形态**（无 `http(s)://` 协议，或 `file://`），
/// 几乎总是模型幻觉 → 提取 url。带 `http(s)://` 的 url 视为用户真实链接，不碰
///（交由退化链接规则判断）。尾部必须无剩余文本（纯链接形态）。
fn fix_path_markdown_link(value: &mut Value) -> Option<RepairKind> {
    let Value::String(s) = value else {
        return None;
    };
    let s = s.trim();
    let rest = s.strip_prefix('[')?;
    let paren_rel = rest.find("](")?;
    let url_start = paren_rel + 2;
    let rest_after = &rest[url_start..];
    let paren_end = rest_after.find(')')?;
    let url = rest_after[..paren_end].trim();
    // 链接之后不能有尾巴（纯链接形态才解包）。
    if !rest_after[paren_end + 1..].trim().is_empty() || url.is_empty() {
        return None;
    }
    // 仅路径形态 URL（相对路径 / 绝对路径 / file://），带 http(s):// 的不碰。
    if url.contains("://") && !url.starts_with("file://") {
        return None;
    }
    *value = Value::String(url.to_string());
    Some(RepairKind::StripMarkdownLinkFromPath)
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

    #[derive(Debug, Deserialize, PartialEq, schemars::JsonSchema)]
    struct PathsArgs {
        paths: Vec<String>,
    }

    #[derive(Debug, Deserialize, PartialEq, schemars::JsonSchema)]
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

    #[derive(Debug, Deserialize, PartialEq, schemars::JsonSchema)]
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

    // ─── Phase 1 新增规则测试 ───────────────────────────────────────────────

    #[test]
    fn coerce_string_to_number_on_number_expected() {
        let raw = json!({ "limit": "100" });
        let hint = RepairHint {
            path: Some("limit".into()),
            expected: Some("u64".into()),
        };
        let (out, outcome) = repair_input(&raw, &hint);
        assert!(matches!(
            outcome,
            RepairOutcome::Repaired(kinds) if kinds.contains(&RepairKind::CoerceStringToNumber)
        ));
        assert_eq!(out["limit"], json!(100));
        // 非数字字符串不被强转
        let raw2 = json!({ "limit": "abc" });
        let (out2, outcome2) = repair_input(&raw2, &hint);
        assert_eq!(outcome2, RepairOutcome::Unrepairable);
        assert_eq!(out2["limit"], json!("abc"));
    }

    #[test]
    fn coerce_string_to_boolean_on_boolean_expected() {
        let raw = json!({ "replace_all": "true" });
        let hint = RepairHint {
            path: Some("replace_all".into()),
            expected: Some("a boolean".into()),
        };
        let (out, outcome) = repair_input(&raw, &hint);
        assert!(matches!(
            outcome,
            RepairOutcome::Repaired(kinds) if kinds.contains(&RepairKind::CoerceStringToBoolean)
        ));
        assert_eq!(out["replace_all"], json!(true));
        // 大小写不敏感
        let raw2 = json!({ "replace_all": "FALSE" });
        let (out2, _) = repair_input(&raw2, &hint);
        assert_eq!(out2["replace_all"], json!(false));
        // 非布尔字符串不修
        let raw3 = json!({ "replace_all": "yes" });
        let (_, outcome3) = repair_input(&raw3, &hint);
        assert_eq!(outcome3, RepairOutcome::Unrepairable);
    }

    #[test]
    fn wrap_bare_number_and_bool_as_array() {
        let raw = json!({ "offsets": 5 });
        let hint = RepairHint {
            path: Some("offsets".into()),
            expected: Some("a sequence".into()),
        };
        let (out, outcome) = repair_input(&raw, &hint);
        assert!(matches!(
            outcome,
            RepairOutcome::Repaired(kinds) if kinds.contains(&RepairKind::WrapBareScalarAsArray)
        ));
        assert_eq!(out["offsets"], json!([5]));

        let raw2 = json!({ "flags": true });
        let hint2 = RepairHint {
            path: Some("flags".into()),
            expected: Some("a sequence".into()),
        };
        let (out2, outcome2) = repair_input(&raw2, &hint2);
        assert!(matches!(
            outcome2,
            RepairOutcome::Repaired(kinds) if kinds.contains(&RepairKind::WrapBareScalarAsArray)
        ));
        assert_eq!(out2["flags"], json!([true]));
    }

    #[test]
    fn strip_full_markdown_link_from_path_field() {
        // url 为路径形态（无协议）→ 提取为纯路径。
        let raw = json!({ "path": "[notes.md](notes.md)" });
        let hint = RepairHint {
            path: Some("path".into()),
            expected: Some("a string".into()),
        };
        let (out, outcome) = repair_input(&raw, &hint);
        assert!(matches!(
            outcome,
            RepairOutcome::Repaired(kinds)
                if kinds.contains(&RepairKind::StripMarkdownLinkFromPath)
        ));
        assert_eq!(out["path"], "notes.md");
    }

    #[test]
    fn http_url_link_in_path_field_not_stripped() {
        // 带 http(s):// 的 url 视为真实用户链接，路径字段也不解包。
        let raw = json!({ "path": "[click](https://example.com)" });
        let hint = RepairHint {
            path: Some("path".into()),
            expected: Some("a string".into()),
        };
        let (out, outcome) = repair_input(&raw, &hint);
        assert_eq!(outcome, RepairOutcome::Unrepairable);
        assert_eq!(out["path"], "[click](https://example.com)");
    }

    #[test]
    fn markdown_link_with_tail_not_stripped_from_path() {
        // 链接后有尾巴、非纯链接形态 → 路径分支不解包；但全局退化链接扫描
        // （text == url）仍会把它解为纯文本路径。
        let raw = json!({ "path": "see [notes.md](http://notes.md) here" });
        let hint = RepairHint {
            path: Some("path".into()),
            expected: Some("a string".into()),
        };
        let (out, outcome) = repair_input(&raw, &hint);
        assert!(matches!(
            outcome,
            RepairOutcome::Repaired(kinds) if kinds.contains(&RepairKind::MarkdownLinkUnwrapped)
        ));
        assert_eq!(out["path"], "see notes.md here");
    }

    fn known_set(fields: &[&str]) -> std::collections::HashSet<String> {
        fields.iter().map(|s| s.to_string()).collect()
    }

    fn read_ctx<'a>(fields: &'a std::collections::HashSet<String>) -> RepairContext<'a> {
        RepairContext {
            tool_name: Some("kimix:read_file"),
            known_fields: Some(fields),
        }
    }

    #[test]
    fn rename_aliased_field_to_canonical() {
        // read_file 的 schema 字段是 target_file；模型发 path → 重命名。
        let raw = json!({ "path": "/tmp/x", "limit": "10" });
        let hint = RepairHint {
            path: Some("limit".into()),
            expected: Some("u64".into()),
        };
        let fields = known_set(&["target_file", "offset", "limit"]);
        let ctx = read_ctx(&fields);
        let (out, outcome) = repair_input_with_context(&raw, &hint, &ctx);
        assert!(matches!(
            outcome,
            RepairOutcome::Repaired(kinds) if kinds.contains(&RepairKind::RenameAliasedField)
        ));
        assert!(out.get("path").is_none());
        assert_eq!(out["target_file"], "/tmp/x");
        assert_eq!(out["limit"], json!(10));
    }

    #[test]
    fn aliased_field_does_not_clobber_existing_canonical() {
        let raw = json!({ "path": "/tmp/alias", "target_file": "/tmp/canon", "limit": 10 });
        let hint = RepairHint {
            path: Some("limit".into()),
            expected: Some("a sequence".into()),
        };
        let fields = known_set(&["target_file", "offset", "limit"]);
        let ctx = read_ctx(&fields);
        let (out, outcome) = repair_input_with_context(&raw, &hint, &ctx);
        // path 是冗余别名 → 作为未知键丢弃（drop），不覆盖规范字段。
        assert!(matches!(
            outcome,
            RepairOutcome::Repaired(kinds) if kinds.contains(&RepairKind::DropUnknownKey)
        ));
        assert!(out.get("path").is_none());
        assert_eq!(out["target_file"], "/tmp/canon");
    }

    #[test]
    fn drop_unknown_key_outside_schema() {
        let raw = json!({ "target_file": "/tmp/x", "limit": 10, "random_field": 42 });
        let hint = RepairHint {
            path: Some("limit".into()),
            expected: Some("a sequence".into()),
        };
        let fields = known_set(&["target_file", "offset", "limit"]);
        let ctx = read_ctx(&fields);
        let (out, outcome) = repair_input_with_context(&raw, &hint, &ctx);
        assert!(matches!(
            outcome,
            RepairOutcome::Repaired(kinds) if kinds.contains(&RepairKind::DropUnknownKey)
        ));
        assert!(out.get("random_field").is_none());
        assert_eq!(out["target_file"], "/tmp/x");
    }

    #[test]
    fn no_context_keeps_unknown_keys_untouched() {
        // 不带上下文（旧入口/未知工具）：绝不丢弃字段，保持向后兼容。
        let raw = json!({ "path": "/tmp/x", "random_field": 42 });
        let hint = RepairHint::default();
        let (out, outcome) = repair_input(&raw, &hint);
        assert_eq!(outcome, RepairOutcome::Unrepairable);
        assert_eq!(out["path"], "/tmp/x");
        assert_eq!(out["random_field"], json!(42));
    }

    #[test]
    fn nested_object_unknown_keys_not_touched() {
        // 嵌套对象（值本身就是任意对象的参数）不做字段修复，避免误伤。
        // 顶层含别名（path→target_file）触发修复；合法字段 extra 内的任意键必须原样保留。
        let raw = json!({ "path": "/tmp/x", "limit": 10, "extra": { "whatever": 1 } });
        let fields = known_set(&["target_file", "offset", "limit", "extra"]);
        let ctx = read_ctx(&fields);
        let (out, outcome) = repair_input_with_context(&raw, &RepairHint::default(), &ctx);
        assert!(matches!(
            outcome,
            RepairOutcome::Repaired(kinds) if kinds.contains(&RepairKind::RenameAliasedField)
        ));
        assert!(out.get("path").is_none());
        assert_eq!(out["target_file"], "/tmp/x");
        assert_eq!(
            out["extra"]["whatever"], json!(1),
            "nested object must stay untouched"
        );
    }

    #[test]
    fn schema_field_names_from_args_struct() {
        use serde::Deserialize;
        #[derive(Debug, Deserialize, schemars::JsonSchema)]
        struct SchemaArgs {
            #[serde(rename = "target_file")]
            path: String,
            #[serde(default)]
            offset: Option<i64>,
            #[serde(default)]
            limit: Option<usize>,
        }
        let names = schema_field_names::<SchemaArgs>().expect("schema fields");
        assert!(names.contains("target_file"));
        assert!(names.contains("offset"));
        assert!(names.contains("limit"));
        assert!(!names.contains("path"), "serde rename must win");
    }

    #[test]
    fn canonical_field_name_resolves_aliases() {
        assert_eq!(
            canonical_field_name("kimix:read_file", "path"),
            Some("target_file")
        );
        assert_eq!(
            canonical_field_name("kimix:read_file", "filePath"),
            Some("target_file")
        );
        // 不带 kimix: 前缀也解析。
        assert_eq!(canonical_field_name("read_file", "path"), Some("target_file"));
        // 未注册工具/未知字段 → None。
        assert_eq!(canonical_field_name("kimix:nope", "path"), None);
        assert_eq!(canonical_field_name("kimix:read_file", "target_file"), None);
    }

    #[test]
    fn deserialize_with_repair_applies_alias_rename() {
        use serde::Deserialize;
        #[derive(Debug, Deserialize, PartialEq, schemars::JsonSchema)]
        struct ReadArgs {
            #[serde(rename = "target_file")]
            path: String,
            #[serde(default)]
            limit: Option<usize>,
        }
        // 模型发 `path`（别名）+ 非法 limit（数字字符串）→ 两项都修。
        let raw = json!({ "path": "/tmp/x", "limit": "10" });
        let (typed, kinds) =
            deserialize_with_repair::<ReadArgs>(raw, "kimix:read_file").expect("repaired");
        assert_eq!(typed.path, "/tmp/x");
        assert_eq!(typed.limit, Some(10));
        assert!(kinds.is_some_and(|k| {
            k.contains(&RepairKind::RenameAliasedField)
                && k.contains(&RepairKind::CoerceStringToNumber)
        }));
    }
}
