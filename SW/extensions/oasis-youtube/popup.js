const HOLD_RATE_KEY = "holdPlaybackRate";
const DEFAULT_HOLD_RATE = 2;

function clampHoldRate(value) {
  const n = Number(value);
  if (!Number.isFinite(n))
    return DEFAULT_HOLD_RATE;
  return Math.min(4, Math.max(1.25, Math.round(n * 100) / 100));
}

async function loadHoldRate() {
  const stored = await chrome.storage.local.get(HOLD_RATE_KEY);
  return clampHoldRate(stored[HOLD_RATE_KEY] ?? DEFAULT_HOLD_RATE);
}

async function saveHoldRate(rate) {
  const next = clampHoldRate(rate);
  await chrome.storage.local.set({ [HOLD_RATE_KEY]: next });
  try {
    await chrome.runtime.sendMessage({
      type: "oasis.holdRate",
      holdPlaybackRate: next
    });
  } catch {
    // Background may be restarting.
  }
  return next;
}

async function refreshConnection() {
  const statusEl = document.getElementById("status");
  const reasonEl = document.getElementById("statusReason");

  const setReason = (text) => {
    if (reasonEl)
      reasonEl.textContent = text || "";
  };

  try {
    const response = await chrome.runtime.sendMessage({ type: "oasis.status" });
    if (!response) {
      statusEl.textContent = "OFFLINE";
      statusEl.className = "status-pill bad";
      setReason("Background worker is not responding. Reload the extension.");
      return;
    }

    statusEl.textContent = response.connected ? "CONNECTED" : "DISCONNECTED";
    statusEl.className = "status-pill " + (response.connected ? "ok" : "bad");
    // A bare DISCONNECTED hides why — a rejected token looks identical to OASIS being closed.
    setReason(response.connected ? "" : response.lastError);
  } catch {
    statusEl.textContent = "ERROR";
    statusEl.className = "status-pill bad";
    setReason("Could not reach the extension background worker.");
  }
}

async function init() {
  const holdRateEl = document.getElementById("holdRate");
  const rate = await loadHoldRate();
  holdRateEl.value = String(rate);
  // If stored value isn't in the list, add it.
  if (![...holdRateEl.options].some((o) => o.value === String(rate))) {
    const opt = document.createElement("option");
    opt.value = String(rate);
    opt.textContent = `${rate}×`;
    holdRateEl.appendChild(opt);
    holdRateEl.value = String(rate);
  }

  holdRateEl.addEventListener("change", async () => {
    await saveHoldRate(holdRateEl.value);
  });

  document.getElementById("retry").addEventListener("click", async () => {
    await chrome.runtime.sendMessage({ type: "oasis.reconnect" });
    setTimeout(refreshConnection, 500);
  });

  await refreshConnection();
  setInterval(refreshConnection, 1500);
}

init();
