//! Profile icons: the icon a profile shows, and the icons of programs and websites to choose from.

use super::*;

/// Give a profile a picture icon (a small PNG from the page), or remove it (`None`). The picture is
/// kept in the data folder's `profile_icons/`; the one it replaces is deleted if nothing else uses it.
#[tauri::command]
pub fn set_profile_icon(
    slot: State<'_, EngineSlot>,
    profile: usize,
    png: Option<Vec<u8>>,
) -> Result<(), String> {
    with_engine(&slot, |engine| {
        let root = engine.paths().app_data_root.clone();
        let old = engine
            .settings()
            .profiles
            .get(profile)
            .map(|p| p.icon_path.clone())
            .ok_or("No such profile.")?;
        let new_path = match png {
            None => String::new(),
            Some(bytes) => {
                if bytes.len() > 512 * 1024 || !bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
                    return Err("That is not a small PNG picture.".into());
                }
                let folder = engine.paths().profile_icons_folder();
                std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
                let id = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |d| d.as_millis());
                std::fs::write(folder.join(format!("{id}.png")), bytes)
                    .map_err(|e| e.to_string())?;
                format!("profile_icons/{id}.png")
            }
        };
        engine
            .change(|s| s.set_profile_icon_path(profile, &new_path))
            .map_err(|e| e.to_string())?;
        let still_used = engine
            .settings()
            .profiles
            .iter()
            .any(|p| p.icon_path == old);
        if !still_used {
            oasis_platform::profile_icons::delete_managed(&root, &old);
        }
        Ok(())
    })
}

/// The picture icons profiles have, as data addresses the page can show, by profile number.
#[tauri::command]
pub fn profile_icons(slot: State<'_, EngineSlot>) -> std::collections::BTreeMap<usize, String> {
    use base64::Engine as _;
    with_engine(&slot, |engine| {
        let root = engine.paths().app_data_root.clone();
        Ok(engine
            .settings()
            .profiles
            .iter()
            .enumerate()
            .filter_map(|(i, p)| {
                let file = oasis_platform::profile_icons::resolve_managed(&root, &p.icon_path)?;
                let bytes = std::fs::read(file).ok()?;
                Some((
                    i,
                    format!(
                        "data:image/png;base64,{}",
                        base64::engine::general_purpose::STANDARD.encode(bytes)
                    ),
                ))
            })
            .collect())
    })
    .unwrap_or_default()
}

/// A program's own icon as a 64-pixel PNG. `target` is a file path, or the name of a program that is
/// open or installed.
#[tauri::command]
pub async fn app_icon(target: String) -> Result<Vec<u8>, String> {
    #[cfg(windows)]
    {
        tauri::async_runtime::spawn_blocking(move || {
            use base64::Engine as _;
            use std::os::windows::process::CommandExt;
            let target = target.split(',').next().unwrap_or("").trim().to_string();
            if target.is_empty() {
                return Err("Say which app first.".to_string());
            }
            // A file path as it is; else an open program of that name; else the installed list.
            let path = if std::path::Path::new(&target).is_file() {
                target.clone()
            } else {
                // Names are compared as letters and digits only, so "apple music" finds AppleMusic.exe.
                let squash = |s: &str| s.trim_end_matches(".exe").chars().filter(|c| c.is_alphanumeric()).collect::<String>().to_lowercase();
                let want = squash(&target);
                let exact = |n: &str| squash(n) == want;
                let loose = |n: &str| want.len() >= 3 && (squash(n).contains(&want) || (squash(n).len() >= 3 && want.contains(&squash(n))));
                let open = running_programs();
                let installed = oasis_platform::windows::app_discovery::windows_apps().installed_apps();
                open.iter().find(|r| exact(&r.name)).map(|r| r.path.clone())
                    .or_else(|| installed.iter().find(|(n, _)| exact(n)).map(|(_, p)| p.clone()))
                    .or_else(|| open.iter().find(|r| loose(&r.name)).map(|r| r.path.clone()))
                    .or_else(|| installed.iter().find(|(n, _)| loose(n)).map(|(_, p)| p.clone()))
                    .ok_or("OASIS cannot find that program to read its icon.")?
            };
            const SCRIPT: &str = "Add-Type -AssemblyName System.Drawing; $i = [System.Drawing.Icon]::ExtractAssociatedIcon($env:OASIS_ICON_FILE); if (-not $i) { exit 1 }; $b = New-Object System.Drawing.Bitmap 64, 64; $g = [System.Drawing.Graphics]::FromImage($b); $g.InterpolationMode = 'HighQualityBicubic'; $g.DrawImage($i.ToBitmap(), 0, 0, 64, 64); $m = New-Object System.IO.MemoryStream; $b.Save($m, [System.Drawing.Imaging.ImageFormat]::Png); [Convert]::ToBase64String($m.ToArray())";
            let out = std::process::Command::new("powershell")
                .args(["-NoProfile", "-NonInteractive", "-Command", SCRIPT])
                .env("OASIS_ICON_FILE", &path)
                .creation_flags(0x0800_0000)
                .output()
                .map_err(|e| e.to_string())?;
            let text = String::from_utf8_lossy(&out.stdout);
            base64::engine::general_purpose::STANDARD
                .decode(text.trim())
                .map_err(|_| "That program has no icon OASIS can read.".to_string())
        })
        .await
        .map_err(|e| e.to_string())?
    }
    #[cfg(not(windows))]
    {
        let _ = target;
        Err("App icons are only read on Windows.".into())
    }
}

/// A website's logo as a 64-pixel PNG: the icon its page names, else `/favicon.ico`, fetched from the
/// site itself (no third party is asked).
#[tauri::command]
pub async fn site_icon(host: String) -> Result<Vec<u8>, String> {
    #[cfg(windows)]
    {
        tauri::async_runtime::spawn_blocking(move || {
            use base64::Engine as _;
            use std::os::windows::process::CommandExt;
            // A pasted address is cut down to its host; a bare name ("instagram") is taken as .com.
            let host = host.trim().to_lowercase();
            let host = host.split("://").last().unwrap_or("").split(['/', '?', '#', ':']).next().unwrap_or("").to_string();
            let host = if !host.is_empty() && !host.contains('.') { format!("{host}.com") } else { host };
            let plain = !host.is_empty()
                && host.len() <= 253
                && host.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-');
            if !plain {
                return Err("That is not a website address.".to_string());
            }
            // The script is passed encoded, so its quotes and new lines reach PowerShell untouched.
            let script: Vec<u8> = include_str!("site_icon.ps1").encode_utf16().flat_map(u16::to_le_bytes).collect();
            let out = std::process::Command::new("powershell")
                .args(["-NoProfile", "-NonInteractive", "-EncodedCommand", &base64::engine::general_purpose::STANDARD.encode(script)])
                .env("OASIS_SITE", &host)
                .creation_flags(0x0800_0000)
                .output()
                .map_err(|e| e.to_string())?;
            base64::engine::general_purpose::STANDARD
                .decode(String::from_utf8_lossy(&out.stdout).trim())
                .map_err(|_| "That site has no icon OASIS can read.".to_string())
        })
        .await
        .map_err(|e| e.to_string())?
    }
    #[cfg(not(windows))]
    {
        let _ = host;
        Err("Site icons are only read on Windows.".into())
    }
}
