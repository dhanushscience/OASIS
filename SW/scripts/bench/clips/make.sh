#!/usr/bin/env bash
# Synthesises the bench clips with the installed Piper voices: <id>-<voice>.wav and the reference <id>-<voice>.txt.
set -u
T="$LOCALAPPDATA/ProjectOASIS/Models/tts"; C="$(dirname "$0")"
while IFS=$'\t' read -r id text; do
  for v in en_US-ryan-medium en_GB-alan-medium; do
    out="$C/$id-${v%%-*}.wav"
    [ -s "$out" ] && continue
    printf '%s' "$text" | "$T/piper.exe" -m "$T/$v.onnx" -f "$out" --espeak_data "$T/espeak-ng-data" >/dev/null 2>&1 || echo "FAILED $id $v"
    printf '%s' "$text" > "${out%.wav}.txt"
  done
done < "$C/corpus.tsv"
ls "$C"/*.wav | wc -l
