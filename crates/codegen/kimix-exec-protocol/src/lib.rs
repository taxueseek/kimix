//! Wire protocol for the Kimix out-of-process exec sandbox server.
//!
//! The exec server (kimix-exec-server) runs tool-side filesystem and shell
//! operations **inside a kernel-sandboxed child process** (Landlock/Seatbelt
//! applied via kimix-sandbox). The agent process stays unsandboxed so it can
//! reach the LLM API; the child carries the enforcement.
//!
//! Transport is line-delimited JSON-RPC 2.0 over stdio: one request per line
//! on stdin, one response per line on stdout. This keeps the protocol
//! trivially debuggable (`echo '{"jsonrpc":"2.0",...}' | kimix-exec-server`)
//! and dependency-free on the wire.
//!
//! Design notes (adapted from the industry-standard out-of-process sandbox
//! pattern, original implementation):
//!
//! - **Methods are grouped by noun**: `fs/*` for filesystem, `exec` for
//!   command execution. The server enforces the sandbox mode *inside* each
//!   handler, so a read-only child rejects `fs/write_file` even if the client
//!   never checks.
//! - **`initialize`** carries the sandbox context (workspace + mode) so the
//!   same binary can be spawned per-session with different profiles.
//! - Errors are machine-readable JSON-RPC errors with stable codes; the
//!   client surfaces the `message` verbatim to the model.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// JSON-RPC version constant (2.0).
pub const JSONRPC_VERSION: &str = "2.0";

/// Method names.
pub mod method {
    /// `initialize` — carry workspace + sandbox mode into the child.
    pub const INITIALIZE: &str = "initialize";
    /// `fs/read_file` — read a file (path as param).
    pub const FS_READ_FILE: &str = "fs/read_file";
    /// `fs/write_file` — write a file (path + content).
    pub const FS_WRITE_FILE: &str = "fs/write_file";
    /// `fs/create_directory` — create a directory.
    pub const FS_CREATE_DIRECTORY: &str = "fs/create_directory";
    /// `exec` — run a command inside the sandbox.
    pub const EXEC: &str = "exec";
    /// `shutdown` — ask the child to exit cleanly.
    pub const SHUTDOWN: &str = "shutdown";
}

/// Sandbox enforcement mode for the child process.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SandboxMode {
    /// No kernel sandbox applied in the child (off).
    Off,
    /// Child can read anywhere but write only inside the workspace.
    WorkspaceWrite,
    /// Child can read anywhere, writes rejected by the handler.
    ReadOnly,
}

impl Default for SandboxMode {
    fn default() -> Self {
        Self::WorkspaceWrite
    }
}

impl SandboxMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::WorkspaceWrite => "workspace-write",
            Self::ReadOnly => "read-only",
        }
    }

    /// Parse from a CLI/config string (`off` | `workspace-write` | `read-only`).
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "off" | "danger-full-access" => Some(Self::Off),
            "workspace-write" | "write" | "workspace" => Some(Self::WorkspaceWrite),
            "read-only" | "readonly" | "read" => Some(Self::ReadOnly),
            _ => None,
        }
    }

    /// Whether the handler should reject writes (read-only mode).
    pub fn is_read_only(self) -> bool {
        matches!(self, Self::ReadOnly)
    }
}

// ─── Requests ──────────────────────────────────────────────────────────────

/// A JSON-RPC request (`params` as a raw object the server matches by method).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcRequest {
    pub jsonrpc: String,
    pub id: u64,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

impl RpcRequest {
    pub fn new(id: u64, method: impl Into<String>, params: Value) -> Self {
        Self {
            jsonrpc: JSONRPC_VERSION.to_string(),
            id,
            method: method.into(),
            params,
        }
    }
}

// ─── Responses ─────────────────────────────────────────────────────────────

/// Stable JSON-RPC error codes.
pub mod error_code {
    /// Params failed to deserialize into the method's expected shape.
    pub const INVALID_PARAMS: i64 = -32602;
    /// The sandbox mode forbids the operation (e.g. write in read-only).
    pub const FORBIDDEN: i64 = -32001;
    /// The operation failed at the OS level (io error, non-zero exit).
    pub const OPERATION_FAILED: i64 = -32002;
    /// The child received an unknown method.
    pub const METHOD_NOT_FOUND: i64 = -32601;
    /// The child has not been initialized.
    pub const NOT_INITIALIZED: i64 = -32003;
}

/// JSON-RPC error object.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RpcError {
    pub code: i64,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl RpcError {
    pub fn new(code: i64, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            data: None,
        }
    }

    pub fn with_data(code: i64, message: impl Into<String>, data: Value) -> Self {
        Self {
            code,
            message: message.into(),
            data: Some(data),
        }
    }
}

/// JSON-RPC response (success carries `result`, failure carries `error`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcResponse {
    pub jsonrpc: String,
    pub id: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<RpcError>,
}

impl RpcResponse {
    pub fn ok(id: u64, result: Value) -> Self {
        Self {
            jsonrpc: JSONRPC_VERSION.to_string(),
            id,
            result: Some(result),
            error: None,
        }
    }

    pub fn err(id: u64, error: RpcError) -> Self {
        Self {
            jsonrpc: JSONRPC_VERSION.to_string(),
            id,
            result: None,
            error: Some(error),
        }
    }
}

// ─── Typed params / results ────────────────────────────────────────────────

/// `initialize` params.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitializeParams {
    pub workspace: String,
    pub mode: SandboxMode,
}

/// `fs/read_file` params.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FsReadParams {
    pub path: String,
}

/// `fs/write_file` params.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FsWriteParams {
    pub path: String,
    pub content: String,
}

/// `fs/create_directory` params.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FsCreateDirectoryParams {
    pub path: String,
}

/// `exec` params.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecParams {
    /// Command to run (argv[0]).
    pub command: String,
    /// Arguments (without argv[0]).
    #[serde(default)]
    pub args: Vec<String>,
    /// Working directory (defaults to the sandbox workspace).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    /// Timeout in milliseconds (0 / None = no timeout).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
}

/// `fs/read_file` result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FsReadResult {
    pub content: String,
}

/// `exec` result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecResult {
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    /// True when the command was killed by the timeout.
    pub timed_out: bool,
}

// ─── Serialization helpers ─────────────────────────────────────────────────

/// Serialize a request to one line.
pub fn request_to_line(req: &RpcRequest) -> String {
    serde_json::to_string(req).expect("request serializes")
}

/// Deserialize a response from one line.
pub fn response_from_line(line: &str) -> Result<RpcResponse, serde_json::Error> {
    serde_json::from_str(line)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sandbox_mode_parse() {
        assert_eq!(SandboxMode::parse("read-only"), Some(SandboxMode::ReadOnly));
        assert_eq!(
            SandboxMode::parse("workspace-write"),
            Some(SandboxMode::WorkspaceWrite)
        );
        assert_eq!(SandboxMode::parse("off"), Some(SandboxMode::Off));
        assert_eq!(SandboxMode::parse("bogus"), None);
        assert_eq!(SandboxMode::parse(""), None);
    }

    #[test]
    fn request_roundtrips() {
        let req = RpcRequest::new(
            1,
            method::FS_READ_FILE,
            serde_json::to_value(FsReadParams {
                path: "/tmp/x.rs".into(),
            })
            .unwrap(),
        );
        let line = request_to_line(&req);
        let back: RpcRequest = serde_json::from_str(&line).unwrap();
        assert_eq!(back.id, 1);
        assert_eq!(back.method, "fs/read_file");
        let p: FsReadParams = serde_json::from_value(back.params).unwrap();
        assert_eq!(p.path, "/tmp/x.rs");
    }

    #[test]
    fn response_ok_and_err_roundtrip() {
        let ok = RpcResponse::ok(7, serde_json::json!({ "content": "hi" }));
        let line = serde_json::to_string(&ok).unwrap();
        let back = response_from_line(&line).unwrap();
        assert_eq!(back.id, 7);
        assert!(back.error.is_none());
        assert_eq!(back.result.as_ref().unwrap()["content"], "hi");

        let err = RpcResponse::err(8, RpcError::new(error_code::FORBIDDEN, "write blocked"));
        let line = serde_json::to_string(&err).unwrap();
        let back = response_from_line(&line).unwrap();
        assert_eq!(back.id, 8);
        assert_eq!(back.error.unwrap().code, error_code::FORBIDDEN);
    }

    #[test]
    fn exec_params_omit_optional_fields() {
        let p = ExecParams {
            command: "ls".into(),
            args: vec!["-la".into()],
            cwd: None,
            timeout_ms: Some(5000),
        };
        let v = serde_json::to_value(&p).unwrap();
        assert_eq!(v["command"], "ls");
        assert_eq!(v["args"][0], "-la");
        assert_eq!(v["timeout_ms"], 5000);
        assert!(v.get("cwd").is_none(), "None fields are omitted");
    }
}
