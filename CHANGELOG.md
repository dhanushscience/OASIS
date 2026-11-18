# Changelog

All notable changes to OASIS. The format follows [Keep a Changelog](https://keepachangelog.com). Full notes: [beta/RELEASE-NOTES.md](beta/RELEASE-NOTES.md).

## [0.1.8]

### Changed
- The project is now at github.com/dhanushscience/PROJECT-OASIS, and the website at dhanushscience.github.io/PROJECT-OASIS. Updates and models come from there. Nothing to do on your side.

## [0.1.7]

### Changed
- Updates and voice models now come from one place, github.com/dhanushscience/OASIS. Nothing to do on your side: your models stay where they are.

## [0.1.6]

### Changed
- Voice commands understand spoken numbers; a failed command is never typed.
- Administrator mode is only turned on when OASIS is installed under Program Files, so other programs can't borrow its administrator rights.

## [0.1.5]

### Added
- Welcome screen with two short scenes: dictation that fixes a slip on its own, and a voice command that opens an app.
- Say "open a new window in (app)" to get a new window; plain "open" still reuses the last one.

### Changed
- Calendar, Spotify and the browser extension are greyed out as "Coming soon".
- The update prompt asks before upgrading.

## [0.1.3]

### Added
- OASIS's own fine-tuned S1-mini text-polish model, with about a quarter fewer mistakes than the stock one (downloaded once, about 460 MB).
- Guide for removing an old version and keeping your profiles.

### Changed
- PCs that suit QUALITY download it straight away instead of starting on the small set.

## [0.1.2]

### Added
- First-run Getting Started guide with Mr. Science (browser, microphone, first press, first voice command).
- Voice editing commands ("select all", "copy", "undo", "press enter" and more) and a Try voice commands practice box.
- Hands-free dictation by double-clicking the talk button; recordings up to 30 minutes.
- Layout choice (horizontal F1 F2 F3, or vertical keys), start and paste sounds, an update prompt once a day.

### Changed
- The voice pop-up and start sound appear right at the button press, with less waiting after you stop talking.
- Better dictation cleanup: capitals, filler words, corrections, question marks.
- Speech models renamed Basic and Balanced, with a one-click **Better accuracy** upgrade.
- The polish helper frees its memory after 10 idle minutes.
- The pad's buttons run their actions on the OASIS window itself.

### Removed
- Spoken replies. OASIS no longer talks back.

### Fixed
- A memory leak where leftover polish helpers held about 1.2 GB each and slowed the PC.
