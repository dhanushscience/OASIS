# `oasis-core/src/matching/` — "is this X?" helpers

## Overview
**What:** single definitions of the questions profile switching and voice keep asking: is this
process a browser? is this host YouTube? how close are these two names?

**Why:** in the C# app each of these was copied in 3–5 places, and the copies drifted (one list
of browsers knew Arc, another didn't; one YouTube check knew "yt music", another didn't). Each
drift was a real bug ("my Arc website profile never activates"). One copy, tested, fixes that.

**When:** on every foreground-app change (choosing the profile), when parsing spoken app names,
and when the extension reports a URL.

**How:** plain functions over `Option<&str>` (so "null" inputs from the OS are handled the same
way the C# `string?` was). Case-insensitive comparisons follow .NET `OrdinalIgnoreCase` for
ASCII.

## Interface
Exports: `BrowserProcesses`, `ForegroundContext`, `levenshtein`, `levenshtein_bounded`, `similarity`, `YouTubeHosts`. Subfolder: [`panel/`](./panel/README.md).

| File | Contents |
|---|---|
| `mod.rs` | Public exports |
| `browser_processes.rs` | Known browsers; exact vs. substring match (Arc only exact). Also strips macOS `.app` |
| `youtube_hosts.rs` | YouTube hosts, profile matches, spoken names ("yt", "yt music") |
| `edit_distance.rs` | Levenshtein distance and similarity for fuzzy app names |
| `text.rs` | ASCII case-insensitive `starts_with` / `ends_with` / `contains`, `is_blank` |
| `address_bar.rs` | `is_address_bar_name`, `pick_address`: which edit field is the address bar |
| `context.rs` | `ForegroundContext` (app, title, url) as the profile matcher sees it |
| `profile_targets.rs` | `APP_KIND`, `WEBSITE_KIND`, `is_website`, `infer_kind`, `website_match` |
| `url_host.rs` | `try_get_url_host`, `absolute_host`, `url_path`, `url_host_or_path_contains`, `site_label` |
| `web_catalog.rs` | `compact`, `try_resolve`, `guess_url`: spoken site names to addresses |

## Invariants
- One definition per question; Arc matches browsers by exact name only (`browser_processes.rs`).
- Inputs are `Option<&str>`; ASCII case-insensitive like .NET `OrdinalIgnoreCase`.
- Web-catalog lookup ignores case, spaces and punctuation (`web_catalog.rs:compact`).

## Side Effects
- None.

## Verification
- Run: `cargo test -p oasis-core`
