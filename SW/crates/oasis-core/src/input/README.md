# `oasis-core/src/input/` — gesture arbitration

## Overview
**What:** decides what each pad interaction *means*. Three buttons carry: tap, double-tap,
hold, chord, push-to-talk, hold-for-2× speed, AI hold, show-desktop, meeting-reminder and
confirmation answers. This folder picks exactly one meaning per gesture.

**Why:** with that many gestures on three buttons they collide (BTN 2 might be both "hold for
2×" and "hold to talk"). The C# app grew this logic inside `OverlayService` over months; it was
extracted into a pure reducer so it could be pinned by tests. This is the most
behaviour-sensitive code in OASIS.

**When:** on every `BTN:x:D` / `BTN:x:U` from the pad, on every timer the reducer itself asked
for, and on disconnect.

**How:**
1. The host passes the current `PadState`, the event, the user's `ArbiterConfig` (button
   mapping + timings), an `ArbiterContext` (is YouTube in front? is a confirmation pending?) and the claim list (`default_claims()`).
2. `reduce` walks the claims **in priority order** — meeting reminder (1000) > AI session
   (900) > confirmation (850) > voice PTT (80) > hold-2× (70) > double-click volume (60) >
   show desktop (55) > AI hold (50) > bound tap (10).
3. Undecided gestures are kept in `PadState::Pressed { arm }`, with a deadline per candidate.
   The reducer asks for a timer for each deadline; when the tick comes back, the first
   candidate whose deadline passed and still applies wins.
4. It returns effects: `DispatchCommand { button, claim }`, `StartVoiceCapture`, `EndVoiceCapture`,
   `CancelVoiceCapture`, `SendToDevice("playbackRate.hold2x")`, `SetTimer`, `CancelTimer`, `ShowPeek`.

Key rules, each pinned by a test in `../../tests/pad_reducer.rs`:
- Outside YouTube a tap runs **on press** (lowest latency); with AI or a desktop hold on that
  button it waits for release, because only release proves it was a tap.
- On YouTube the volume buttons wait 300 ms for a second tap; no 700 ms floor, so two seeks
  stay two seeks.
- Voice on a volume button waits at least 700 ms, so a quick double-tap never opens the mic.
- Double-clicking the talk button (when it is alone in the push-to-talk mask) locks the mic on (`PadState::VoiceLocked`); the next press of the talk button ends it, and button 2 throws the recording away.
- Button events other than pad buttons are ignored. A disconnect cancels every timer, ends a voice capture and undoes hold-2x.
- One `ArbiterContext` per gesture: if the foreground check flakes between the two taps of a
  double-click, it is still volume.

## Interface
Exports: `default_claims`, `GestureClaim`, `GestureClaimId`, `PriorityClaim`, `ArbiterConfig`, `Binding`, `KeyMatcher`, `KeyVerdict`, `LabelPolicy`, `reduce` and its timer names (`DOUBLE_CLICK_TIMER`, `HOLD2X_TIMER`, `VOICE_PTT_TIMER`, `AI_HOLD_TIMER`, `SHOW_DESKTOP_TIMER`) and commands (`HOLD2X_ENGAGE_COMMAND`, `HOLD2X_RESTORE_COMMAND`), `ArbiterContext`, `ArbiterEffect`, `PadEvent`, `PadState`, `PressArm`, `Transition`.

`reduce(state, event, config, context, claims) -> Transition`. `PadState` is `Idle`, `Pressed`, `AwaitingSecondTap`, `Claimed`, `Executing` or `VoiceLocked`. `PadEvent` is `Down`, `Up`, `Tick` or `Disconnected`.

| File | Contents |
|---|---|
| `mod.rs` | Module list and public exports (the submodules are private; only the exports are reachable) |
| `config.rs` | `ArbiterConfig`: mapping, timings, clamps (double-click 200-1000 ms, push-to-talk 200-2000 ms, desktop hold 400-3000 ms); `product_defaults()` (voice and AI off), `user_pad()` (the owner's pad) |
| `claims.rs` | `GestureClaimId`, the `GestureClaim` trait, `PriorityClaim`, `default_claims()` (the priority list) |
| `state.rs` | `PadState`, `PressArm`, `PadEvent`, `ArbiterContext` (also `execution_allowed`: false when the profile store failed its check, so bound actions do not run), `ArbiterEffect`, `Transition` (with test helpers such as `dispatched`, `sets_timer`) |
| `reducer/` | `reduce()` — press, release, tick, disconnect; line-for-line port of `PadReducer*.cs` → [README](reducer/README.md) |
| `label_policy.rs` | `LabelPolicy`: per-profile "first tap shows labels" override (`""` inherit, `show`, `hide`); `normalize`, `effective` |
| `keybinds.rs` | `Binding`, `KeyMatcher`, `KeyVerdict`: the three buttons from the keyboard; a button goes down when its key is pressed with exactly its modifiers held and up when the key or a modifier is released |

## Invariants
- Claims are checked in priority order: meeting reminder (1000), AI session (900), confirmation (850), voice PTT (80), hold-2x (70), double-click volume (60), show desktop (55), AI hold (50), bound tap (10) (`claims.rs`).
- Exactly one meaning per gesture; undecided gestures wait in `PadState::Pressed { arm }` with a deadline per candidate.
- One `ArbiterContext` per gesture.
- The reducer is pure: time arrives as events (`Tick`), timers are requested as effects.

## Side Effects
- None. Effects (`DispatchCommand`, `StartVoiceCapture`, `SetTimer`, `CancelTimer`, `ShowPeek`, `SendToDevice`) are returned for the host to run.

## Verification
- Run: `cargo test -p oasis-core`
