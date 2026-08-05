//! Exec-server binary: apply the kernel sandbox to this process and serve
//! filesystem/exec JSON-RPC over stdio.
//!
//! Spawned by the agent (kimix-shell) as a child; the sandbox is applied to
//! this process before any request is served.

fn main() {
    // Keep stderr for diagnostics; the protocol rides stdin/stdout only.
    if let Err(e) = kimix_exec_server::serve() {
        eprintln!("kimix-exec-server: fatal: {e}");
        std::process::exit(1);
    }
}
