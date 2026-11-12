use std::path::{Path, PathBuf};

use anyhow::{Result, bail};

pub struct Plan {
    pub before_root: PathBuf,
    pub after_root: PathBuf,
    /// `(before, after)` paths relative to their roots.
    pub pairs: Vec<(PathBuf, PathBuf)>,
    pub only_before: Vec<PathBuf>,
    pub only_after: Vec<PathBuf>,
}

/// Pair the pages to compare. Two files are compared directly, with each file's
/// directory acting as its site root; two directories are compared page by page,
/// matching `*.html` files by relative path.
pub fn plan(before: &Path, after: &Path) -> Result<Plan> {
    for p in [before, after] {
        if !p.exists() {
            bail!("{} does not exist", p.display());
        }
    }
    match (before.is_dir(), after.is_dir()) {
        (true, true) => {
            let b = html_files(before)?;
            let a = html_files(after)?;
            Ok(Plan {
                before_root: before.to_path_buf(),
                after_root: after.to_path_buf(),
                pairs: b
                    .iter()
                    .filter(|p| a.contains(p))
                    .map(|p| (p.clone(), p.clone()))
                    .collect(),
                only_before: b.iter().filter(|p| !a.contains(p)).cloned().collect(),
                only_after: a.iter().filter(|p| !b.contains(p)).cloned().collect(),
            })
        }
        (false, false) => {
            let root = |p: &Path| match p.parent() {
                Some(d) if !d.as_os_str().is_empty() => d.to_path_buf(),
                _ => PathBuf::from("."),
            };
            let name = |p: &Path| PathBuf::from(p.file_name().unwrap_or_default());
            Ok(Plan {
                before_root: root(before),
                after_root: root(after),
                pairs: vec![(name(before), name(after))],
                only_before: vec![],
                only_after: vec![],
            })
        }
        _ => bail!("compare a file with a file, or a directory with a directory"),
    }
}

fn html_files(root: &Path) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir)? {
            let path = entry?.path();
            if path.is_dir() {
                stack.push(path);
            } else if matches!(
                path.extension().and_then(|e| e.to_str()),
                Some("html" | "htm")
            ) {
                out.push(path.strip_prefix(root)?.to_path_buf());
            }
        }
    }
    out.sort();
    Ok(out)
}
