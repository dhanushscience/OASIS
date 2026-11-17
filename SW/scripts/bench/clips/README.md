# clips

## Overview

Generates the speech bench corpus with local Piper voices.

## Interface

- `corpus.tsv`: tab-separated `<id>` and `<text>` (ids `c01`-`c10` short commands and sentences, `p01`-`p02` paragraphs).
- `make.sh`: for each row and each voice (`en_US-ryan-medium`, `en_GB-alan-medium`) writes `<id>-<en_US|en_GB>.wav` and the reference `.txt` beside it, using `piper.exe` from `$LOCALAPPDATA/ProjectOASIS/Models/tts`.

## Invariants

- Existing non-empty `.wav` files are skipped, so reruns are cheap; delete a clip to regenerate it.
- Failures print `FAILED <id> <voice>`; the final line is the wav count.

## Side Effects

- Writes `.wav` and `.txt` files into this directory (bench data, not source).

## Verification

- Run: `bash scripts/bench/clips/make.sh`
