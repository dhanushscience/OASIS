# OASIS Bridge (Chrome / Edge / Brave)

Connects your browser to the OASIS desktop app so page activity can drive macros and automations.

Websocket: `ws://127.0.0.1:17322/oasis` (falls back to `ws://localhost:17322/oasis`)

## Overview

A Manifest V3 extension (`manifest.json`, version 1.3.1, name "OASIS Bridge"). A service worker holds the
websocket to OASIS; content scripts on YouTube hosts report playback state and obey OASIS commands; a
popup shows connection status and the hold-speed setting.

## Interface

| File | Role |
|------|------|
| `background.js` | Service worker. Websocket client with token handshake (`hello`), reconnect backoff (1.5 s to 15 s), queued sends, 30 s keepalive alarm (`oasis-bridge-keepalive`, sends `ping`). Handles runtime messages `oasis.status`, `oasis.reconnect`, `oasis.holdRate`, and republishes any `youtube.*` message to OASIS. Forwards commands to YouTube tabs. |
| `content.js` | Content script (isolated world). Tracks the `<video>` element, sends `youtube.*` events, receives `oasis.command`, `oasis.settings`, `oasis.holdRate`; pauses and resumes video while OASIS's microphone is open (only a video it paused itself). |
| `adskip.js` | Content script in the page's `MAIN` world. Clicks Skip when it unlocks, mild speed-up on unskippable ads, posts `oasis.adSkipped` to the page. Never seeks to the end of an ad. |
| `popup.html` / `popup.js` | Toolbar popup: CONNECTED / DISCONNECTED pill with reason, RETRY CONNECTION, hold-speed select. |
| `manifest.json` | Permissions `storage`, `alarms`, `scripting`, `tabs`; hosts are YouTube domains plus `127.0.0.1` / `localhost` (port 17322). |

## Invariants

- Hold playback speed is clamped to 1.25 to 4 (default 2) in both `popup.js` and `background.js`.
- The bridge token is read at runtime from `bridge-token.json` (or legacy `bridge-token.js`) in the extension folder, never imported, so a stale empty token is not cached.
- A video the viewer paused themselves is never resumed by OASIS.
- An ad speed-up is never remembered as the user's playback rate.

## Side Effects

- Opens a local websocket to OASIS; fetches the token file from the extension folder.
- Writes `holdPlaybackRate`, `oasisBridgeUrl`, `oasisLastForwarded` and `oasisLastForwardedAt` to `chrome.storage.local`.
- Clicks Skip buttons, pauses, resumes and changes playback rate on YouTube tabs.

## Install

1. Start OASIS (Browser Bridge **ENABLED**).
2. Settings → **OPEN EXT FOLDER**
3. Brave/Chrome → Extensions → Developer mode → **Load unpacked** → select that folder
4. Badge should show **ON**; status popup should say **CONNECTED**

## Notes

- Prefer the LocalAppData copy (OPEN EXT FOLDER), not OneDrive Debug output.
- Reload the extension after OASIS updates.
- Current page hooks include video playback signals; more site automations can be added later.
- `fonts/` and `icons/` are assets only and are not documented.

## Verification

- No automated tests. Load unpacked and check the popup reads **CONNECTED**.
