# Dictation quality fixtures (`docs/VOICE_DECISIONS.md` V38)

One long sample the owner dictated on 2026-11-12, as different tools wrote it.

| File | What it is |
|---|---|
| `wispr.txt` | Wispr Flow's text: the target, one line per line |
| `take_new.txt` | OASIS, the build of 2026-11-12, rules only (the S1-mini polish did not run on this take) |
| `take_s1.txt` | OASIS, an earlier take, rules and S1-mini |

The speech model's raw text was never saved, and each take is a separate recording. So the two
OASIS texts are used as the input: `tests/quality.rs` runs the rules over each and counts the
target lines reached. A text the rules already cleaned is a fair input, because the rules must
leave their own output alone or improve it.

`cargo test -p oasis-core --test quality -- --nocapture` prints the score.
