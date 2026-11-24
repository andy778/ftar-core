//! Zip bundles of a tree, for bug reports.
//!
//! GitHub refuses `.fta` and `.ped` attachments but accepts `.zip`. A bundle
//! holds the tree, its `.ped` database, the trees it transfers in and any
//! saved reports (`.crp`/`.prp`/`.mrp`), all as plain OpenFTA files, plus a
//! `BUNDLE.txt` naming the main tree and the ftar version.
//! [`import`] unpacks such a bundle (or any zip of OpenFTA files).

use std::collections::BTreeSet;
use std::io::{Cursor, Read, Write};
use std::path::{Path, PathBuf};

use ftar_core::model::{EventType, Node};
use ftar_core::parse::parse_fta;
use ftar_core::project::ped_save_path;
use ftar_core::report::ReportKind;
use ftar_core::report_files;

const MANIFEST: &str = "BUNDLE.txt";
/// Refuse to unpack more than this many bytes or files from one bundle.
const MAX_TOTAL: u64 = 256 * 1024 * 1024;
const MAX_FILES: usize = 1000;

/// Name for the bundle of `tree`: `<tree>.zip` beside it.
pub fn default_bundle_path(tree: &Path) -> PathBuf {
    tree.with_extension("zip")
}

/// Bundle the tree at `tree` and what it needs; `version` is the ftar version
/// to record. Returns the zip file's bytes.
pub fn export(tree: &Path, version: &str) -> Result<Vec<u8>, String> {
    let dir = tree.parent().unwrap_or_else(|| Path::new("."));
    let main = tree
        .file_name()
        .ok_or_else(|| format!("{}: not a file", tree.display()))?
        .to_string_lossy()
        .into_owned();

    // (archive name, file on disk)
    let mut files: Vec<(String, PathBuf)> = Vec::new();
    let mut seen = BTreeSet::new();
    let mut add = |name: String, path: PathBuf, files: &mut Vec<(String, PathBuf)>| {
        if seen.insert(name.clone()) {
            files.push((name, path));
        }
    };
    add(main.clone(), tree.to_path_buf(), &mut files);

    let mut queue = vec![tree.to_path_buf()];
    let mut visited = BTreeSet::new();
    while let Some(t) = queue.pop() {
        let canon = t.canonicalize().unwrap_or_else(|_| t.clone());
        if !visited.insert(canon) {
            continue;
        }
        let bytes = std::fs::read(&t).map_err(|e| format!("{}: {e}", t.display()))?;
        let parsed = parse_fta(&bytes).map_err(|e| format!("{}: {e}", t.display()))?;
        let tdir = t.parent().unwrap_or_else(|| Path::new("."));
        if let Some(db) = parsed.db_name.as_deref() {
            let p = ped_save_path(tdir, db);
            if p.is_file() {
                // A shared database may live outside the tree's folder: it is
                // stored by file name, which is where loading looks next.
                add(archive_name(dir, &p), p, &mut files);
            }
        }
        let mut transfers = Vec::new();
        collect_transfers(&parsed.root, &mut transfers);
        for id in transfers {
            let p = tdir.join(&id);
            if p.is_file() {
                add(archive_name(dir, &p), p.clone(), &mut files);
                queue.push(p);
            }
        }
        for kind in [
            ReportKind::CutSets,
            ReportKind::Probability,
            ReportKind::MonteCarlo,
        ] {
            if let Some(r) = report_files::report_path(&t, kind).filter(|r| r.is_file()) {
                add(archive_name(dir, &r), r, &mut files);
            }
        }
    }

    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let opts = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    let manifest = format!(
        "ftar bundle\nftar version: {version}\nos: {} {}\ntree: {main}\n",
        std::env::consts::OS,
        std::env::consts::ARCH
    );
    let mut put = |name: &str, data: &[u8]| -> Result<(), String> {
        zip.start_file(name, opts).map_err(|e| e.to_string())?;
        zip.write_all(data).map_err(|e| e.to_string())
    };
    put(MANIFEST, manifest.as_bytes())?;
    for (name, path) in &files {
        let data = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        put(name, &data)?;
    }
    Ok(zip.finish().map_err(|e| e.to_string())?.into_inner())
}

/// Path inside the zip: relative to the tree's folder, with `/`; a file outside
/// that folder (a shared `.ped`) is stored under its file name.
fn archive_name(dir: &Path, p: &Path) -> String {
    let rel = p.strip_prefix(dir).ok().filter(|r| {
        r.components()
            .all(|c| matches!(c, std::path::Component::Normal(_)))
    });
    match rel {
        Some(r) => r
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("/"),
        None => p
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
    }
}

fn collect_transfers(n: &Node, out: &mut Vec<String>) {
    if n.kind == EventType::TransferIn {
        out.push(n.id.clone());
    }
    for c in &n.children {
        collect_transfers(c, out);
    }
}

/// What [`import`] unpacked.
#[derive(Debug, Clone)]
pub struct Imported {
    /// Every file written.
    pub files: Vec<PathBuf>,
    /// The main tree: the one `BUNDLE.txt` names, else the first `.fta`.
    pub tree: Option<PathBuf>,
}

/// Unpack the zip `bytes` into `dest` (created if missing). Existing files are
/// never overwritten: the import fails first, naming them. Entries whose path
/// would leave `dest` are refused.
pub fn import(bytes: &[u8], dest: &Path) -> Result<Imported, String> {
    let mut zip =
        zip::ZipArchive::new(Cursor::new(bytes)).map_err(|e| format!("not a zip file: {e}"))?;
    if zip.len() > MAX_FILES {
        return Err(format!("too many files in the bundle ({})", zip.len()));
    }
    let mut entries: Vec<(PathBuf, usize)> = Vec::new();
    let mut total = 0u64;
    for i in 0..zip.len() {
        let f = zip.by_index(i).map_err(|e| e.to_string())?;
        if f.is_dir() {
            continue;
        }
        let rel = f
            .enclosed_name()
            .ok_or_else(|| format!("unsafe path in the bundle: {}", f.name()))?;
        total += f.size();
        if total > MAX_TOTAL {
            return Err("the bundle unpacks to too much data".into());
        }
        entries.push((rel, i));
    }
    let clash: Vec<String> = entries
        .iter()
        .map(|(r, _)| dest.join(r))
        .filter(|p| p.exists())
        .map(|p| p.display().to_string())
        .collect();
    if !clash.is_empty() {
        return Err(format!(
            "would overwrite existing files: {}; pick an empty folder",
            clash.join(", ")
        ));
    }

    let mut files = Vec::new();
    let mut manifest_tree = None;
    for (rel, i) in entries {
        let mut f = zip.by_index(i).map_err(|e| e.to_string())?;
        // Trust the data read, not the declared size.
        let mut data = Vec::new();
        f.by_ref()
            .take(MAX_TOTAL + 1)
            .read_to_end(&mut data)
            .map_err(|e| format!("{}: {e}", rel.display()))?;
        if rel == Path::new(MANIFEST) {
            manifest_tree = String::from_utf8_lossy(&data)
                .lines()
                .find_map(|l| l.strip_prefix("tree: "))
                .map(|s| s.trim().to_string());
            continue;
        }
        let out = dest.join(&rel);
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        std::fs::write(&out, &data).map_err(|e| format!("{}: {e}", out.display()))?;
        files.push(out);
    }
    let is_fta = |p: &PathBuf| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("fta"));
    let tree = manifest_tree
        .map(|n| dest.join(n))
        .filter(|p| files.contains(p))
        .or_else(|| files.iter().find(|p| is_fta(p)).cloned());
    Ok(Imported { files, tree })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("ftar_bundle_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn data(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../core/tests/data")
            .join(name)
    }

    #[test]
    fn round_trip_theatre() {
        let src = temp("src");
        for n in ["theatre.fta", "theatre.ped"] {
            if data(n).is_file() {
                std::fs::copy(data(n), src.join(n)).unwrap();
            }
        }
        std::fs::write(src.join("theatre.mrp"), "report").unwrap();
        let zip = export(&src.join("theatre.fta"), "9.9.9").unwrap();
        let dest = temp("dest");
        let got = import(&zip, &dest).unwrap();
        assert_eq!(got.tree, Some(dest.join("theatre.fta")));
        for f in &got.files {
            let name = f.file_name().unwrap();
            assert_eq!(
                std::fs::read(f).unwrap(),
                std::fs::read(src.join(name)).unwrap()
            );
        }
        assert!(got.files.iter().any(|f| f.ends_with("theatre.mrp")));
        // The unpacked tree loads.
        ftar_core::project::Project::load(got.tree.unwrap()).unwrap();
        // A second import into the same folder is refused.
        assert!(import(&zip, &dest).unwrap_err().contains("overwrite"));
    }

    #[test]
    fn refuses_path_traversal() {
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        zip.start_file("../evil.txt", zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(b"x").unwrap();
        let bytes = zip.finish().unwrap().into_inner();
        let dest = temp("trav");
        assert!(import(&bytes, &dest).unwrap_err().contains("unsafe"));
        assert!(!dest.parent().unwrap().join("evil.txt").exists());
    }

    #[test]
    fn not_a_zip() {
        assert!(import(b"hello", &temp("nz")).is_err());
    }
}
