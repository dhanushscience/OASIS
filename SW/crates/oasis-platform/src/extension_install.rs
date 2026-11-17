//! Publishes a stable copy of the browser extension under the local data folder. Port of
//! `Services/ExtensionInstallService.cs`. Loading it from a build folder or OneDrive often breaks
//! MV3 service workers in Brave/Chrome, so the browser is pointed at this copy instead.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::paths::OasisPaths;

pub const EXTENSION_FOLDER_NAME: &str = "oasis-youtube";

/// Where the browser should load the extension from.
pub fn published_folder(paths: &OasisPaths) -> PathBuf {
    paths.extensions_root().join(EXTENSION_FOLDER_NAME)
}

/// Where a bundled copy may be, nearest first: beside the program, in the source tree the
/// program was built from, and in the working folder.
pub fn bundled_source_candidates(exe: &Path, working_dir: &Path) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(dir) = exe.parent() {
        candidates.push(dir.join("extensions").join(EXTENSION_FOLDER_NAME));
        candidates.push(dir.join("Extensions").join(EXTENSION_FOLDER_NAME));
        // target/debug/oasis-desktop.exe → the workspace's extensions/ folder.
        if let Some(workspace) = dir.parent().and_then(Path::parent) {
            candidates.push(workspace.join("extensions").join(EXTENSION_FOLDER_NAME));
        }
    }
    candidates.push(working_dir.join("extensions").join(EXTENSION_FOLDER_NAME));
    candidates
}

fn is_extension(folder: &Path) -> bool {
    folder.join("manifest.json").is_file()
}

/// Copy the bundled extension into the published folder, unless that folder already holds a newer
/// version (never clobber a newer manually published extension with an older build). Returns the
/// published folder, or `None` if there is neither a bundled copy nor a published one.
pub fn ensure_published(paths: &OasisPaths, bundled: &[PathBuf]) -> Option<PathBuf> {
    let published = published_folder(paths);
    let Some(source) = bundled.iter().find(|p| is_extension(p)) else {
        return is_extension(&published).then_some(published);
    };

    let (source_version, published_version) = (manifest_version(source), manifest_version(&published));
    if let (Some(have), Some(offered)) = (&published_version, &source_version) {
        if compare_versions(have, offered).is_gt() {
            return Some(published);
        }
    }

    let copied = copy_extension(source, &published);
    match copied {
        Ok(()) => Some(published),
        Err(_) => is_extension(&published).then_some(published),
    }
}

fn copy_extension(source: &Path, destination: &Path) -> io::Result<()> {
    fs::create_dir_all(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let name = entry.file_name();
        let target = destination.join(&name);
        if entry.file_type()?.is_dir() {
            copy_extension(&entry.path(), &target)?;
        } else if !name.to_string_lossy().eq_ignore_ascii_case("README.md") {
            fs::copy(entry.path(), target)?;
        }
    }
    let _ = fs::write(destination.join("LOAD_ME.txt"), LOAD_ME);
    Ok(())
}

const LOAD_ME: &str = "OASIS Bridge: load this folder in Brave/Chrome/Edge:\r\n\
1. Open brave://extensions (or chrome://extensions)\r\n\
2. Enable Developer mode\r\n\
3. Load unpacked, then select THIS folder\r\n\
4. OASIS must be running with Browser Bridge ENABLED\r\n\
5. The badge should show ON; the OASIS status should show a connected browser\r\n\
6. Allow site access for pages you want OASIS to read\r\n";

/// The `"version"` of an extension's manifest.
fn manifest_version(folder: &Path) -> Option<String> {
    let text = fs::read_to_string(folder.join("manifest.json")).ok()?;
    let manifest: serde_json::Value = serde_json::from_str(&text).ok()?;
    manifest.get("version")?.as_str().map(|v| v.trim().to_string())
}

/// Compare dotted numeric versions part by part; a missing or non-numeric part counts as 0.
fn compare_versions(a: &str, b: &str) -> std::cmp::Ordering {
    let parts = |v: &str| v.split('.').map(|p| p.trim().parse::<u64>().unwrap_or(0)).collect::<Vec<_>>();
    let (a, b) = (parts(a), parts(b));
    (0..a.len().max(b.len()))
        .map(|i| a.get(i).unwrap_or(&0).cmp(b.get(i).unwrap_or(&0)))
        .find(|o| o.is_ne())
        .unwrap_or(std::cmp::Ordering::Equal)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cmp::Ordering;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("oasis-ext-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn make_extension(folder: &Path, version: &str) {
        fs::create_dir_all(folder.join("icons")).unwrap();
        fs::write(folder.join("manifest.json"), format!(r#"{{"version": "{version}"}}"#)).unwrap();
        fs::write(folder.join("background.js"), version).unwrap();
        fs::write(folder.join("README.md"), "dev notes").unwrap();
        fs::write(folder.join("icons").join("icon16.png"), "png").unwrap();
    }

    #[test]
    fn versions_compare_part_by_part() {
        assert_eq!(compare_versions("1.3.1", "1.3.0"), Ordering::Greater);
        assert_eq!(compare_versions("1.10", "1.9"), Ordering::Greater, "numbers, not text");
        assert_eq!(compare_versions("1.3", "1.3.0"), Ordering::Equal);
        assert_eq!(compare_versions("1.2.9", "1.3"), Ordering::Less);
    }

    #[test]
    fn the_bundled_extension_is_published_without_its_readme_and_with_load_instructions() {
        let root = scratch("publish");
        let paths = OasisPaths::with_roots(root.join("roaming"), root.join("local"));
        let bundled = root.join("bundled");
        make_extension(&bundled, "1.3.1");
        let published = ensure_published(&paths, &[bundled]).expect("published");
        assert_eq!(published, published_folder(&paths));
        assert!(published.join("manifest.json").is_file() && published.join("icons").join("icon16.png").is_file());
        assert!(!published.join("README.md").exists());
        assert!(fs::read_to_string(published.join("LOAD_ME.txt")).unwrap().contains("Load unpacked"));
    }

    #[test]
    fn a_newer_published_copy_is_never_overwritten_by_an_older_one() {
        let root = scratch("newer");
        let paths = OasisPaths::with_roots(root.join("roaming"), root.join("local"));
        make_extension(&published_folder(&paths), "1.4.0");
        let old = root.join("old");
        make_extension(&old, "1.3.1");
        ensure_published(&paths, &[old]).unwrap();
        assert_eq!(fs::read_to_string(published_folder(&paths).join("background.js")).unwrap(), "1.4.0");
    }

    #[test]
    fn an_older_published_copy_is_updated() {
        let root = scratch("older");
        let paths = OasisPaths::with_roots(root.join("roaming"), root.join("local"));
        make_extension(&published_folder(&paths), "1.2.0");
        let new = root.join("new");
        make_extension(&new, "1.3.1");
        ensure_published(&paths, &[new]).unwrap();
        assert_eq!(fs::read_to_string(published_folder(&paths).join("background.js")).unwrap(), "1.3.1");
    }

    #[test]
    fn without_any_source_the_existing_copy_is_used_or_there_is_none() {
        let root = scratch("none");
        let paths = OasisPaths::with_roots(root.join("roaming"), root.join("local"));
        assert_eq!(ensure_published(&paths, &[root.join("missing")]), None);
        make_extension(&published_folder(&paths), "1.0.0");
        assert_eq!(ensure_published(&paths, &[root.join("missing")]), Some(published_folder(&paths)));
    }

    #[test]
    fn the_source_tree_is_found_from_a_build_folder() {
        let candidates = bundled_source_candidates(Path::new(r"C:\w\SW\target\debug\oasis-desktop.exe"), Path::new(r"C:\x"));
        assert!(candidates.contains(&PathBuf::from(r"C:\w\SW\extensions\oasis-youtube")));
        assert_eq!(candidates.first().unwrap(), &PathBuf::from(r"C:\w\SW\target\debug\extensions\oasis-youtube"));
    }
}
