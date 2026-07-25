//! `KIMIX_SHARE_DIR` override tests in an isolated binary so `kimix_home()`'s
//! process-wide `OnceLock` initializes from the overridden env var.
use std::path::PathBuf;

#[test]
fn kimix_home_override_path_helpers() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let kimix_home = tmp.path().to_path_buf();
    unsafe {
        std::env::set_var("KIMIX_SHARE_DIR", &kimix_home);
    }

    assert_eq!(
        kimix_tui::util::pager_toml_path(),
        kimix_home.join("pager.toml")
    );
    assert_eq!(
        kimix_tui::util::display_kimix_home_prefix(),
        "$KIMIX_SHARE_DIR"
    );
    assert_eq!(
        kimix_tui::util::display_user_kimix_path("config.toml"),
        "$KIMIX_SHARE_DIR/config.toml"
    );

    let memory_path = kimix_home.join("memory/MEMORY.md");
    assert_eq!(
        kimix_tui::util::abbreviate_path(&memory_path.display().to_string()),
        "$KIMIX_SHARE_DIR/memory/MEMORY.md"
    );

    assert!(kimix_tui::util::is_under_user_kimix_home(&memory_path));
    assert!(!kimix_tui::util::is_under_user_kimix_home(
        PathBuf::from("/tmp/other").as_path()
    ));
}
