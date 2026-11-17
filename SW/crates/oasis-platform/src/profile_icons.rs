//! Port of the path rules of `Services/ProfileIconStore.cs`: a profile's icon is either an emoji,
//! a file bundled with the app, an absolute path, or a *managed* copy kept in the data folder's
//! `profile_icons/` folder (stored as `profile_icons/<id>.png`). Only managed icons are ours to
//! delete.
//!
//! Importing an icon (copying a chosen picture into the folder) comes with the icon picker.

use std::fs;
use std::path::{Component, Path, PathBuf};

use crate::paths::PROFILE_ICONS_FOLDER_NAME;

fn normalise(stored: &str) -> String {
    stored.replace('\\', "/").trim_start_matches('/').to_string()
}

/// Whether `stored` points into the managed icon folder (`profile_icons/…`).
pub fn is_managed(stored: &str) -> bool {
    let prefix = format!("{PROFILE_ICONS_FOLDER_NAME}/");
    let normalised = normalise(stored);
    !stored.trim().is_empty()
        && normalised.len() > prefix.len()
        && normalised[..prefix.len()].eq_ignore_ascii_case(&prefix)
}

/// The file a managed icon path names, if it names one inside `app_data_root/profile_icons` and
/// the file exists. A path that climbs out (`profile_icons/../profiles.json`) or names a drive is
/// refused, so a damaged or hostile settings file cannot make OASIS delete anything else.
pub fn resolve_managed(app_data_root: &Path, stored: &str) -> Option<PathBuf> {
    if !is_managed(stored) {
        return None;
    }
    let relative = normalise(stored);
    let safe = relative.split('/').all(|part| {
        !part.is_empty() && part != "." && part != ".." && !part.contains(':')
    });
    if !safe {
        return None;
    }
    let path = relative.split('/').fold(app_data_root.to_path_buf(), |p, part| p.join(part));
    // Belt and braces: every component of what we built is a plain name below the root.
    let below_root = path
        .strip_prefix(app_data_root)
        .is_ok_and(|rest| rest.components().all(|c| matches!(c, Component::Normal(_))));
    (below_root && path.is_file()).then_some(path)
}

/// Delete a managed icon file. `false` if the path is not a managed icon, the file is not there,
/// or it could not be deleted (best effort: a leftover icon is harmless).
pub fn delete_managed(app_data_root: &Path, stored: &str) -> bool {
    resolve_managed(app_data_root, stored).is_some_and(|path| fs::remove_file(path).is_ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("oasis-icons-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join(PROFILE_ICONS_FOLDER_NAME)).unwrap();
        dir
    }

    #[test]
    fn managed_paths_are_recognised_with_either_slash_and_any_case() {
        assert!(is_managed("profile_icons/abc.png"));
        assert!(is_managed(r"profile_icons\abc.png"));
        assert!(is_managed("Profile_Icons/abc.png"));
        assert!(!is_managed(""));
        assert!(!is_managed("profile_icons/"));
        assert!(!is_managed("icons/abc.png"));
        assert!(!is_managed(r"C:\pics\abc.png"));
        assert!(!is_managed("Assets/chrome.png"));
    }

    #[test]
    fn a_managed_icon_is_deleted_and_nothing_else() {
        let dir = root("delete");
        let icon = dir.join(PROFILE_ICONS_FOLDER_NAME).join("abc.png");
        let other = dir.join("profiles.json");
        fs::write(&icon, b"png").unwrap();
        fs::write(&other, b"{}").unwrap();

        assert!(delete_managed(&dir, "profile_icons/abc.png"));
        assert!(!icon.exists());
        assert!(other.exists());
        assert!(!delete_managed(&dir, "profile_icons/abc.png"), "already gone");
    }

    #[test]
    fn a_path_that_climbs_out_of_the_folder_is_never_touched() {
        let dir = root("climb");
        let victim = dir.join("profiles.json");
        fs::write(&victim, b"{}").unwrap();
        for hostile in [
            "profile_icons/../profiles.json",
            r"profile_icons\..\profiles.json",
            "profile_icons/./../profiles.json",
            "profile_icons//abc.png",
            "profile_icons/C:/Windows/x.png",
        ] {
            assert!(!delete_managed(&dir, hostile), "{hostile}");
        }
        assert!(victim.exists());
    }

    #[test]
    fn an_absolute_or_bundled_icon_is_not_ours_to_delete() {
        let dir = root("others");
        let outside = std::env::temp_dir().join(format!("oasis-icons-outside-{}.png", std::process::id()));
        fs::write(&outside, b"png").unwrap();
        assert!(!delete_managed(&dir, &outside.to_string_lossy()));
        assert!(outside.exists());
        assert!(!delete_managed(&dir, "Assets/chrome.png"));
        let _ = fs::remove_file(outside);
    }

    #[test]
    fn a_folder_is_not_deleted_as_if_it_were_an_icon() {
        let dir = root("folder");
        fs::create_dir_all(dir.join(PROFILE_ICONS_FOLDER_NAME).join("sub")).unwrap();
        assert!(!delete_managed(&dir, "profile_icons/sub"));
        assert!(dir.join(PROFILE_ICONS_FOLDER_NAME).join("sub").exists());
    }
}
