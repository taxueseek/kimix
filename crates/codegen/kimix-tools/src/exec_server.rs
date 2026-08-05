//! Out-of-process exec-sandbox client.
//!
//! Spawns the `kimix-exec-server` binary as a child, initializes it with a
//! workspace + sandbox mode, and proxies filesystem / exec operations through
//! it. The agent process stays unsandboxed (it needs network for the LLM
//! API); every fs/exec operation runs inside the kernel-sandboxed child.
//!
//! # Enablement
//!
//! Off by default — the existing in-process tools remain the default path.
//! Set `KIMIX_EXEC_SERVER=1` to route tool-side operations through the
//! sandboxed child (see the tool-layer integration notes in `registry`).
//! `KIMIX_EXEC_SERVER_BIN` overrides the server binary path (tests, custom
//! builds).

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use kimix_exec_protocol::*;

/// Whether the exec-sandbox server is enabled for this process.
pub fn exec_server_enabled() -> bool {
    parse_enabled_flag(&std::env::var("KIMIX_EXEC_SERVER").unwrap_or_default())
}

/// Parse the `KIMIX_EXEC_SERVER` flag (pure, testable).
fn parse_enabled_flag(v: &str) -> bool {
    matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on")
}

/// Resolve the server binary path (`KIMIX_EXEC_SERVER_BIN` override, else the
/// bundled `kimix-exec-server` on PATH).
fn server_bin() -> String {
    std::env::var("KIMIX_EXEC_SERVER_BIN").unwrap_or_else(|_| "kimix-exec-server".to_string())
}

/// A live connection to one exec-server child.
pub struct ExecServerClient {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: u64,
    workspace: PathBuf,
    mode: SandboxMode,
}

impl ExecServerClient {
    /// Spawn the server and run `initialize`.
    ///
    /// Returns `Err` when the binary is missing or initialization fails.
    pub fn spawn(workspace: &Path, mode: SandboxMode) -> anyhow::Result<Self> {
        let mut child = Command::new(server_bin())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| anyhow::anyhow!("failed to spawn exec-server: {e}"))?;
        let stdin = child.stdin.take().ok_or_else(|| {
            anyhow::anyhow!("exec-server stdin unavailable")
        })?;
        let stdout = BufReader::new(
            child
                .stdout
                .take()
                .ok_or_else(|| anyhow::anyhow!("exec-server stdout unavailable"))?,
        );
        let mut client = Self {
            child,
            stdin,
            stdout,
            next_id: 1,
            workspace: workspace.to_path_buf(),
            mode,
        };
        client.initialize()?;
        Ok(client)
    }

    fn initialize(&mut self) -> anyhow::Result<()> {
        let params = InitializeParams {
            workspace: self.workspace.to_string_lossy().into_owned(),
            mode: self.mode,
        };
        let resp = self.call(method::INITIALIZE, serde_json::to_value(params)?)?;
        if let Some(e) = resp.error {
            return Err(anyhow::anyhow!("exec-server initialize failed: {}", e.message));
        }
        Ok(())
    }

    /// Send one request and read the matching response.
    fn call(&mut self, method: &str, params: serde_json::Value) -> anyhow::Result<RpcResponse> {
        let id = self.next_id;
        self.next_id += 1;
        let req = RpcRequest::new(id, method, params);
        let line = request_to_line(&req);
        self.stdin.write_all(line.as_bytes())?;
        self.stdin.write_all(b"\n")?;
        self.stdin.flush()?;
        loop {
            let mut buf = String::new();
            let n = self.stdout.read_line(&mut buf)?;
            if n == 0 {
                return Err(anyhow::anyhow!(
                    "exec-server closed stdout (crash or early exit)"
                ));
            }
            let resp = response_from_line(buf.trim())?;
            if resp.id == id {
                return Ok(resp);
            }
            // Skip unrelated/notification lines defensively.
        }
    }

    /// Read a file through the sandboxed child.
    pub fn read_file(&mut self, path: &str) -> anyhow::Result<String> {
        let params = FsReadParams { path: path.into() };
        let resp = self.call(method::FS_READ_FILE, serde_json::to_value(params)?)?;
        if let Some(e) = resp.error {
            return Err(anyhow::anyhow!("read_file: {}", e.message));
        }
        let result: FsReadResult = serde_json::from_value(resp.result.unwrap_or_default())?;
        Ok(result.content)
    }

    /// Write a file through the sandboxed child (rejected in read-only mode).
    pub fn write_file(&mut self, path: &str, content: &str) -> anyhow::Result<()> {
        let params = FsWriteParams {
            path: path.into(),
            content: content.into(),
        };
        let resp = self.call(method::FS_WRITE_FILE, serde_json::to_value(params)?)?;
        if let Some(e) = resp.error {
            return Err(anyhow::anyhow!("write_file: {}", e.message));
        }
        Ok(())
    }

    /// Run a command inside the sandboxed child.
    pub fn exec(
        &mut self,
        command: &str,
        args: &[String],
        timeout_ms: Option<u64>,
    ) -> anyhow::Result<ExecResult> {
        let params = ExecParams {
            command: command.into(),
            args: args.to_vec(),
            cwd: None,
            timeout_ms,
        };
        let resp = self.call(method::EXEC, serde_json::to_value(params)?)?;
        if let Some(e) = resp.error {
            return Err(anyhow::anyhow!("exec: {}", e.message));
        }
        Ok(serde_json::from_value(resp.result.unwrap_or_default())?)
    }

    /// Current sandbox mode.
    pub fn mode(&self) -> SandboxMode {
        self.mode
    }
}

impl Drop for ExecServerClient {
    fn drop(&mut self) {
        // Best-effort clean shutdown, then reap.
        let _ = self
            .stdin
            .write_all(format!("{}\n", request_to_line(&RpcRequest::new(self.next_id, method::SHUTDOWN, serde_json::json!({})))).as_bytes());
        let _ = self.stdin.flush();
        let _ = self.child.wait();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enabled_flag_parses_env_value() {
        assert!(parse_enabled_flag("1"));
        assert!(parse_enabled_flag("true"));
        assert!(!parse_enabled_flag("0"));
        assert!(!parse_enabled_flag(""));
    }

    /// 端到端：真实 spawn exec-server 二进制。仅当
    /// `KIMIX_EXEC_SERVER_BIN` 指向已构建的二进制时运行，否则跳过。
    #[test]
    fn end_to_end_with_real_bin() {
        let Ok(bin) = std::env::var("KIMIX_EXEC_SERVER_BIN") else {
            eprintln!(
                "skipping end_to_end_with_real_bin: set KIMIX_EXEC_SERVER_BIN \
                 to a built kimix-exec-server"
            );
            return;
        };
        let ws = std::env::temp_dir().join(format!("kimix-exec-client-{}", std::process::id()));
        std::fs::create_dir_all(&ws).unwrap();
        let mut client = ExecServerClient::spawn(&ws, SandboxMode::WorkspaceWrite)
            .expect("spawn + initialize");
        let target = ws.join("via-client.txt");
        client
            .write_file(target.to_str().unwrap(), "through the sandbox")
            .unwrap();
        let content = client.read_file(target.to_str().unwrap()).unwrap();
        assert_eq!(content, "through the sandbox");
        let result = client
            .exec("/bin/echo", &["pong".to_string()], Some(5000))
            .unwrap();
        assert_eq!(result.exit_code, Some(0));
        assert_eq!(result.stdout.trim(), "pong");
        // read-only rejects writes
        drop(client);
        let mut ro = ExecServerClient::spawn(&ws, SandboxMode::ReadOnly).unwrap();
        let err = ro.write_file(target.to_str().unwrap(), "nope").unwrap_err();
        assert!(err.to_string().contains("read-only"), "got: {err}");
        let _ = bin;
        let _ = std::fs::remove_dir_all(&ws);
    }
}
