# `oasis-core/src/bridge/` — browser-extension rules

## Overview
**What:** the decisions behind the local WebSocket the OASIS browser extension connects to:
who may connect, which messages are understood, and what YouTube is doing right now.

**Why:** the extension is how OASIS knows a video is playing, ended, or is a Short — which
picks the button set (`ConfigProfile.When`). Everything on that socket is **untrusted local
input**, so the admission rules must be explicit and tested.

**When:** on every connection attempt to `ws://127.0.0.1:17322/oasis` and every frame from it.
The socket server itself lives in `oasis-platform` (see below).

**How:**
- `BrowserBridgePolicy::is_allowed_origin`: extension origins (`chrome-extension://`, `moz-extension://`, `safari-web-extension://`), `http://127.0.0.1` and `http://localhost` pages, and a missing, empty or `null` origin; everything else is refused before the socket opens.
- `is_known_message_type`: `hello`, `ping`, `youtube.*`; anything else is dropped.
- `can_introduce_actions` is always `false`: the extension can change *which* buttons show,
  never *add* an action.
- The per-user token (checked by the server) stops web pages and other local users; it does
  **not** stop a process running as the same OS user. Defence in depth, not a boundary.
- `YouTubeWatchState::apply(event) -> changed` reduces events to Idle / Playing / Ended:
  >= 98 % progress or `youtube.ended` -> Ended; < 95 % progress or any start -> Playing; 95-97 % holds Ended; Shorts never end; a new video id leaves Ended immediately; `youtube.ready` on a watch page moves Idle or Ended to Playing.
- `current_when_token()` gives `youtube.playing`, `youtube.ended`, `youtube.shorts` or `""` (Idle). A Short always gives `youtube.shorts`.

## Interface
Exports in `mod.rs`: `BrowserBridgePolicy` (`is_allowed_origin`, `is_known_message_type`, `can_introduce_actions`), `ConfigWhen` (`PLAYING`, `ENDED`, `SHORTS`), `YouTubePlaybackPhase`, `YouTubeWatchEvent`, `YouTubeWatchState`.

| File | Contents |
|---|---|
| `mod.rs` | Module list and the exports above |
| `policy.rs` | Origin and message-type admission (C# GATE-1) |
| `youtube_watch.rs` | `YouTubeWatchState` (`apply`, `current_when_token`), `YouTubeWatchEvent`, `YouTubePlaybackPhase`, `ConfigWhen`; also `ended_notice` (title cut to 48 characters for the "video ended" notice, none for Shorts) and `near_end_peek` (true once per video at 98 %, to flash the labels), reached as `bridge::youtube_watch::...` |

**The socket itself** is `oasis-platform::browser_bridge` (plain threads and the synchronous
`tungstenite`, no async runtime): origin check before the socket opens, a `hello` with the per-user
token within 2 s, only `hello`/`ping`/`youtube.*` read, 60 YouTube frames a second at most, frames
up to 64 KiB. `oasis-platform::extension_install` copies the extension into the local data folder
and `write_token_files` puts the token beside it. `oasis-settings::youtube` holds the bridge
settings and the rule for when the hidden YouTube gestures are live. The engine owns the watch
state and feeds its token to profile matching.

## Invariants
- `can_introduce_actions` is always `false` (`policy.rs`).
- Only `hello`, `ping` and `youtube.*` messages are known; others are dropped.
- Watch phase: >= 98 % is Ended; < 95 % or any start is Playing; 95-97 % holds Ended; Shorts never end (`youtube_watch.rs`).
- `apply` ignores an empty event type, and an event type it does not know changes nothing.

## Side Effects
- None. `YouTubeWatchState::apply` mutates only itself; `near_end_peek` mutates only the `last_peeked` string the caller passes.

## Verification
- Run: `cargo test -p oasis-core`
