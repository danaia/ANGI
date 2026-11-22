//! A folder attached to a message travels as a bounded text summary, not as a copy of its
//! contents: a layout, file-type counts, the largest files and the top of its key files. The
//! summary names the real path, so Project and OS agents can read further; Chat works from the
//! summary alone. Hidden items, build folders and symbolic links are left out.
use std::{collections::BTreeMap, fs, path::{Path, PathBuf}};

const MAX_DEPTH: usize = 4;
const MAX_LISTED: usize = 400;
const MAX_SCANNED: usize = 20_000;
const LARGEST: usize = 8;
const EXCERPT_LINES: usize = 30;
const EXCERPT_BYTES: usize = 1_500;
const SKIPPED: [&str; 7] = ["node_modules", "target", "__pycache__", "dist", "build", "vendor", "venv"];
const KEY_FILES: [&str; 8] = ["README.md", "README", "Cargo.toml", "package.json", "pyproject.toml", "Makefile", "go.mod", "index.html"];

#[derive(Default)]
struct Scan {
    lines: Vec<String>,
    listed_all: bool,
    files: usize,
    folders: usize,
    bytes: u64,
    capped: bool,
    kinds: BTreeMap<String, (usize, u64)>,
    largest: Vec<(u64, PathBuf)>,
}

fn size_text(bytes: u64) -> String {
    match bytes {
        b if b >= 1 << 20 => format!("{:.1} MB", b as f64 / 1_048_576.0),
        b if b >= 1 << 10 => format!("{} KB", b >> 10),
        b => format!("{b} B"),
    }
}

fn visible(name: &str) -> bool {
    !name.starts_with('.') && !SKIPPED.contains(&name)
}

fn walk(root: &Path, dir: &Path, depth: usize, scan: &mut Scan) {
    let Ok(read) = fs::read_dir(dir) else { return };
    let mut entries: Vec<_> = read.flatten().filter(|e| visible(&e.file_name().to_string_lossy())).collect();
    entries.sort_by_key(|e| (!e.file_type().is_ok_and(|t| t.is_dir()), e.file_name()));
    for entry in entries {
        if scan.files + scan.folders >= MAX_SCANNED {
            scan.capped = true;
            return;
        }
        let Ok(kind) = entry.file_type() else { continue };
        let name = entry.file_name().to_string_lossy().into_owned();
        let indent = "  ".repeat(depth);
        let listed = scan.lines.len() < MAX_LISTED;
        if kind.is_symlink() {
            if listed {
                scan.lines.push(format!("{indent}{name} (link, not followed)"));
            }
        } else if kind.is_dir() {
            scan.folders += 1;
            if listed {
                scan.lines.push(format!("{indent}{name}/"));
            }
            if depth + 1 < MAX_DEPTH {
                walk(root, &entry.path(), depth + 1, scan);
            } else if listed {
                scan.lines.push(format!("{indent}  … (deeper folders are counted but not listed)"));
                count_only(&entry.path(), scan);
            }
        } else if kind.is_file() {
            let size = entry.metadata().map_or(0, |m| m.len());
            scan.files += 1;
            scan.bytes += size;
            let ext = Path::new(&name).extension().map_or_else(|| "(none)".to_string(), |e| format!(".{}", e.to_string_lossy().to_lowercase()));
            let slot = scan.kinds.entry(ext).or_default();
            slot.0 += 1;
            slot.1 += size;
            scan.largest.push((size, entry.path().strip_prefix(root).unwrap_or(&entry.path()).to_path_buf()));
            scan.largest.sort_by(|a, b| b.0.cmp(&a.0));
            scan.largest.truncate(LARGEST);
            if listed {
                scan.lines.push(format!("{indent}{name} ({})", size_text(size)));
            }
        }
    }
}

/// Count what lies below the listing depth so the totals stay honest.
fn count_only(dir: &Path, scan: &mut Scan) {
    let Ok(read) = fs::read_dir(dir) else { return };
    for entry in read.flatten().filter(|e| visible(&e.file_name().to_string_lossy())) {
        if scan.files + scan.folders >= MAX_SCANNED {
            scan.capped = true;
            return;
        }
        let Ok(kind) = entry.file_type() else { continue };
        if kind.is_dir() {
            scan.folders += 1;
            count_only(&entry.path(), scan);
        } else if kind.is_file() {
            scan.files += 1;
            scan.bytes += entry.metadata().map_or(0, |m| m.len());
        }
    }
}

fn excerpt(path: &Path) -> Option<String> {
    let bytes = fs::read(path).ok()?;
    let text = std::str::from_utf8(&bytes[..bytes.len().min(EXCERPT_BYTES * 2)]).ok().or_else(|| std::str::from_utf8(&bytes[..bytes.len().min(EXCERPT_BYTES)]).ok())?;
    let mut out: String = text.lines().take(EXCERPT_LINES).collect::<Vec<_>>().join("\n");
    out.truncate(out.char_indices().nth(EXCERPT_BYTES).map_or(out.len(), |(i, _)| i));
    Some(out)
}

/// The Markdown summary the agent reads.
pub(super) fn manifest(root: &Path) -> String {
    let mut scan = Scan { listed_all: true, ..Scan::default() };
    walk(root, root, 0, &mut scan);
    scan.listed_all = scan.lines.len() < MAX_LISTED;
    let name = root.file_name().map_or_else(|| root.display().to_string(), |n| n.to_string_lossy().into_owned());
    let mut out = format!(
        "# Folder summary: {name}\n\nPath: {}\n\n{} files in {} folders, {} in total. Hidden items, build folders ({}) and links are left out.{}\n",
        root.display(), scan.files, scan.folders, size_text(scan.bytes), SKIPPED.join(", "),
        if scan.capped { " The scan stopped early at a safety limit, so counts are a minimum." } else { "" },
    );
    out.push_str("\n## File types\n");
    let mut kinds: Vec<_> = scan.kinds.iter().collect();
    kinds.sort_by(|a, b| b.1 .0.cmp(&a.1 .0).then(a.0.cmp(b.0)));
    for (ext, (n, bytes)) in kinds.into_iter().take(15) {
        out.push_str(&format!("- {ext}: {n} file{}, {}\n", if *n == 1 { "" } else { "s" }, size_text(*bytes)));
    }
    if !scan.largest.is_empty() {
        out.push_str("\n## Largest files\n");
        for (size, path) in &scan.largest {
            out.push_str(&format!("- {} ({})\n", path.display(), size_text(*size)));
        }
    }
    out.push_str("\n## Layout\n");
    out.push_str(&scan.lines.join("\n"));
    if !scan.listed_all {
        out.push_str(&format!("\n… listing stops after {MAX_LISTED} entries; use the path above to see the rest."));
    }
    for key in KEY_FILES {
        if let Some(text) = excerpt(&root.join(key)).filter(|t| !t.trim().is_empty()) {
            out.push_str(&format!("\n\n## {key} (start)\n```\n{text}\n```"));
        }
    }
    out.push('\n');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree() -> PathBuf {
        let root = std::env::temp_dir().join(format!("angi-folder-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("src")).unwrap();
        fs::create_dir_all(root.join("node_modules/dep")).unwrap();
        fs::create_dir_all(root.join(".git")).unwrap();
        fs::write(root.join("README.md"), "# Demo\nA tiny project.\n").unwrap();
        fs::write(root.join("src/main.rs"), "fn main() {}\n").unwrap();
        fs::write(root.join("src/lib.rs"), "").unwrap();
        fs::write(root.join("node_modules/dep/x.js"), "x").unwrap();
        fs::write(root.join(".git/HEAD"), "ref").unwrap();
        root
    }

    #[test]
    fn the_summary_describes_the_project_and_skips_hidden_and_build_folders() {
        let root = tree();
        let text = manifest(&root);
        assert!(text.contains("3 files in 1 folders"), "{text}");
        assert!(text.contains("- .rs: 2 files"));
        assert!(text.contains("main.rs ("));
        assert!(text.contains("## README.md (start)") && text.contains("A tiny project."));
        assert!(!text.contains("x.js") && !text.contains("HEAD"));
    }

    #[test]
    fn a_large_folder_is_listed_in_part_but_still_counted() {
        let root = std::env::temp_dir().join(format!("angi-folder-big-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        for n in 0..(MAX_LISTED + 25) {
            fs::write(root.join(format!("f{n:04}.txt")), "x").unwrap();
        }
        let text = manifest(&root);
        assert!(text.contains(&format!("{} files in 0 folders", MAX_LISTED + 25)));
        assert!(text.contains("listing stops after"));
    }
}
