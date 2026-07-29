#![allow(unexpected_cfgs)] // bundle_rg is set by the shell build script; harmless warning in the workspace lib

use std::path::PathBuf;
use std::sync::OnceLock;

#[cfg(bundle_rg)]
use kimix_tools::implementations::kimix::grep::ripgrep::install_bundled_binary;
use kimix_tools::implementations::kimix::grep::ripgrep::resolve_rg_fallback;

#[cfg(bundle_rg)]
const RG_BYTES: &[u8] = include_bytes!(concat!(
    env!("KIMIX_SHELL_RG_GEN_DIR"),
    "/rg-",
    env!("KIMIX_SHELL_RG_VER"),
    "-",
    env!("KIMIX_SHELL_RG_TARGET"),
    ".bin"
));

#[cfg(bundle_rg)]
fn resolve_bundled_rg() -> std::io::Result<PathBuf> {
    install_bundled_binary(
        RG_BYTES,
        concat!(
            "rg-",
            env!("KIMIX_SHELL_RG_VER"),
            "-",
            env!("KIMIX_SHELL_RG_TARGET")
        ),
    )
}

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
