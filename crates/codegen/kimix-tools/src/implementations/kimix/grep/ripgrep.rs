use std::path::PathBuf;
use std::sync::OnceLock;

#[cfg(bundle_rg)]
const RG_BYTES: &[u8] = include_bytes!(concat!(
    env!("OUT_DIR"),
    "/bundle-rg/rg-",
    env!("KIMIX_TOOLS_RG_VER"),
    "-",
    env!("KIMIX_TOOLS_RG_TARGET"),
    ".bin"
));

/// Write bundled binary bytes into `~/.kimix/vendor/<vendor_name>` and make it executable.
/// Returns the path to the installed binary. No-op if the file already exists.
pub fn install_bundled_binary(bytes: &[u8], vendor_name: &str) -> std::io::Result<PathBuf> {
    use std::fs;
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;
    let p = crate::util::kimix_home().join("vendor").join(vendor_name);
    if !p.exists() {
        fs::create_dir_all(p.parent().unwrap())?;
        let tmp = p.with_extension("tmp");
        fs::write(&tmp, bytes)?;
        fs::rename(&tmp, &p)?;
        #[cfg(unix)]
        {
            let mut perms = fs::metadata(&p)?.permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&p, perms)?;
        }
    }
    Ok(p)
}

#[cfg(bundle_rg)]
fn resolve_bundled_rg() -> std::io::Result<PathBuf> {
    install_bundled_binary(
        RG_BYTES,
        concat!(
            "rg-",
            env!("KIMIX_TOOLS_RG_VER"),
            "-",
            env!("KIMIX_TOOLS_RG_TARGET")
        ),
    )
}

/// Resolve ripgrep via environment variables and filesystem heuristics.
/// Returns the fallback "rg" when nothing more specific is found.
pub fn resolve_rg_fallback() -> PathBuf {
    // RG_BIN_PATH: explicit override (tests / packaging can set this).
    if let Ok(p) = std::env::var("RG_BIN_PATH") {
        return PathBuf::from(p);
    }
    // Some hermetic test runners set RUNFILES_DIR and ship rg as a
    // data dependency rather than on PATH. Scan for a directory
    // entry containing "ripgrep_hermetic" and prefer arch-scoped
    // paths when present.
    if let Ok(rf) = std::env::var("RUNFILES_DIR") {
        let base = PathBuf::from(rf);
        if let Ok(entries) = std::fs::read_dir(&base) {
            for entry in entries.flatten() {
                let name = entry.file_name();
                if name.to_string_lossy().contains("ripgrep_hermetic") {
                    for sub in ["amd64/rg", "arm64/rg", "rg"] {
                        let candidate = entry.path().join(sub);
                        if candidate.exists() {
                            return candidate;
                        }
                    }
                }
            }
        }
    }
    PathBuf::from("rg")
}

/// Get the path to the ripgrep executable.
///
/// In release builds with bundling enabled, this extracts the bundled ripgrep
/// binary to ~/.kimix/vendor/ and returns that path.
/// Otherwise, resolves via `resolve_rg_fallback`.
pub fn rg_path() -> PathBuf {
    static RG_EXEC: OnceLock<PathBuf> = OnceLock::new();
    RG_EXEC
        .get_or_init(|| {
            #[cfg(bundle_rg)]
            {
                resolve_bundled_rg().unwrap_or_else(|_| resolve_rg_fallback())
            }
            #[cfg(not(bundle_rg))]
            {
                resolve_rg_fallback()
            }
        })
        .clone()
}
