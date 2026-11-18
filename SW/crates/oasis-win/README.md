# oasis-win

## Overview
Safe wrappers over the Win32 calls OASIS needs (`windows-sys`, plus the `windows` crate for UI Automation, media sessions and network state). Modules are `#[cfg(windows)]`, so the crate is empty on other OSes. With `oasis-dpapi`, one of the crates meant for `unsafe` (D13; `oasis-voice` also allows it for its speech engine binding); each block has a `SAFETY:` comment and the public API is safe.

## Interface
- `window`: `foreground_window`, `window_title`, `window_process_id`, `process_name`, `visible_windows`, `close_program`, `WindowId`.
- `desktop`: `app_windows`, `foreground`, `monitors`, `cursor_monitor`, `read`, `restore`, `maximize`, `minimize`, `close`, `raise_topmost`, `set_outer`; `Rect`, `AppWindow`, `Monitor`, `Placement`.
- `focus`: `is_window`, `main_windows_of`, `main_windows_where`, `activate`, `wait_until_front`, `wait_input_idle`.
- `input`: `send_key`, `send_text`, `click_at`, `KeyEvent`, `MouseButton`, `InputError`, `OASIS_KEY_TAG`.
- `keyboard`: `KeyboardHook::start`, `modifiers_held`, `is_down`, `send_foreign_key`, `Handler`.
- `watch`: `FolderWatch::start(folder, on_change)`: calls `on_change` from its own thread (`oasis-folder-watch`) after files directly in the folder were written, renamed, added or removed, once 150 ms of quiet has passed. Dropping it stops and joins the thread. The engine uses it to notice outside edits of `profiles.json`.
- `job`: `usage` (private memory and handle count of a process), `end_with_this_process` (a helper program such as the polish server ends when OASIS ends, however it ends), `end_strays` (ends copies left by an older OASIS).
- `clipboard`: `set_text` (kept out of Windows clipboard history, cloud clipboard sync and clipboard managers that honour the flags; retried up to 10 times when busy), `get_text`, `sequence`, `clipboard_units`, `ClipboardError`.
- `shell`: `open`, `shortcut_target`, `default_browser`, `ask_yes_no`.
- `uia`: `FieldReader` (`edit_fields`, `editable_focus`, `focused_text`, `focus_chain`, `scan_window`). `uia_panel`: `focus_chain`, `scan_window`, `control_type_name`, `RawNode`, `RawScanNode`.
- `hardware::read` (`Hardware`), `network::connection` (`Connection`), `power::status` (`Power`).
- `system`: `is_elevated`, `current_user_sid`, `run_elevated_and_wait`, `SingleInstance` (`try_own`, `is_held`, `signal_show`, `watch_show_requests`), `run_key_value`, `set_run_key_value`, `app_paths`, `utc_offset_seconds`, `beep`, `default_browser_exe`, `executable_of`, `play_wav`.
- `audio`: `play_wav_blocking`, `stop`. `media`: `pause_playing`, `resume`.
- Example: `examples/focused_text.rs` (`cargo run -p oasis-win --example focused_text -- 6`) prints a focused field's text.

## Invariants
- Public functions are safe; `unsafe` stays inside this crate with `SAFETY:` notes.
- Keys injected by `input` carry `OASIS_KEY_TAG`.

## Side Effects
- Starts threads: `oasis-folder-watch` (`FolderWatch`), `oasis-key-listener` (`KeyboardHook`), `oasis-single-instance` (`watch_show_requests`), `oasis-uia` (`FieldReader`, one UI Automation thread that answers calls with a timeout), and a short-lived thread per media call (waited on at once). Injects keyboard and mouse input, installs a keyboard hook (`KeyboardHook`), changes window state, writes the clipboard, launches programs, writes the HKCU Run key, creates a named mutex, plays sound, reads hardware and network state.

## Verification
- Run: `cargo test -p oasis-win` (Windows only)
