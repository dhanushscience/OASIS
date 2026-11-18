# `apps/oasis-desktop/` — the desktop app

## Overview

**What:** the program users run. A tray icon, the main window (Profiles, Quick Launch, Voice and
Insights sections, with Settings as a pop-up), the always-on-top **overlay** (pad bars and button
labels), the **HUD** (listening pill, close question, "save this word?"), the first-run **welcome**
window with its caption panel (Getting Started), and the engine behind them. It replaces
`OasisShell.csproj` (`App.xaml`, `MainWindow`, `Controls/CenterTray*`, `Windows/*`,
`Services/OverlayService.cs`, `TrayService.cs`).

**Why Tauri 2:** Tauri gives a native window and tray with a web page inside, the fastest way to
rebuild the large settings window (DECISIONS D7). The page is Svelte + Vite, source in `web/`;
`npm install` and `npm run build` there write the finished pages into `ui/`, and **those built
files are committed**, so `cargo build -p oasis-desktop` needs no Node. Rebuild the page whenever
`web/` changes and commit `ui/` with it. The installer is made with `scripts/package-beta.ps1`
(NSIS, from `tauri.beta.conf.json`).

**When it runs:** at sign-in if "Start with Windows" is on (it then starts hidden in the tray with
`--tray`), or when you start it. One copy only.

**How it works:**
1. `src/main.rs` starts `oasis-engine` on its own threads and forwards every engine event to the
   windows (`engine-event`; the microphone level goes out as `voice-level` so the other windows
   are not woken 20 times a second).
2. The main window (`web/`, built into `ui/app.html`) shows the pad's status and battery in the
   side card, the profiles and their buttons, Quick Launch setups, voice settings and the kept
   dictations.
3. **It starts live:** presses run their actions. If `profiles.json` fails its signature check, a
   banner says so, nothing is saved and no press runs. (The engine still has a preview mode,
   `EngineHandle::set_live`, but the app does not use it.)
4. Closing the main window hides it; the app keeps running in the tray. **Quit** in the tray menu
   stops the engine and leaves. Closing the welcome window ends it and brings the main window
   forward.
5. If the engine cannot start (no `profiles.json`, no Bluetooth adapter), a banner says why.
6. Reminders and meeting reminders bring a window forward. A typed dictation starts the "learn
   from my fixes" watch (`fixes.rs`).

**The Profiles section** (`web/src/components/ProfilesScreen.svelte` and its parts; commands in
`src/profiles.rs`):
- Picks a profile; edits its name, what it applies to (`*` = everything) and whether that is an app
  or a website; shows each set of buttons and lets you edit the three buttons.
- Every save goes through `engine.change(...)`, which checks the value with the same rules the
  engine runs actions by, saves safely (signed, last-good copy), and puts the change in force: the
  pad uses it from the next press.
- A refused save says why, in plain words, and changes nothing: an unparseable shortcut, a link the
  allowlist blocks, a malformed mouse position, a profile with no name, **or the file having been
  changed by another program since OASIS read it** (use *Reload from file* in Settings, then save
  again).
- **New profile** (the + in the profile strip) asks what it applies to (an app name, a program
  path, or a website; a bare site name like "netflix" is looked up and becomes `netflix.com`;
  empty means "everything else"), whether it is an app or a website, and an optional name. It
  starts with three blank buttons.
- Profiles can be moved (`move_profile`). The first profile that matches wins, so order matters.
- **Delete profile** asks first. The last profile cannot be deleted. Deleting the only "everything
  else" profile adds a warning. The profile's icon file is removed too, unless another profile
  uses it.
- **Several steps:** a button can hold a list of steps (Quick Launch, LaunchApp, OpenUrl,
  Shortcut, AutoHotkey, MouseClick, JoinMeeting, and others the editor knows). Saving needs at
  least one step and checks each (the reason names the step: "Step 2: Unsupported key 'Nope'").
  A step of a kind this screen cannot make (made by an AI plan) is shown locked and kept as is.
- **Try it** runs a saved button now (`try_button`), under the same rules as a press.
- The label is shown on the pad in capitals, up to 14 characters.
- **Sets of buttons:** a profile's "extra sets" switch turns on a list of sets, each with a name
  and a condition (window title text, a panel, or a YouTube state). The main set has no condition
  and cannot be deleted; switching extra sets off keeps them.
- A profile has an icon (an app or site icon, or a picture) and a label behaviour and emoji
  (`save_profile_look`).

**Quick Launch** (`web/src/components/QuickLaunchScreen.svelte`, `src/quick_launch.rs`): saved
setups that open several apps, URLs or folders, each on a chosen screen and part of it. **Capture**
reads the open windows (`capture_windows`); `run_quick_setup` opens one. Voice can launch a setup
by name.

**The overlay** (`ui/overlay.html`, `src/overlay.rs`, `src/overlay_window.rs`, DECISIONS D18):
- A second window, transparent, always on top, never focused, clicks pass through. Its size
  depends on where it docks: 380x260 stacked down a left or right edge, 680x190 in a row at the
  bottom or top (`overlay::size_for`). It sits where `OverlayX/OverlayY` say, or at the dock
  edge of the main screen; a saved spot that no screen shows any more falls back to that default.
  With more than one screen it follows the screen the mouse is on, and a thread
  (`follow_the_screen_in_use`) keeps it in front of other windows.
- It listens to the same `engine-event` stream: `pad_button` thickens a bar while the pad button is
  held; `show_labels` slides in the three labels; `hide_labels` (time up, or an action ran) removes
  them; `voice` and `confirm` wake the bars while you talk or a question is up; `pad_lost` clears
  the bars; `settings_changed` re-reads its look. Other events: `pick-prompt` (the countdown
  while OASIS waits for a click on a program), `overlay-moving`, `overlay-jump`.
- Its look (`overlay_look` command) comes from `OverlayAppearance` in `profiles.json`, checked by
  `oasis-settings::overlay`: bad colours or sizes fall back to the C# defaults.
- **Move overlay** (tray menu, or the button in Settings > Overlay; both call
  `toggle_overlay_move`) makes it a solid, draggable box; **Done moving** saves the spot with the
  normal safe save (`save_overlay_position`).
- The first tap on a button with an action only shows the labels (when label showing is on for the
  profile); a second tap within 4 seconds runs it. Decided in the engine, not here.
- Capability file `capabilities/overlay.json` gives this window only event listening and dragging.

**The HUD** (`ui/hud.html`, `src/hud.rs`): one small transparent window, never focused, on the screen
under the mouse. It shows the **listening pill** (phases `getting_ready:*`, `listening`,
`dictating`, `thinking`, `locked`; a tick and a cross while the recording is locked, which press
button 1 or 2 through `press_button`), the **"Close this app?" question** (engine `confirm` /
`confirm_end` events) and the **"save this word?" offer** (`learn-offer`, answered with
`learn_answer`). The pill sits at the bottom middle, or stands up at a side when the overlay is
docked to a side (`hud-side`). It is hidden when nothing is up.

**Getting Started** (`src/onboarding.rs`, `web/src/guide/`, `web/src/components/Welcome.svelte`,
`Guide.svelte`; steps in `oasis_core::onboarding`): the first-run guide with Mr. Science. Progress
is saved as `onboarding.json` beside `profiles.json`. A first run (the app just made its
`profiles.json`) opens the `welcome` window; after its steps the main window carries on with a
guide card. A caption panel (`guide` window, `ui/guide.html`, 520x136, over other programs) shows
what to do; the guide borrows button 2 for a demo website and hands it back empty. Every event goes
through one worker thread (`onboarding::send`). Test switches: `OASIS_GUIDE_TEST=try_press,main_focused`
plays events, `OASIS_TEST_BELOW_FLOOR=1` pretends the PC is below the voice floor.

**Ways of pressing the buttons (Settings > Pad & keys):** the pad is one of them. The other is the
keyboard: three shortcuts, one per button, listened for system-wide by a low-level keyboard hook
(`oasis_win::keyboard`) that exists only while that switch is on, reports only the chosen shortcuts, and is
logged to `security.log` when switched on or off. Keys OASIS sends itself are stamped and ignored, so it
cannot trigger itself. Chosen shortcuts are kept from the program in front unless "keep these keys" is off.
Both are judged by the same gesture rules as the pad, so tap, hold and double-tap work the same. At least one
way must stay on. Planned for version 2: phone as a deck, trackpad regions, mouse ring menu.

**Its own profiles:** the app keeps its profiles in `%APPDATA%\ProjectOASIS\Rust\profiles.json`
(`oasis paths` shows the real one), never the C# app's `profiles.json`. The first run there starts
with one profile for everything, the keyboard keys on, button 1 named Voice and button 3 set to
`Win+D` (Desktop). (Secrets, the paired pad and logs are still shared with the C# app.)

**Trying it safely:** set `OASIS_DATA_DIR` to a folder holding a copy of `profiles.json`,
`profiles.sig`, `trusted_device.json` and `secrets\profile_hmac.key` before starting the app; it
then reads and writes only that copy.

## Interface

Child contracts: [src/](./src/README.md) (Rust, full command list), [web/](./web/README.md) (Svelte page), [ui/](./UI_PAGES.md) (built and hand-written pages; the notes sit outside `ui/` so the installer does not pack them), [capabilities/](./capabilities/README.md), [examples/](./examples/README.md).

| File | Contents |
|---|---|
| `src/main.rs` | Starts the engine, tray menu, hide-on-close, the `status` command and the command registration (the other commands live in one file per screen; see [src/README.md](./src/README.md)) |
| `src/profiles.rs`, `icons.rs`, `apps.rs`, `quick_launch.rs`, `voice.rs`, `accounts.rs`, `settings_screen.rs`, `overlay_window.rs` | Tauri commands, one file per screen |
| `src/overlay.rs`, `hud.rs` | The overlay and HUD windows: creation, placement, modes, with tests |
| `src/onboarding.rs` | Getting Started: progress, the welcome window, the caption panel, the demo button |
| `src/settings_view.rs` | The plain view of the profiles the page receives, with tests |
| `src/system.rs`, `update.rs`, `models.rs`, `restart.rs`, `fixes.rs`, `fixes_core.rs` | Startup and switches, updater, voice models, deferred restart, learn-from-my-fixes |
| `src/site_icon.ps1` | PowerShell that finds a website's logo |
| `web/` (builds into `ui/app.html`, `ui/welcome.html`) | The main window and the welcome window (Svelte, light and dark themes) |
| `ui/` | Built pages (committed) and the hand-written `overlay.html`, `hud.html`, `guide.html` |
| `tauri.conf.json`, `tauri.beta.conf.json` | Window size, icon, security policy, updater key and endpoint; beta adds the NSIS installer bundle and the packed native files and browser extension |
| `capabilities/` | What each window is allowed to do (`default`, `overlay`, `welcome`, `guide`, `hud`) |
| `examples/fixes_probe.rs` | Manual probe of the learn-from-my-fixes loop |
| `build.rs` | Runs `tauri_build` |
| `installer-hooks.nsh` | NSIS installer hooks |
| `icons/` | The OASIS icon (`icon.png`, `icon.ico`), made from the C# app's PNG |

Windows: `main` (the window), `overlay`, `hud`, `welcome` (first run) and `guide` (caption panel).

Threads: the engine's own threads (`oasis-engine`); `oasis-models` (voice model setup, `models.rs`);
the update checker (`update.rs`, at start and every 12 h); `oasis-fixes` (one per typed dictation,
`fixes.rs`); the onboarding worker (`onboarding.rs`); the overlay's screen follower
(`overlay_window.rs`, every 100 ms); the single-copy watcher (`system.rs`). Slow commands run on
Tauri's blocking pool.

## Invariants
- Starts live, not in preview; a failed signature check turns actions off and blocks saving, and a banner says so.
- Closing the main window hides it; only tray Quit stops the engine. Closing the welcome window ends it and shows the main window.
- Built `ui/` files are committed and must be rebuilt (`web/`) whenever `web/` changes. `ui/overlay.html`, `hud.html`, `guide.html` are hand-written and survive a rebuild.
- Only one copy runs (single-instance lock); `OASIS_DATA_DIR` makes a rehearsal on a copy of the data.
- The overlay page has no state of its own and only reacts to events, so it can be reloaded at any time. It is the user's sign that OASIS is running. It shares the web runtime with the other windows, so a runtime-wide crash is not isolated; a watchdog that recreates the window is still to do.

## Side Effects
- Engine threads, tray icon, five windows, a keyboard hook while keyboard input is on, PowerShell helpers; writes profiles (signed), `onboarding.json`, logs and settings; network for updates, voice models and connected accounts (Spotify, meetings, Groq key test).

## Verification
- Run: `cargo test -p oasis-desktop` (and `npm test` in `web/` for the page).

**Starting up** (`src/system.rs`; rules in `oasis_core::startup`, Windows calls in
`oasis_win::system` and `oasis_platform::windows::startup`):
1. Read the launch arguments (`--tray`, `--elevated-start`, `--restart`).
2. If administrator mode is on, this copy is not elevated, and the scheduled task starts exactly
   this program, ask Task Scheduler to start the elevated copy and exit. Never during a rehearsal
   (`OASIS_DATA_DIR`), and never when the task belongs to another build.
3. Take the single-copy lock (`Local\ProjectOASIS.SingleInstance`, open to the signed-in user so
   an elevated copy and a normal one can see each other). If it is taken: ask the running copy to
   show its window and exit. A `--restart` or task start waits up to 15 s for the old copy.
4. Show the window unless started with `--tray` (a note left by the hand-over can ask for it). The
   first time OASIS is asked for on a first run, this opens the welcome window.
5. Settings has the two switches (start with Windows, run as administrator) and a **Restart OASIS**
   button (`system_status`, `set_start_with_windows`, `set_run_as_administrator`, `restart_oasis`).
   Rehearsals use a private lock name, so they can run beside the real app.

**Push-to-talk:** the HUD's pill shows the voice phases (see The HUD); the activity list shows what
was heard and whether it was typed. Speech engine files: see `scripts\copy-speech-engine.ps1`
(at `SW/scripts/`).

**Updates and voice models:** `update.rs` checks GitHub Releases and installs only when the person
presses Install; `models.rs` checks the PC, picks a model set, downloads in the background and
points voice at it (`models-status` event, top strip, Settings > Voice models). A restart that
must wait for a quiet moment is asked for through `restart.rs` (`restart-needed` event).

**The Settings pop-up** (`web/src/components/SettingsDialog.svelte`, `web/src/legacy/`, commands
`settings_fields`, `set_setting`, `reset_overlay_look_setting`): the page does not know any setting
by name. It asks `oasis_settings::fields::sections` for the sections and fields (each with a kind:
switch, number, choice or colour, its limits and its current value) and draws them; a change goes
back through `set_setting`, which checks it against the same table and saves it safely. Adding a
setting means adding one entry in `crates/oasis-settings/src/fields/`. Sections: Pad & keys,
Voice, Voice models, Overlay, Gestures, Connections (shown as "Coming soon" and switched off in this beta: Spotify, meetings calendar, browser
extension), Appearance, Preferences, General, Updates & data, Activity.
