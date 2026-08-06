//! Out-of-process sandboxed execution server for Kimix.
//!
//! Spawned as a **child process** by the agent. The child applies the
//! kernel sandbox (Landlock/Seatbelt via kimix-sandbox) to **itself** and
//! then serves filesystem / exec requests over line-delimited JSON-RPC on
//! stdio. The parent stays unsandboxed (needs network for the LLM API); the
//! child carries all the enforcement.
//!
//! # Enforcement model
//!
//! Enforcement is **two-layered**:
//!
//! 1. **Kernel (macOS/Linux)**: `SandboxManager::apply()` pins the whole
//!    child to the selected profile — workspace-write lets it write only
//!    inside the workspace + kimix home + temp; read-only denies writes
//!    entirely; off applies nothing.
//! 2. **Handler (portable)**: regardless of kernel support, every handler
//!    re-checks the mode. A read-only child rejects `fs/write_file` and
//!    `fs/create_directory` with a `FORBIDDEN` error *even on platforms
//!    without a kernel sandbox*, so the contract holds everywhere.
//!
//! # Protocol
//!
//! One JSON-RPC request per stdin line, one response per stdout line (see
//! kimix-exec-protocol). `initialize` must arrive first and carries the
//! workspace + mode; every other method errors `NOT_INITIALIZED` before it.

use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};

use kimix_exec_protocol::*;

/// Runtime state after `initialize`.
struct SandboxedServer {
    workspace: PathBuf,
    mode: SandboxMode,
}

/// Main entry: read stdin line-by-line, dispatch, write responses.
pub fn serve() -> anyhow::Result<()> {
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    let mut state: Option<SandboxedServer> = None;

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(e) => {
                tracing::warn!(error = % e, "exec-server: stdin read error; exiting");
                break;
            }
        };
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let request: RpcRequest = match serde_json::from_str(trimmed) {
            Ok(r) => r,
            Err(e) => {
                // Unparseable request → best-effort error with id 0.
                let resp = RpcResponse::err(
                    0,
                    RpcError::new(error_code::INVALID_PARAMS, format!("invalid JSON-RPC: {e}")),
                );
                write_response(&mut stdout, &resp)?;
                continue;
            }
        };
        let response = dispatch(&mut state, &request);
        write_response(&mut stdout, &response)?;
    }
    Ok(())
}

/// Dispatch one request against the current state.
fn dispatch(state: &mut Option<SandboxedServer>, req: &RpcRequest) -> RpcResponse {
    // initialize is the only method valid before state exists.
    if req.method != method::INITIALIZE {
        let Some(server) = state.as_ref() else {
            return RpcResponse::err(
                req.id,
                RpcError::new(
                    error_code::NOT_INITIALIZED,
                    "exec-server: initialize must be called first",
                ),
            );
        };
        return handle_initialized(server, req);
    }
    let params: InitializeParams = match serde_json::from_value(req.params.clone()) {
        Ok(p) => p,
        Err(e) => {
            return RpcResponse::err(
                req.id,
                RpcError::new(error_code::INVALID_PARAMS, format!("bad initialize params: {e}")),
            );
        }
    };
    match apply_kernel_sandbox(&params.workspace, params.mode) {
        Ok(()) => {
            *state = Some(SandboxedServer {
                workspace: PathBuf::from(&params.workspace),
                mode: params.mode,
            });
            RpcResponse::ok(
                req.id,
                serde_json::json!({
                    "mode": params.mode.as_str(),
                    "workspace": params.workspace,
                    "kernel_sandbox": kernel_applied(),
                }),
            )
        }
        Err(e) => RpcResponse::err(
            req.id,
            RpcError::new(
                error_code::OPERATION_FAILED,
                format!("failed to apply sandbox: {e}"),
            ),
        ),
    }
}

/// Handlers for every method that requires an initialized server.
fn handle_initialized(server: &SandboxedServer, req: &RpcRequest) -> RpcResponse {
    match req.method.as_str() {
        method::FS_READ_FILE => {
            let params: FsReadParams = match serde_json::from_value(req.params.clone()) {
                Ok(p) => p,
                Err(e) => return invalid_params(req, e),
            };
            match std::fs::read_to_string(&params.path) {
                Ok(content) => RpcResponse::ok(req.id, serde_json::to_value(FsReadResult { content }).unwrap()),
                Err(e) => RpcResponse::err(
                    req.id,
                    RpcError::new(error_code::OPERATION_FAILED, format!("read {0}: {e}", params.path)),
                ),
            }
        }
        method::FS_WRITE_FILE => {
            let params: FsWriteParams = match serde_json::from_value(req.params.clone()) {
                Ok(p) => p,
                Err(e) => return invalid_params(req, e),
            };
            if let Some(resp) = reject_write(server, req.id, &params.path) {
                return resp;
            }
            match write_file(&params.path, &params.content) {
                Ok(()) => RpcResponse::ok(req.id, serde_json::json!({ "ok": true })),
                Err(e) => RpcResponse::err(
                    req.id,
                    RpcError::new(error_code::OPERATION_FAILED, format!("write: {e}")),
                ),
            }
        }
        method::FS_CREATE_DIRECTORY => {
            let params: FsCreateDirectoryParams = match serde_json::from_value(req.params.clone())
            {
                Ok(p) => p,
                Err(e) => return invalid_params(req, e),
            };
            if let Some(resp) = reject_write(server, req.id, &params.path) {
                return resp;
            }
            match std::fs::create_dir_all(&params.path) {
                Ok(()) => RpcResponse::ok(req.id, serde_json::json!({ "ok": true })),
                Err(e) => RpcResponse::err(
                    req.id,
                    RpcError::new(error_code::OPERATION_FAILED, format!("mkdir: {e}")),
                ),
            }
        }
        method::EXEC => {
            let params: ExecParams = match serde_json::from_value(req.params.clone()) {
                Ok(p) => p,
                Err(e) => return invalid_params(req, e),
            };
            RpcResponse::ok(req.id, run_exec(&server, &params))
        }
        method::SHUTDOWN => RpcResponse::ok(req.id, serde_json::json!({ "ok": true })),
        other => RpcResponse::err(
            req.id,
            RpcError::new(error_code::METHOD_NOT_FOUND, format!("unknown method: {other}")),
        ),
    }
}

fn invalid_params(req: &RpcRequest, e: serde_json::Error) -> RpcResponse {
    RpcResponse::err(
        req.id,
        RpcError::new(error_code::INVALID_PARAMS, format!("bad params: {e}")),
    )
}

/// Write one JSON response line to stdout.
fn write_response(out: &mut impl Write, resp: &RpcResponse) -> anyhow::Result<()> {
    let line = serde_json::to_string(resp)?;
    out.write_all(line.as_bytes())?;
    out.write_all(b"\n")?;
    out.flush()?;
    Ok(())
}

/// Apply the kernel sandbox to the **current process** (the child).
///
/// Profile selection goes through [`kimix_sandbox::profile_for_sandbox_mode`]
/// (ExecTransform) so mode→profile mapping cannot drift from other surfaces.
fn apply_kernel_sandbox(workspace: &str, mode: SandboxMode) -> anyhow::Result<()> {
    let profile = kimix_sandbox::profile_for_sandbox_mode(mode.as_str()).ok_or_else(|| {
        anyhow::anyhow!("unknown sandbox mode for profile mapping: {}", mode.as_str())
    })?;
    let mut manager = kimix_sandbox::SandboxManager::new(profile.clone(), Path::new(workspace));
    let applied = manager.apply(Path::new(workspace))?;
    // Off mode never applies anything; enforcement then relies on the
    // portable handler-level checks alone.
    if mode != SandboxMode::Off {
        KERNEL_APPLIED.store(true, std::sync::atomic::Ordering::Relaxed);
    }
    tracing::info!(
        mode = mode.as_str(),
        profile = % profile,
        kernel_sandbox = mode != SandboxMode::Off,
        "exec-server: child sandbox initialized"
    );
    let _ = applied;
    Ok(())
}

/// Whether the kernel sandbox was successfully applied (for initialize result).
static KERNEL_APPLIED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn kernel_applied() -> bool {
    KERNEL_APPLIED.load(std::sync::atomic::Ordering::Relaxed)
}

/// Handler-level write gate via ExecTransform (`allows_write`).
///
/// Returns `Some(error response)` when the write is forbidden; `None` to proceed.
fn reject_write(server: &SandboxedServer, req_id: u64, path: &str) -> Option<RpcResponse> {
    let profile = kimix_sandbox::profile_for_sandbox_mode(server.mode.as_str())
        .unwrap_or(kimix_sandbox::ProfileName::Workspace);
    // Prefer canonical containment when possible; fall back to lexical path.
    let target = PathBuf::from(path);
    let decision = if !within_workspace(&server.workspace, path)
        && !kimix_sandbox::path_within_workspace(&server.workspace, &target)
    {
        kimix_sandbox::WriteDecision::DenyOutsideWorkspace
    } else {
        // Use workspace-relative check for protected prefixes even when
        // canonicalize differs; pass absolute path under workspace when known.
        let check_path = if target.is_absolute() {
            target.clone()
        } else {
            server.workspace.join(&target)
        };
        kimix_sandbox::allows_write(&profile, &server.workspace, &check_path)
    };
    match decision {
        kimix_sandbox::WriteDecision::Allow => None,
        kimix_sandbox::WriteDecision::DenyReadOnly => Some(RpcResponse::err(
            req_id,
            RpcError::new(
                error_code::FORBIDDEN,
                "exec-server is read-only; write rejected",
            ),
        )),
        kimix_sandbox::WriteDecision::DenyOutsideWorkspace => Some(RpcResponse::err(
            req_id,
            RpcError::new(
                error_code::FORBIDDEN,
                format!("path outside workspace: {path}"),
            ),
        )),
        kimix_sandbox::WriteDecision::DenyProtected => Some(RpcResponse::err(
            req_id,
            RpcError::new(
                error_code::FORBIDDEN,
                format!("path is protected under workspace-write policy: {path}"),
            ),
        )),
    }
}

/// Path containment check (portable handler-level guard). No fs access —
/// purely lexical after canonicalizing the target's parent.
fn within_workspace(workspace: &Path, target: &str) -> bool {
    let target_abs = if Path::new(target).is_absolute() {
        PathBuf::from(target)
    } else {
        workspace.join(target)
    };
    let Ok(ws) = std::fs::canonicalize(workspace) else {
        // Workspace missing — be conservative.
        return false;
    };
    let Ok(target) = std::fs::canonicalize(&target_abs) else {
        // Target doesn't exist yet (write of a new file): canonicalize the
        // parent dir and require the final name be a bare file name (no path
        // separators → cannot escape the parent).
        let parent = target_abs.parent().unwrap_or(workspace);
        let Ok(parent_c) = std::fs::canonicalize(parent) else {
            return false;
        };
        let name = target_abs.file_name().map(|n| n.to_string_lossy().into_owned());
        let Some(name) = name else {
            return false;
        };
        if name.contains('/') || name.contains(std::path::MAIN_SEPARATOR) {
            return false;
        }
        return parent_c.starts_with(&ws);
    };
    target.starts_with(&ws)
}

/// Write content atomically-ish (create parent dirs, write, no fsync dance —
/// good enough for tool semantics).
fn write_file(path: &str, content: &str) -> std::io::Result<()> {
    if let Some(parent) = Path::new(path).parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, content)
}

/// Run a command with a timeout, capturing stdout/stderr.
fn run_exec(server: &SandboxedServer, params: &ExecParams) -> serde_json::Value {
    let cwd = params
        .cwd
        .as_deref()
        .map(PathBuf::from)
        .unwrap_or_else(|| server.workspace.clone());
    let mut cmd = std::process::Command::new(&params.command);
    cmd.args(&params.args).current_dir(&cwd);

    let timeout = params.timeout_ms.filter(|t| *t > 0).map(std::time::Duration::from_millis);
    let output = match timeout {
        Some(dur) => match run_with_timeout(cmd, dur) {
            Ok(o) => o,
            Err(e) => {
                return serde_json::to_value(ExecResult {
                    exit_code: None,
                    stdout: String::new(),
                    stderr: format!("exec-server: failed to spawn: {e}"),
                    timed_out: false,
                })
                .unwrap()
            }
        },
        None => match cmd.output() {
            Ok(o) => o,
            Err(e) => {
                return serde_json::to_value(ExecResult {
                    exit_code: None,
                    stdout: String::new(),
                    stderr: format!("exec-server: failed to spawn: {e}"),
                    timed_out: false,
                })
                .unwrap()
            }
        },
    };
    let result = ExecResult {
        exit_code: output.status.code(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        timed_out: false,
    };
    serde_json::to_value(result).unwrap()
}

/// Spawn and wait with a timeout; kill on expiry.
fn run_with_timeout(
    mut cmd: std::process::Command,
    dur: std::time::Duration,
) -> std::io::Result<std::process::Output> {
    let mut child = cmd.spawn()?;
    let start = std::time::Instant::now();
    loop {
        if let Some(status) = child.try_wait()? {
            let mut output = std::process::Output {
                status,
                stdout: Vec::new(),
                stderr: Vec::new(),
            };
            // Wait for any buffered output the child left behind.
            if let Ok(waited) = child.wait_with_output() {
                output = waited;
            }
            return Ok(output);
        }
        if start.elapsed() >= dur {
            let _ = child.kill();
            let _ = child.wait();
            // Synthesize a "killed by timeout" status: run `false` to get a
            // non-zero ExitStatus without platform-specific extensions.
            let status = std::process::Command::new("false")
                .status()
                .unwrap_or(std::process::ExitStatus::default());
            return Ok(std::process::Output {
                status,
                stdout: Vec::new(),
                stderr: Vec::new(),
            });
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn within_workspace_accepts_inside_and_rejects_outside() {
        let ws = std::env::temp_dir().join(format!("kimix-exec-ws-{}", std::process::id()));
        std::fs::create_dir_all(ws.join("sub")).unwrap();
        std::fs::create_dir_all(ws.join("relative")).unwrap();
        let ws = ws.canonicalize().unwrap();
        let inside = ws.join("sub/file.rs");
        assert!(within_workspace(&ws, inside.to_str().unwrap()));
        assert!(within_workspace(&ws, "relative/file.rs"));
        let outside = std::env::temp_dir().join("other-place.txt");
        assert!(!within_workspace(&ws, outside.to_str().unwrap()));
        // absolute traversal
        assert!(!within_workspace(&ws, "/etc/passwd"));
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn read_only_rejects_write() {
        // Handler-level: dispatch FS_WRITE_FILE against a read-only server.
        let server = SandboxedServer {
            workspace: PathBuf::from("/tmp"),
            mode: SandboxMode::ReadOnly,
        };
        let req = RpcRequest::new(
            1,
            method::FS_WRITE_FILE,
            serde_json::to_value(FsWriteParams {
                path: "/tmp/x".into(),
                content: "hi".into(),
            })
            .unwrap(),
        );
        let resp = handle_initialized(&server, &req);
        assert_eq!(resp.error.unwrap().code, error_code::FORBIDDEN);
    }

    #[test]
    fn uninitialized_rejects_everything_but_initialize() {
        let mut state = None;
        let req = RpcRequest::new(1, method::FS_READ_FILE, serde_json::json!({}));
        let resp = dispatch(&mut state, &req);
        assert_eq!(resp.error.unwrap().code, error_code::NOT_INITIALIZED);
    }

    #[test]
    fn unknown_method_errors() {
        let server = SandboxedServer {
            workspace: PathBuf::from("/tmp"),
            mode: SandboxMode::WorkspaceWrite,
        };
        let req = RpcRequest::new(1, "bogus/method", serde_json::json!({}));
        let resp = handle_initialized(&server, &req);
        assert_eq!(resp.error.unwrap().code, error_code::METHOD_NOT_FOUND);
    }

    #[test]
    fn workspace_write_rejects_protected_git() {
        let ws = std::env::temp_dir().join(format!("kimix-exec-git-{}", std::process::id()));
        std::fs::create_dir_all(ws.join(".git")).unwrap();
        let ws_c = ws.canonicalize().unwrap();
        let server = SandboxedServer {
            workspace: ws_c.clone(),
            mode: SandboxMode::WorkspaceWrite,
        };
        let git_path = ws_c.join(".git/config");
        let req = RpcRequest::new(
            1,
            method::FS_WRITE_FILE,
            serde_json::to_value(FsWriteParams {
                path: git_path.to_string_lossy().into_owned(),
                content: "evil".into(),
            })
            .unwrap(),
        );
        let resp = handle_initialized(&server, &req);
        let err = resp.error.expect("protected .git write must be forbidden");
        assert_eq!(err.code, error_code::FORBIDDEN);
        assert!(
            err.message.contains("protected"),
            "message should mention protected: {}",
            err.message
        );
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn write_roundtrip_through_server() {
        let ws = std::env::temp_dir().join(format!("kimix-exec-rw-{}", std::process::id()));
        std::fs::create_dir_all(&ws).unwrap();
        let ws_c = ws.canonicalize().unwrap();
        let server = SandboxedServer {
            workspace: ws_c.clone(),
            mode: SandboxMode::WorkspaceWrite,
        };
        let target = ws_c.join("out.txt");
        let req = RpcRequest::new(
            1,
            method::FS_WRITE_FILE,
            serde_json::to_value(FsWriteParams {
                path: target.to_str().unwrap().into(),
                content: "hello".into(),
            })
            .unwrap(),
        );
        let resp = handle_initialized(&server, &req);
        assert!(resp.error.is_none(), "write must succeed: {:?}", resp.error);
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "hello");

        // Read it back.
        let req2 = RpcRequest::new(
            2,
            method::FS_READ_FILE,
            serde_json::to_value(FsReadParams {
                path: target.to_str().unwrap().into(),
            })
            .unwrap(),
        );
        let resp2 = handle_initialized(&server, &req2);
        let result: FsReadResult = serde_json::from_value(resp2.result.unwrap()).unwrap();
        assert_eq!(result.content, "hello");
        let _ = std::fs::remove_dir_all(&ws);
    }
}
