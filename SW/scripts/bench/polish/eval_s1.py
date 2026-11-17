"""Score S1-mini on a cases file (| n | raw | expected | tables), three ways, and print the result.

    cd SW
    cargo run -q -p oasis-core --example eval_cleanup -- <cases.md> target/m.txt target/rules.jsonl
    py -3 scripts/bench/polish/eval_s1.py target/rules.jsonl [results.json]

  rules    the app's rules alone (no model)
  S1 raw   S1-mini on the raw words, the whole text in one request
  S1 sent  S1-mini on the raw words, one request per sentence
  rules+S1 the app's real path: rules first, then S1-mini per sentence (a sentence that grows by more
           than 2 words is dropped, as in the app; the app's meaning check is not repeated here)

"exact" = same text; "close" = same words, ignoring case, punctuation and spacing.
Starts its own llama-server on port 18731 and stops it at the end. Standard library only.
"""
import json, os, re, statistics, subprocess, sys, time, urllib.request

PORT = 18731
BASE = f"http://127.0.0.1:{PORT}"
MODELS = os.path.join(os.environ["LOCALAPPDATA"], "ProjectOASIS", "Models")
RUNTIME = os.path.join(MODELS, "runtime", "llama-vulkan")
SYSTEM = ("You are a text normalizer for speech-to-text transcripts. The input begins with a control line "
          "specifying the styling, structure, and context settings; clean the transcript to match those settings "
          "and output only the cleaned text.")
CONTROL = "[Styling: semi-formal] [Structure: prose] [Context: general]"
SECTIONS = {1: "corrections", 2: "corrections 2", 3: "fillers", 4: "repeats", 5: "numbers", 6: "formatting", 7: "lists", 8: "mixed"}


def loose(t):
    return " ".join(w for w in re.split(r"[^\w']+", t.lower()) if w)


def sentences(text):
    parts = re.findall(r".+?(?:[.?!](?=\s|$)|$)", text.strip(), flags=re.S)
    return [p.strip() for p in parts if p.strip()]


calls = []  # (ms, out_tokens, out_tokens_per_s) for every request


def ask(text):
    body = {"messages": [{"role": "system", "content": SYSTEM}, {"role": "user", "content": f"{CONTROL}\n{text}"}],
            "temperature": 0, "max_tokens": 500, "chat_template_kwargs": {"enable_thinking": False}}
    req = urllib.request.Request(BASE + "/v1/chat/completions", json.dumps(body).encode(), {"Content-Type": "application/json"})
    t = time.perf_counter()
    reply = json.load(urllib.request.urlopen(req, timeout=120))
    ms = (time.perf_counter() - t) * 1000
    tm = reply.get("timings", {})
    calls.append((ms, tm.get("predicted_n", 0), tm.get("predicted_per_second", 0)))
    return reply["choices"][0]["message"]["content"].strip(), ms


def per_sentence(text):
    out, total = [], 0.0
    for s in sentences(text):
        if len(s.split()) < 5:  # the app skips short sentences
            out.append(s)
            continue
        got, ms = ask(s)
        total += ms
        out.append(got if len(got.split()) <= len(s.split()) + 2 else s)
    return " ".join(out), total


rows = [json.loads(l) for l in open(sys.argv[1], encoding="utf-8")]
server = subprocess.Popen(
    [os.path.join(RUNTIME, "llama-server.exe"), "-m", os.path.join(MODELS, "llm", "s1-mini", "s1-mini-q4_k_m.gguf"),
     "--host", "127.0.0.1", "--port", str(PORT), "-c", "4096", "-ngl", "99", "--jinja", "-np", "1", "--no-webui"],
    cwd=RUNTIME, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
try:
    while True:
        try:
            if urllib.request.urlopen(BASE + "/health", timeout=2).status == 200:
                break
        except Exception:
            time.sleep(0.4)
    ask("warm up the model please now")
    calls.clear()

    modes = ["rules", "S1 raw", "S1 sent", "rules+S1"]
    results = {m: [] for m in modes}  # per case: (section, got, ms)
    for i, r in enumerate(rows, 1):
        results["rules"].append((r["section"], r["rules"], 0.0))
        got, ms = ask(r["raw"])
        results["S1 raw"].append((r["section"], got, ms))
        got, ms = per_sentence(r["raw"])
        results["S1 sent"].append((r["section"], got, ms))
        got, ms = per_sentence(r["rules"])
        results["rules+S1"].append((r["section"], got, ms))
        if i % 50 == 0:
            print(f"  {i}/{len(rows)} cases", file=sys.stderr)
finally:
    server.kill()

n = len(rows)
print(f"\n{n} cases from {sys.argv[1]}\n")
print(f"{'':10} {'exact':>12} {'close':>12} {'unchanged':>10} {'lost words':>11} {'added words':>12}")
summary = {}
for m in modes:
    res = results[m]
    exact = sum(g == r["expected"] for (_, g, _), r in zip(res, rows))
    close = sum(loose(g) == loose(r["expected"]) for (_, g, _), r in zip(res, rows))
    same = sum(g == r["raw"] for (_, g, _), r in zip(res, rows))
    lost = sum(len(g.split()) < 0.6 * len(r["raw"].split()) and len(r["raw"].split()) > 6 for (_, g, _), r in zip(res, rows))
    grew = sum(len(g.split()) > len(r["raw"].split()) + 2 for (_, g, _), r in zip(res, rows))
    summary[m] = dict(exact=exact, close=close, unchanged=same, lost=lost, grew=grew)
    print(f"{m:10} {exact:>5} {100 * exact / n:5.1f}% {close:>5} {100 * close / n:5.1f}% {same:>10} {lost:>11} {grew:>12}")

print("\nclose-match by section (rules / S1 raw / S1 sent / rules+S1):")
for sec in sorted({r["section"] for r in rows}):
    idx = [i for i, r in enumerate(rows) if r["section"] == sec]
    cells = []
    for m in modes:
        ok = sum(loose(results[m][i][1]) == loose(rows[i]["expected"]) for i in idx)
        cells.append(f"{ok:>2}/{len(idx)}")
    print(f"  {sec} {SECTIONS.get(sec, ''):14} " + "  ".join(cells))

ms = sorted(c[0] for c in calls)
tps = [c[2] for c in calls if c[2]]
pct = lambda p: ms[min(len(ms) - 1, int(p * len(ms)))]
print(f"\n{len(calls)} requests: latency mean {statistics.mean(ms):.0f} ms, median {pct(0.5):.0f} ms, "
      f"P95 {pct(0.95):.0f} ms, max {ms[-1]:.0f} ms; output speed {statistics.mean(tps):.0f} tokens/s")
for m in ("S1 raw", "rules+S1"):
    per_case = sorted(r[2] for r in results[m])
    print(f"  {m}: wait per case median {per_case[len(per_case) // 2]:.0f} ms, P95 {per_case[int(.95 * len(per_case))]:.0f} ms "
          f"(all sentences, one after another; the app polishes earlier ones while you talk)")
if len(sys.argv) > 2:
    json.dump({"summary": summary, "results": {m: [g for _, g, _ in results[m]] for m in modes}}, open(sys.argv[2], "w", encoding="utf-8"), ensure_ascii=False, indent=1)
