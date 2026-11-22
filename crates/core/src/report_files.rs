//! Report files next to a tree, named as OpenFTA names them.
//!
//! OpenFTA's analyses write `<tree>.crp` (qualitative / cut sets),
//! `<tree>.prp` (probability) and `<tree>.mrp` (Monte Carlo) beside the
//! `.fta` file; Report > View reopens them without running the analysis again.
//! The suffix replaces the tree's extension (`generate_filename` in
//! `FileUtilities.c`; `MCS_REPORT_SUFFIX` etc. in `fta.h`).

use std::path::{Path, PathBuf};

use crate::report::ReportKind;

/// The file suffix OpenFTA uses for `kind`; `None` for report kinds OpenFTA
/// has no file for.
pub fn suffix(kind: ReportKind) -> Option<&'static str> {
    match kind {
        ReportKind::CutSets => Some("crp"),
        ReportKind::Probability => Some("prp"),
        ReportKind::MonteCarlo => Some("mrp"),
        ReportKind::FailureStatistics => None,
    }
}

/// Path of the report file of `kind` for the tree at `tree`.
pub fn report_path(tree: &Path, kind: ReportKind) -> Option<PathBuf> {
    Some(tree.with_extension(suffix(kind)?))
}

/// Whether the report file for `kind` exists next to `tree`.
pub fn exists(tree: &Path, kind: ReportKind) -> bool {
    report_path(tree, kind).is_some_and(|p| p.is_file())
}

/// Write the report text next to the tree; returns the file written.
pub fn write(tree: &Path, kind: ReportKind, text: &str) -> Result<PathBuf, String> {
    let path = report_path(tree, kind).ok_or("this report has no file")?;
    std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(path)
}

/// Read the saved report (lossy UTF-8, as reports may hold Latin-1 names).
pub fn read(tree: &Path, kind: ReportKind) -> Result<String, String> {
    let path = report_path(tree, kind).ok_or("this report has no file")?;
    std::fs::read(&path)
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .map_err(|e| format!("{}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_follow_openfta() {
        let t = Path::new("/a/b/theatre.fta");
        assert_eq!(
            report_path(t, ReportKind::CutSets).unwrap(),
            Path::new("/a/b/theatre.crp")
        );
        assert_eq!(
            report_path(t, ReportKind::Probability).unwrap(),
            Path::new("/a/b/theatre.prp")
        );
        assert_eq!(
            report_path(t, ReportKind::MonteCarlo).unwrap(),
            Path::new("/a/b/theatre.mrp")
        );
        assert!(report_path(t, ReportKind::FailureStatistics).is_none());
    }

    #[test]
    fn round_trip_and_missing() {
        let d = std::env::temp_dir().join(format!("ftar_rf_{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let t = d.join("x.fta");
        assert!(!exists(&t, ReportKind::Probability));
        assert!(read(&t, ReportKind::Probability).is_err());
        write(&t, ReportKind::Probability, "hello").unwrap();
        assert!(exists(&t, ReportKind::Probability));
        assert_eq!(read(&t, ReportKind::Probability).unwrap(), "hello");
        let _ = std::fs::remove_dir_all(&d);
    }
}
