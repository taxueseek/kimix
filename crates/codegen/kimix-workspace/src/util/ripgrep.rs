#![allow(unexpected_cfgs)] // bundle_rg is set by the shell build script; harmless warning in the workspace lib

use std::path::PathBuf;
use std::sync::OnceLock;

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
    use std::{fs, os::unix::fs::PermissionsExt};
    let p = kimix_tools::util::kimix_home::kimix_home()
        .join("vendor")
        .join(concat!(
            "rg-",
            env!("KIMIX_SHELL_RG_VER"),
            "-",
            env!("KIMIX_SHELL_RG_TARGET")
        ));
    if !p.exists() {
        fs::create_dir_all(p.parent().unwrap())?;
        let tmp = p.with_extension("tmp");
        fs::write(&tmp, RG_BYTES)?;
        fs::rename(&tmp, &p)?;
        let mut perms = fs::metadata(&p)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&p, perms)?;
    }
    Ok(p)
}

pub fn rg_path() -> PathBuf {
    static RG_EXEC: OnceLock<PathBuf> = OnceLock::new();
    RG_EXEC
        .get_or_init(|| {
            #[cfg(bundle_rg)]
            {
                resolve_bundled_rg().unwrap_or_else(|_| PathBuf::from("rg"))
            }
            #[cfg(not(bundle_rg))]
            {
                PathBuf::from("rg")
            }
        })
        .clone()
}
