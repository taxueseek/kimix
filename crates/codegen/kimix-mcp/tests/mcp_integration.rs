//! MCP integration tests for Kimix.
//!
//! Tests cover:
//! - Tool name validation
//! - URL normalization
//! - Server descriptor sanitization
//! - Transport layer basics
//!
//! Run with: cargo test -p kimix-mcp --test mcp_integration

use kimix_mcp::servers::{sanitize_descriptor_segment, validate_tool_name};

// ───────────────────────────────────────────────────────────────────────────
// Tool Name Validation Tests
// ───────────────────────────────────────────────────────────────────────────

#[test]
fn test_tool_name_valid_simple() {
    assert!(validate_tool_name("echo").is_ok());
    assert!(validate_tool_name("read_file").is_ok());
    assert!(validate_tool_name("my-tool").is_ok());
}

#[test]
fn test_tool_name_valid_with_underscore_prefix() {
    assert!(validate_tool_name("_private").is_ok());
    assert!(validate_tool_name("__dunder__").is_ok());
}

#[test]
fn test_tool_name_valid_max_length() {
    let name = "a".repeat(64);
    assert!(validate_tool_name(&name).is_ok());
}

#[test]
fn test_tool_name_valid_with_digits() {
    assert!(validate_tool_name("tool123").is_ok()); // digits after first char OK
}

#[test]
fn test_tool_name_invalid_empty() {
    assert!(validate_tool_name("").is_err());
}

#[test]
fn test_tool_name_invalid_too_long() {
    let name = "a".repeat(65);
    assert!(validate_tool_name(&name).is_err());
}

#[test]
fn test_tool_name_invalid_with_dot() {
    assert!(validate_tool_name("server.tool").is_err());
}

#[test]
fn test_tool_name_invalid_with_space() {
    assert!(validate_tool_name("my tool").is_err());
}

#[test]
fn test_tool_name_invalid_with_special_chars() {
    assert!(validate_tool_name("tool@name").is_err());
    assert!(validate_tool_name("tool$name").is_err());
    assert!(validate_tool_name("tool!name").is_err());
}

#[test]
fn test_tool_name_invalid_start_with_digit() {
    // 实现契约（servers.rs TOOL_NAME_REGEX）：必须以字母或下划线开头
    //（Gemini 兼容要求），数字开头一律拒绝。
    assert!(validate_tool_name("1tool").is_err());
    assert!(validate_tool_name("9_server").is_err());
}

// ───────────────────────────────────────────────────────────────────────────
// Sanitize Descriptor Segment Tests
// ───────────────────────────────────────────────────────────────────────────

#[test]
fn test_sanitize_simple() {
    assert_eq!(sanitize_descriptor_segment("hello"), "hello");
    assert_eq!(sanitize_descriptor_segment("my-server"), "my-server");
    assert_eq!(sanitize_descriptor_segment("my_server"), "my_server");
}

#[test]
fn test_sanitize_with_dots() {
    assert_eq!(sanitize_descriptor_segment("server.v2"), "server.v2");
}

#[test]
fn test_sanitize_with_spaces() {
    assert_eq!(sanitize_descriptor_segment("Hugging Face"), "Hugging_Face");
    assert_eq!(sanitize_descriptor_segment("my server"), "my_server");
}

#[test]
fn test_sanitize_with_special_chars() {
    assert_eq!(sanitize_descriptor_segment("server@home"), "server_home");
    assert_eq!(sanitize_descriptor_segment("tool!"), "tool_");
}

#[test]
fn test_sanitize_empty() {
    assert_eq!(sanitize_descriptor_segment(""), "_");
}

#[test]
fn test_sanitize_preserves_structure() {
    assert_eq!(
        sanitize_descriptor_segment("user-Hugging Face/v2"),
        "user-Hugging_Face_v2"
    );
}

// ───────────────────────────────────────────────────────────────────────────
// MCP Tool Name Delimiter Tests
// ───────────────────────────────────────────────────────────────────────────

#[test]
fn test_mcp_tool_name_delimiter() {
    use kimix_workspace_types::MCP_TOOL_NAME_DELIMITER;
    assert_eq!(MCP_TOOL_NAME_DELIMITER, "__");
}

#[test]
fn test_mcp_tool_name_qualified_format() {
    use kimix_workspace_types::MCP_TOOL_NAME_DELIMITER;
    let server = "github";
    let tool = "create_issue";
    let qualified = format!("{}{}{}", server, MCP_TOOL_NAME_DELIMITER, tool);
    assert_eq!(qualified, "github__create_issue");
}

// ───────────────────────────────────────────────────────────────────────────
// URL Normalization Tests
// ───────────────────────────────────────────────────────────────────────────

// Note: normalize_url is private, but we can test it indirectly through
// the server management APIs. For now, we test the public API surface.

// ───────────────────────────────────────────────────────────────────────────
// Integration Smoke Tests
// ───────────────────────────────────────────────────────────────────────────

#[test]
fn test_mcp_module_structure() {
    // Verify that MCP modules are properly exported
    use kimix_mcp::servers::sanitize_descriptor_segment;
    use kimix_mcp::servers::validate_tool_name;

    // Basic validation tests
    assert!(validate_tool_name("valid_tool").is_ok());
    assert_eq!(sanitize_descriptor_segment("test-name"), "test-name");
}
