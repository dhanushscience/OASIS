"""Type a sentence, S1-mini cleans it, you see the answer and what it cost.

    py -3 scripts/bench/polish/try_s1.py            (from SW/; type text, empty line quits)
    echo some text | py -3 scripts/bench/polish/try_s1.py

Starts its own llama-server on port 18731 (the app's own keeps running; both hold the model, so
close the app if graphics memory is tight) and stops it on exit. Standard library only.
"""
import json, os, subprocess, sys, time, urllib.request

PORT = 18731
BASE = f"http://127.0.0.1:{PORT}"
MODELS = os.path.join(os.environ["LOCALAPPDATA"], "ProjectOASIS", "Models")
RUNTIME = os.path.join(MODELS, "runtime", "llama-vulkan")
SYSTEM = ("You are a text normalizer for speech-to-text transcripts. The input begins with a control line "
          "specifying the styling, structure, and context settings; clean the transcript to match those settings "
          "and output only the cleaned text.")
CONTROL = "[Styling: semi-formal] [Structure: prose] [Context: general]"


def run(cmd):
    return subprocess.run(cmd, capture_output=True, text=True, shell=False).stdout.strip()


def server_use(pid):
    """CPU seconds and memory (MB) of the server process, as PowerShell reports them."""
    out = run(["powershell", "-NoProfile", "-Command", f"$p=Get-Process -Id {pid}; '{{0:N2}} {{1:N0}}' -f $p.CPU,($p.WorkingSet64/1MB)"])
    cpu, mb = out.split()
    return float(cpu), int(mb.replace(",", ""))


def gpu():
    out = run(["nvidia-smi", "--query-gpu=utilization.gpu,memory.used,memory.total", "--format=csv,noheader,nounits"]) if shutil_which("nvidia-smi") else ""
    return out or "n/a (no nvidia-smi)"


def shutil_which(name):
    return any(os.path.exists(os.path.join(p, name + ".exe")) for p in os.environ["PATH"].split(os.pathsep))


def ask(text):
    body = {"messages": [{"role": "system", "content": SYSTEM}, {"role": "user", "content": f"{CONTROL}\n{text}"}],
            "temperature": 0, "max_tokens": 400, "chat_template_kwargs": {"enable_thinking": False}}
    req = urllib.request.Request(BASE + "/v1/chat/completions", json.dumps(body).encode(), {"Content-Type": "application/json"})
    t = time.perf_counter()
    reply = json.load(urllib.request.urlopen(req, timeout=120))
    return reply, (time.perf_counter() - t) * 1000


server = subprocess.Popen(
    [os.path.join(RUNTIME, "llama-server.exe"), "-m", os.path.join(MODELS, "llm", "s1-mini", "s1-mini-q4_k_m.gguf"),
     "--host", "127.0.0.1", "--port", str(PORT), "-c", "4096", "-ngl", "99", "--jinja", "-np", "1", "--no-webui"],
    cwd=RUNTIME, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
try:
    t0 = time.perf_counter()
    while True:
        try:
            if urllib.request.urlopen(BASE + "/health", timeout=2).status == 200:
                break
        except Exception:
            time.sleep(0.4)
    print(f"model loaded in {time.perf_counter() - t0:.1f} s   GPU (util %, used MB, total MB): {gpu()}")
    ask("warm up")  # the first call is slower; the app warms the model the same way
    print("Type text and press Enter (empty line quits).\n")
    for line in (sys.stdin if not sys.stdin.isatty() else iter(lambda: input("> "), "")):
        text = line.strip()
        if not text:
            break
        cpu0, _ = server_use(server.pid)
        reply, ms = ask(text)
        cpu1, mb = server_use(server.pid)
        t = reply.get("timings", {})
        out = reply["choices"][0]["message"]["content"].strip()
        print(f"\n{out}\n")
        print(f"  total      {ms:.0f} ms   (input {t.get('prompt_n', '?')} tokens in {t.get('prompt_ms', 0):.0f} ms, "
              f"output {t.get('predicted_n', '?')} tokens in {t.get('predicted_ms', 0):.0f} ms)")
        print(f"  speed      {t.get('predicted_per_second', 0):.0f} tokens/s out, {t.get('prompt_per_second', 0):.0f} tokens/s in")
        print(f"  changed    {'no' if out == text else 'yes'}   words {len(text.split())} -> {len(out.split())}   "
              f"ratio {len(out.split()) / max(1, len(text.split())):.2f}")
        print(f"  server     CPU {cpu1 - cpu0:.2f} s used this call, {mb} MB RAM")
        print(f"  GPU        util %, used MB, total MB: {gpu()}   (util is read just after the call, so it may show idle)\n")
finally:
    server.kill()
