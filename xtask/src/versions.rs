//! Every app and library crate takes its version from the workspace, so that a release bumps
//! one line and `--version`, About, crash reports and bundles all name that release.

use std::path::Path;

#[test]
fn shipped_crates_inherit_the_workspace_version() -> Result<(), std::io::Error> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut checked = 0;
    let mut own_version = Vec::new();
    for dir in ["apps", "crates"] {
        for entry in std::fs::read_dir(root.join(dir))? {
            let manifest = entry?.path().join("Cargo.toml");
            let text = std::fs::read_to_string(&manifest)?;
            checked += 1;
            if !text.lines().any(|l| l.trim() == "version.workspace = true") {
                own_version.push(manifest);
            }
        }
    }
    assert!(checked > 0, "no manifests found under {}", root.display());
    assert!(
        own_version.is_empty(),
        "set `version.workspace = true` in {own_version:?}"
    );
    Ok(())
}
