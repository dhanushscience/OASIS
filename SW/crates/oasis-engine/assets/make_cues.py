"""Makes voice-in.wav and voice-out.wav (the talk cues) from the owner's IN.wav and OUT.wav.

Run from the repo root:
  python SW/crates/oasis-engine/assets/make_cues.py SW/crates/oasis-engine/assets/cue-sources/IN.wav SW/crates/oasis-engine/assets/cue-sources/OUT.wav

Each cue: silence trimmed off both ends (a few ms kept), 6 ms fades so the cut never clicks, peak
lowered to about -20 dBFS so it is soft even at full volume, and a lead-in of very quiet noise.
A Bluetooth speaker that has gone idle swallows the first few hundred ms of any sound; the noise
(about -76 dBFS, far below hearing) wakes the link so the cue itself is heard.
The start cue must sound the moment the hold is recognised, so its lead-in goes to voice-wake.wav
instead, which the engine plays when the talk button goes down (2026-11-16).
"""
import array
import random
import sys
import wave

TARGET_PEAK = 3300  # of 32767: about -20 dBFS
LEAD_MS = 400
LEAD_NOISE = 5  # of 32767: about -76 dBFS
THRESHOLD = 300
PAD_MS = 4
FADE_MS = 6


def make(src, dst, wake=None):
    w = wave.open(src)
    p = w.getparams()
    samples = array.array('h')
    samples.frombytes(w.readframes(p.nframes))
    w.close()
    ch, rate = p.nchannels, p.framerate
    n = len(samples) // ch
    loud = [i for i in range(n) if max(abs(samples[i * ch + c]) for c in range(ch)) > THRESHOLD]
    pad = rate * PAD_MS // 1000
    start, end = max(0, loud[0] - pad), min(n, loud[-1] + 1 + pad)
    out = samples[start * ch:end * ch]
    frames = end - start
    gain = TARGET_PEAK / (max(abs(x) for x in out) or 1)
    out = array.array('h', (int(x * gain) for x in out))
    fade = min(frames, rate * FADE_MS // 1000)
    for i in range(fade):
        g = i / fade
        for c in range(ch):
            out[i * ch + c] = int(out[i * ch + c] * g)
            out[(frames - 1 - i) * ch + c] = int(out[(frames - 1 - i) * ch + c] * g)
    rng = random.Random(7)  # the same file every run
    lead = array.array('h', (rng.randint(-LEAD_NOISE, LEAD_NOISE) for _ in range(rate * LEAD_MS // 1000 * ch)))
    if wake:
        w = wave.open(wake, 'wb')
        w.setparams(p)
        w.writeframes(lead.tobytes())
        w.close()
        lead = array.array('h')
    o = wave.open(dst, 'wb')
    o.setparams(p)
    o.writeframes((lead + out).tobytes())
    o.close()
    print(f'{src}: {n * 1000 // rate} ms -> {frames * 1000 // rate} ms of sound after {LEAD_MS} ms lead-in')


if __name__ == '__main__':
    here = __file__.rsplit('/', 1)[0].rsplit('\\', 1)[0]
    make(sys.argv[1], f'{here}/voice-in.wav', f'{here}/voice-wake.wav')
    make(sys.argv[2], f'{here}/voice-out.wav')
