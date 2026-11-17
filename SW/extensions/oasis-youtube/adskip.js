(() => {
  if (window.__oasisAdSkipInstalled)
    return;
  window.__oasisAdSkipInstalled = true;

  let autoSkipAds = true;
  let lastAttemptAt = 0;
  let lastReportedAt = 0;
  let pendingConfirm = false;
  let adStartedAt = 0;
  let lastVia = "none";
  let speedAdjusted = false;
  let speedVideo = null;
  let savedMuted = null;
  let savedRate = null;

  window.addEventListener("message", (ev) => {
    if (ev.source !== window || !ev.data || typeof ev.data !== "object")
      return;
    if (ev.data.type !== "oasis.youtubeSettings")
      return;
    autoSkipAds = ev.data.autoSkipAds !== false;
  });

  function getPlayer() {
    return document.getElementById("movie_player")
      || document.querySelector(".html5-video-player");
  }

  /** Only treat a real in-progress ad — leftover ad DOM nodes must not steal search focus. */
  function isAdShowing() {
    const player = getPlayer();
    return !!(player
      && (player.classList.contains("ad-showing")
          || player.classList.contains("ad-interrupting")));
  }

  function isTypingInField() {
    const el = document.activeElement;
    if (!(el instanceof HTMLElement))
      return false;
    const tag = el.tagName;
    if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT")
      return true;
    if (el.isContentEditable)
      return true;
    const role = (el.getAttribute("role") || "").toLowerCase();
    if (role === "textbox" || role === "combobox" || role === "searchbox")
      return true;
    // YouTube search / masthead query box.
    if (el.closest("ytd-searchbox, #search-input, #search-form, form#search-form"))
      return true;
    return false;
  }

  function isVisible(el) {
    if (!(el instanceof HTMLElement))
      return false;
    const style = window.getComputedStyle(el);
    if (style.display === "none" || style.visibility === "hidden" || Number(style.opacity) === 0)
      return false;
    const rect = el.getBoundingClientRect();
    return rect.width > 2 && rect.height > 2;
  }

  function skipLabel(el) {
    const text = (el.textContent || "").replace(/\s+/g, " ").trim();
    const label = (el.getAttribute("aria-label") || "").trim();
    return `${text} ${label}`.trim();
  }

  /** True when Skip is clickable — not the "Skip in 5" countdown. */
  function isSkipReady(el) {
    if (!(el instanceof HTMLElement) || !isVisible(el))
      return false;
    if (el.disabled || el.getAttribute("aria-disabled") === "true")
      return false;
    if (el.classList.contains("ytp-ad-skip-button-disabled")
        || el.hasAttribute("disabled")
        || el.getAttribute("tabindex") === "-1" && el.tagName !== "BUTTON")
      return false;

    const label = skipLabel(el);
    if (/skip\s*in\s*\d/i.test(label))
      return false;
    if (/^\d+\s*$/.test(label))
      return false;

    // Prefer clear Skip affordances; allow generic Skip* once countdown text is gone.
    if (/skip/i.test(label))
      return true;

    // Modern skip control often has no text, only icon + known class.
    return el.matches?.(
      ".ytp-skip-ad-button, .ytp-ad-skip-button, .ytp-ad-skip-button-modern, .ytp-ad-skip-button-container, .ytp-ad-skip-button-slot"
    ) === true;
  }

  function findSkipButton() {
    const selectors = [
      ".ytp-skip-ad-button",
      ".ytp-ad-skip-button-modern",
      ".ytp-ad-skip-button",
      ".ytp-ad-skip-button-text",
      "button.ytp-ad-skip-button-modern",
      ".ytp-ad-skip-button-container button",
      "button.ytp-ad-skip-button-container",
      ".ytp-ad-skip-button-slot button",
      ".ytp-ad-skip-button-slot",
      "button[class*='skip-ad' i]",
      "button[aria-label*='Skip ad' i]",
      "button[aria-label*='Skip ads' i]",
      "[role='button'][aria-label*='Skip ad' i]"
    ];

    for (const sel of selectors) {
      let nodes;
      try { nodes = document.querySelectorAll(sel); }
      catch { continue; }
      for (const el of nodes) {
        if (isSkipReady(el))
          return el;
      }
    }

    for (const el of document.querySelectorAll("button, [role='button']")) {
      if (!isSkipReady(el))
        continue;
      const label = skipLabel(el).toLowerCase();
      if (label === "skip" || /^skip(\s|$)/.test(label) || label.includes("skip ad"))
        return el;
    }

    return null;
  }

  const COUNTDOWN_SELECTORS = [
    ".ytp-ad-preview-container",
    ".ytp-ad-preview-text",
    ".ytp-ad-preview-slot",
    ".ytp-skip-ad",
    ".ytp-skip-ad-button",
    ".ytp-ad-skip-button",
    ".ytp-ad-skip-button-modern",
    ".ytp-ad-skip-button-text",
    ".ytp-ad-text"
  ];

  /**
   * True while the pre-skip counter is still running. Modern YouTube renders it as a bare
   * number inside .ytp-ad-preview-*, not as "Skip in 5" on a button, so matching text on
   * buttons alone misses it and a skippable ad gets force-sped instead of simply skipped.
   */
  function hasSkipCountdown() {
    for (const sel of COUNTDOWN_SELECTORS) {
      let nodes;
      try { nodes = document.querySelectorAll(sel); }
      catch { continue; }
      for (const el of nodes) {
        if (!isVisible(el))
          continue;
        const label = skipLabel(el);
        if (/skip\s*in\s*\d/i.test(label) || /^\d{1,2}$/.test(label))
          return true;
      }
    }

    for (const el of document.querySelectorAll("button, [role='button']")) {
      if (isVisible(el) && /skip\s*in\s*\d/i.test(skipLabel(el)))
        return true;
    }

    return false;
  }

  function forceClick(el) {
    // Never steal focus from search / text fields.
    if (!isTypingInField()) {
      try { el.focus?.({ preventScroll: true }); } catch { /* ignore */ }
    }
    try { el.click(); } catch { /* ignore */ }

    const opts = { bubbles: true, cancelable: true, view: window, composed: true };
    try {
      el.dispatchEvent(new PointerEvent("pointerdown", { ...opts, pointerId: 1, pointerType: "mouse" }));
      el.dispatchEvent(new MouseEvent("mousedown", opts));
      el.dispatchEvent(new PointerEvent("pointerup", { ...opts, pointerId: 1, pointerType: "mouse" }));
      el.dispatchEvent(new MouseEvent("mouseup", opts));
      el.dispatchEvent(new MouseEvent("click", opts));
    } catch { /* ignore */ }
  }

  function getAdVideo() {
    const player = getPlayer();
    if (!player || !(player.classList.contains("ad-showing") || player.classList.contains("ad-interrupting")))
      return null;
    return player.querySelector("video") || document.querySelector("video.html5-main-video");
  }

  function keepAdPlaying() {
    const video = getAdVideo();
    if (!(video instanceof HTMLVideoElement))
      return;
    try {
      // Never leave the ad frozen mid-skip attempt.
      if (video.paused)
        void video.play();
    } catch { /* ignore */ }
  }

  function speedUnskippableAd() {
    const video = getAdVideo();
    if (!(video instanceof HTMLVideoElement))
      return false;

    // Only after we've waited long enough and there is still no Skip control.
    // Do NOT seek to the end — that freezes many YouTube ad players with Skip still visible.
    try {
      if (!speedAdjusted) {
        speedVideo = video;
        savedMuted = video.muted;
        savedRate = video.playbackRate || 1;
        speedAdjusted = true;
      }
      video.muted = true;
      if (video.playbackRate < 8)
        video.playbackRate = 8;
      if (video.paused)
        void video.play();
      return true;
    } catch {
      return false;
    }
  }

  /**
   * YouTube plays ads through the same <video> element as the content, so a speed-up that is
   * never undone leaves the actual video muted at 8× once the ad ends. Only our own changes are
   * reverted: a rate the user changed meanwhile, or a mute they cleared, is left alone.
   */
  function restoreAdPlaybackState() {
    if (!speedAdjusted)
      return;

    speedAdjusted = false;
    const video = speedVideo instanceof HTMLVideoElement
      ? speedVideo
      : document.querySelector("video.html5-main-video");
    const wasMuted = savedMuted;
    const rate = savedRate;
    speedVideo = null;
    savedMuted = null;
    savedRate = null;

    if (!(video instanceof HTMLVideoElement))
      return;

    try {
      if (wasMuted === false && video.muted)
        video.muted = false;
      if (typeof rate === "number" && rate > 0 && video.playbackRate >= 8)
        video.playbackRate = rate;
    } catch { /* ignore */ }
  }

  function attemptSkipActions() {
    const player = getPlayer();
    if (!player)
      return "none";

    // 1) Prefer a ready Skip button — never fight the countdown with seek/API spam.
    const btn = findSkipButton();
    if (btn) {
      forceClick(btn);
      try {
        if (typeof player.skipAd === "function")
          player.skipAd();
      } catch { /* ignore */ }
      restoreAdPlaybackState();
      return "button";
    }

    // 2) Countdown still running — keep playback alive, wait for Skip to enable.
    if (hasSkipCountdown()) {
      keepAdPlaying();
      return "none";
    }

    // 3) Unskippable (or Skip UI missing): mild speed-up only, after a short grace period.
    const waitedMs = adStartedAt > 0 ? Date.now() - adStartedAt : 0;
    if (waitedMs >= 1200 && speedUnskippableAd())
      return "speed";

    keepAdPlaying();
    return "none";
  }

  function reportSkipped(via) {
    const now = Date.now();
    if (now - lastReportedAt < 1500)
      return;
    lastReportedAt = now;
    pendingConfirm = false;
    window.postMessage({ type: "oasis.adSkipped", via: via || "confirmed" }, "*");
  }

  function trySkip() {
    if (!autoSkipAds)
      return;

    if (!isAdShowing()) {
      adStartedAt = 0;
      restoreAdPlaybackState();
      if (pendingConfirm)
        reportSkipped(lastVia);
      return;
    }

    if (adStartedAt === 0)
      adStartedAt = Date.now();

    const now = Date.now();
    // Poll faster once an ad is up so Skip is clicked as soon as it unlocks.
    const minGap = findSkipButton() ? 80 : 150;
    if (now - lastAttemptAt < minGap)
      return;
    lastAttemptAt = now;

    const via = attemptSkipActions();
    if (via === "none")
      return;

    lastVia = via;
    pendingConfirm = true;

    setTimeout(() => {
      if (!isAdShowing() && pendingConfirm)
        reportSkipped(via);
    }, 350);
  }

  const observer = new MutationObserver(() => trySkip());
  observer.observe(document.documentElement, {
    childList: true,
    subtree: true,
    attributes: true,
    attributeFilter: ["class", "disabled", "aria-disabled"]
  });
  setInterval(trySkip, 150);
  trySkip();
})();
