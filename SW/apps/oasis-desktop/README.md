# `apps/oasis-desktop/` — the desktop app

## Overview

**What:** the program users will run. Today it is a tray icon, a **Status** screen, a **Profiles**
screen (buttons, several steps, add/move/delete profiles, sets of buttons) and the always-on-top
**overlay** (pad indicators and button labels) around the engine. Later: the overlay's "listening"
states and the rest of the settings window (icons, voice, AI, …), which together replace
`OasisShell.csproj` (`App.xaml`, `MainWindow`, `Controls/CenterTray*`, `Windows/*`,
`Services/OverlayService.cs`, `TrayService.cs`).

**Why Tauri 2:** Tauri gives a native window and tray with a web page inside, the fastest way to
rebuild the large settings window (DECISIONS D7). The page is Svelte + Vite, source in `web/`;
`npm install` and `npm run build` there write the finished page into `ui/`, and **those built
files are committed**, so `cargo build -p oasis-desktop` needs no Node. Rebuild the page whenever
`web/` changes and commit `ui/` with it. An installer will need the Tauri CLI later.

**When it runs:** you start it by hand for now. Start with Windows and a single-copy guard are
not built yet.

**How it works:**
1. `src/main.rs` starts `oasis-engine` on its own threads and forwards every engine event to the
   page.
2. The page (`web/`, built into `ui/app.html`) shows the pad's status and battery, your last press, and an activity
   list, and has the **Run actions** switch.
3. **It starts in preview:** presses are shown but nothing runs until the switch is on. This is
   deliberate while the Rust build is still replacing the C# app. If `profiles.json` fails its
   signature check the switch is disabled and nothing runs.
4. Closing the window hides it; the app keeps running in the tray. **Quit** in the tray menu stops
   the engine and leaves.
5. If the engine cannot start (no `profiles.json`, no Bluetooth adapter), the window says why.

**The Profiles screen** (`web/src/legacy/`, commands in `src/main.rs`):
- Picks a profile; edits its name, what it applies to (`*` = everything) and whether that is an app
  or a website; shows each set of buttons and lets you edit the three buttons.
- Every save goes through `engine.change(...)`, which checks the value with the same rules the
  engine runs actions by, saves safely (signed, last-good copy), and puts the change in force: the
  pad uses it from the next press.
- A refused save says why, in plain words, and changes nothing: an unparseable shortcut, a link the
  allowlist blocks, a malformed mouse position, a profile with no name, **or the file having been
  changed by another program since OASIS read it** (press *Reload from file*, then save again).
- **New profile…** asks what it applies to (an app name, a program path, or a website; a bare site
  name like "netflix" is looked up and becomes `netflix.com`; empty means "everything else"),
  whether it is an app or a website, and an optional name. It starts with three blank buttons.
- **Move up / Move down** reorder the list. The first profile that matches wins, so order matters.
- **Delete** asks first ("Keep it" / "Yes, delete"). The last profile cannot be deleted. Deleting
  the only "everything else" profile adds a warning. The profile's icon file is removed too, unless
  another profile uses it.
- **Several steps:** choosing "Several steps" for a button shows a step list. Each step has a type
  (Quick Launch, LaunchApp, OpenUrl, Shortcut, AutoHotkey, MouseClick, JoinMeeting), a value, and
  buttons to move it up or down or delete it. **Add step** appends a 1000 ms delay. Saving needs at
  least one step and checks each (the reason names the step: "Step 2: Unsupported key 'Nope'").
  A step of a kind this screen cannot make (made by an AI plan) is shown locked and kept as is.
- The label is shown on the pad in capitals, up to 14 characters; a longer one is shortened.
- **Sets of buttons:** a profile's "extra sets" switch turns on a list of sets. Add one with a name
  and a condition (window title text, or a panel, or a YouTube state), rename or re-condition it,
  delete it (asks first). The main set has no condition and cannot be deleted; switching extra sets
  off keeps them. The Rust build does not yet detect panels or YouTube states, so it still uses the
  main set when it runs.
- Not yet: icons and emojis, a profile's label behaviour, the other settings.

**The overlay** (`ui/overlay.html`, `src/overlay.rs`, DECISIONS D18):
- A second window, 300×240, transparent, always on top, never focused, clicks pass through. It sits
  where `OverlayX/OverlayY` say, or at the right edge of the main screen, vertically centred; a saved
  spot that no screen shows any more falls back to that default.
- It listens to the same `engine-event` stream: `pad_button` thickens a bar while the pad button is
  held; `show_labels` slides in the three labels; `hide_labels` (time up, or an action ran) removes
  them; `settings_changed` re-reads its look.
- Its look (`overlay_look` command) comes from `OverlayAppearance` in `profiles.json`, checked by
  `oasis-settings::overlay`: bad colours or sizes fall back to the C# defaults.
- **Move overlay** (tray menu, or the button on the Status screen; both call `move_overlay`) makes
  it a solid, draggable box; **Done moving** saves the spot with the normal safe save. Checked with
  a real mouse drag.
- The first tap on a button with an action only shows the labels (when label showing is on for the
  profile); a second tap within 4 seconds runs it. Decided in the engine, not here.
- Capability file `capabilities/overlay.json` gives this window only event listening and dragging.

**Ways of pressing the buttons (the Input page):** the pad is one of them. The other built so far is the
keyboard: three shortcuts, one per button, listened for system-wide by a low-level keyboard hook
(`oasis_win::keyboard`) that exists only while that switch is on, reports only the chosen shortcuts, and is
logged to `security.log` when switched on or off. Keys OASIS sends itself are stamped and ignored, so it
cannot trigger itself. Chosen shortcuts are kept from the program in front unless "keep these keys" is off.
Both are judged by the same gesture rules as the pad, so tap, hold and double-tap work the same. At least one
way must stay on. Planned for version 2: phone as a deck, trackpad regions, mouse ring menu.

**Its own profiles:** the app keeps its profiles in `%APPDATA%\ProjectOASISRust\profiles.json` (folder name from the earlier note; `oasis paths` shows the real one), never the C# app's `profiles.json`. The first run there starts with one profile for everything. (Secrets, the paired pad and logs are still shared with the C# app.)

**Trying it safely:** set `OASIS_DATA_DIR` to a folder holding a copy of `profiles.json`,
`profiles.sig`, `trusted_device.json` and `secrets\profile_hmac.key` before starting the app; it
then reads and writes only that copy.

## Interface

Child contracts: [src/](./src/README.md) (Rust, full command list), [web/](./web/README.md) (Svelte page), [capabilities/](./capabilities/README.md), [examples/](./examples/README.md).

| File | Contents |
|---|---|
| `src/main.rs` | Starts the engine, tray menu, hide-on-close, and most of the commands the page calls (full list in [src/README.md](./src/README.md)) |
| `src/overlay.rs` | The overlay window: creation, where it goes, move mode, with tests |
| `ui/overlay.html` | The overlay page |
| `src/settings_view.rs` | The plain view of the profiles the page receives, with tests |
| `src/system.rs`, `update.rs`, `models.rs`, `restart.rs`, `fixes.rs` | Startup and switches, updater, voice models, deferred restart, learn-from-my-fixes |
| `web/` (builds into `ui/app.html`) | The window: the new shell, and the old screens in `web/src/legacy/` until each is rebuilt (light and dark themes) |
| `tauri.conf.json`, `tauri.beta.conf.json` | Window size, icon, security policy, updater key and endpoint; beta adds the NSIS installer bundle |
| `capabilities/default.json`, `overlay.json` | What each window is allowed to do |
| `build.rs` | Runs `tauri_build` |
| `installer-hooks.nsh` | NSIS installer hooks |
| `icons/` | The OASIS icon, made from the C# app's PNG |

## Invariants
- Starts in preview; presses run nothing until "Run actions" is on, and a failed signature check keeps it off.
- Closing the window hides it; only tray Quit stops the engine.
- Built `ui/` files are committed and must be rebuilt (`web/`) whenever `web/` changes.
- Only one copy runs (single-instance lock); `OASIS_DATA_DIR` makes a rehearsal on a copy of the data.

## Side Effects
- Engine threads, tray icon, two windows, a keyboard hook while keyboard input is on, PowerShell helpers; writes profiles (signed), logs and settings; network for updates, voice models and connected accounts.

## Verification
- Run: `cargo test -p oasis-desktop` (and `npm test` in `web/` for the page).

**Checked so far (owner's screenshot, 2026-11-07):** the window opens in the dark theme, connects to
the pad, shows its battery, and lists each button press once with its profile and action, marked
"not run" in preview mode. **Also confirmed by the owner:** the tray icon appears, closing the window
hides it, the tray brings it back, and turning "Run actions" on makes presses run real actions.
**Not reported on:** the tray's Quit item.

**Keep the overlay alive (design rule carried over from C#):** it is the user's sign that OASIS is
running. The overlay page has no state of its own and only reacts to events, so it can be reloaded
at any time. It shares the web runtime with the settings window (D18), so a runtime-wide crash is
not yet isolated; a watchdog that recreates the window is still to do.

**Starting up** (`src/system.rs`; rules in `oasis_core::startup`, Windows calls in
`oasis_win::system` and `oasis_platform::windows::startup`):
1. Read the launch arguments (`--tray`, `--elevated-start`, `--restart`).
2. If administrator mode is on, this copy is not elevated, and the scheduled task starts exactly
   this program, ask Task Scheduler to start the elevated copy and exit. Never during a rehearsal
   (`OASIS_DATA_DIR`), and never when the task belongs to another build.
3. Take the single-copy lock (`Local\ProjectOASIS.SingleInstance`, open to the signed-in user so
   an elevated copy and a normal one can see each other). If it is taken: ask the running copy to
   show its window and exit. A `--restart` or task start waits up to 15 s for the old copy.
4. Show the window unless started with `--tray` (a note left by the hand-over can ask for it).
5. The Status screen's **System** card has the two switches and a **Restart OASIS** button.
   Rehearsals use a private lock name, so they can run beside the real app.

**Push-to-talk and the overlay's badge:** the overlay page shows a **LISTENING** badge (pulsing red
dot) while you talk and **THINKING** while the speech model works, from the engine's `voice`
events. The Status screen's **Push-to-talk** card shows Ready / Off / No speech model; the activity
list shows what was heard and whether it was typed. Speech engine files: see
`scripts\copy-speech-engine.ps1`.

**The Settings tab** (`web/src/legacy/`, commands `settings_fields`, `set_setting`,
`reset_overlay_look_setting`): the page does not know any setting by name. It asks
`oasis_settings::fields::sections` for the sections and fields (each with a kind: switch, number,
choice or colour, its limits and its current value) and draws them; a change goes back through
`set_setting`, which checks it against the same table and saves it safely. Adding a setting means
adding one entry in `crates/oasis-settings/src/fields.rs`. A wide number (a port) is a box; the rest
are sliders. The Profiles screen also has each profile's label behaviour and emoji (`save_profile_look`).
