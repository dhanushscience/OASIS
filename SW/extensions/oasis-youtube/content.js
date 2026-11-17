(() => {
  if (window.__oasisYoutubeBridgeInstalled)
    return;
  window.__oasisYoutubeBridgeInstalled = true;

  let lastVideoId = "";
  let lastStartedId = "";
  let lastEndedId = "";
  let lastProgressBucket = -1;
  let attachedVideo = null;
  let lastReadyKey = "";
  let settings = {
    autoSkipAds: true,
    gesturesEnabled: true,
    holdPlaybackRate: 2
  };

  function clampHoldRate(value) {
    const n = Number(value);
    if (!Number.isFinite(n))
      return 2;
    return Math.min(4, Math.max(1.25, Math.round(n * 100) / 100));
  }

  function loadHoldRateFromStorage() {
    try {
      chrome.storage.local.get("holdPlaybackRate", (stored) => {
        void chrome.runtime.lastError;
        if (stored && stored.holdPlaybackRate != null)
          settings.holdPlaybackRate = clampHoldRate(stored.holdPlaybackRate);
      });
    } catch {
      // Ignore.
    }
  }

  chrome.storage.onChanged.addListener((changes, area) => {
    if (area !== "local" || !changes.holdPlaybackRate)
      return;
    settings.holdPlaybackRate = clampHoldRate(changes.holdPlaybackRate.newValue);
  });

  loadHoldRateFromStorage();
  let savedRate = 1;
  let rateBoosted = false;

  function pushSettingsToPage() {
    try {
      window.postMessage({
        type: "oasis.youtubeSettings",
        autoSkipAds: settings.autoSkipAds !== false
      }, "*");
    } catch {
      // Ignore.
    }
  }

  window.addEventListener("message", (ev) => {
    if (ev.source !== window || !ev.data || typeof ev.data !== "object")
      return;
    if (ev.data.type !== "oasis.adSkipped")
      return;
    send("youtube.ad_skipped", { via: ev.data.via || "page" });
  });

  function getVideoId() {
    try {
      const url = new URL(location.href);
      if (url.pathname.startsWith("/shorts/")) {
        const parts = url.pathname.split("/").filter(Boolean);
        return parts[1] || "";
      }
      if (url.pathname.startsWith("/live/")) {
        const parts = url.pathname.split("/").filter(Boolean);
        return parts[1] || "";
      }
      if (url.hostname.includes("youtu.be")) {
        const parts = url.pathname.split("/").filter(Boolean);
        return parts[0] || "";
      }
      return url.searchParams.get("v") || "";
    } catch {
      return "";
    }
  }

  function getTitle() {
    const el = document.querySelector(
      "h1.ytd-watch-metadata yt-formatted-string, h1.title yt-formatted-string, h1.title, h1"
    );
    const text = (el?.textContent || document.title || "").trim();
    return text.replace(/\s*-\s*YouTube\s*$/i, "").trim();
  }

  function isShortSurface() {
    try {
      const url = new URL(location.href);
      if (url.pathname.startsWith("/shorts/"))
        return true;
    } catch {
      // Fall through to DOM checks.
    }

    // Shorts can also appear in the reel player without /shorts/ still settling in the URL.
    if (document.querySelector("ytd-reel-video-renderer[is-active], ytd-shorts, #shorts-player"))
      return true;

    return false;
  }

  function send(type, extra = {}) {
    const payload = {
      type,
      videoId: getVideoId(),
      title: getTitle(),
      url: location.href,
      surface: isShortSurface() ? "short" : "watch",
      ...extra
    };
    try {
      chrome.runtime.sendMessage(payload, () => {
        void chrome.runtime.lastError;
      });
    } catch {
      // Extension context invalidated during reload.
    }
  }

  function markStarted(force = false) {
    const id = getVideoId();
    if (!id || (!force && id === lastStartedId))
      return;
    lastStartedId = id;
    lastEndedId = "";
    lastProgressBucket = -1;
    send("youtube.started");
  }

  function markEnded() {
    if (isShortSurface())
      return;
    const id = getVideoId();
    if (!id || id === lastEndedId)
      return;
    lastEndedId = id;
    send("youtube.ended");
  }

  function onPlay() {
    markStarted();
  }

  function onPlaying() {
    markStarted();
  }

  function onEnded() {
    markEnded();
  }

  function onTimeUpdate(ev) {
    const video = ev.target;
    if (!(video instanceof HTMLVideoElement) || !video.duration || !isFinite(video.duration))
      return;

    if (!video.paused && video.currentTime > 0.05)
      markStarted();

    if (!isShortSurface()) {
      const percent = Math.floor((video.currentTime / video.duration) * 100);

      // Seek-back / replay after near-end: clear ended sticky and re-announce playing.
      if (percent < 95 && lastEndedId) {
        lastEndedId = "";
        lastProgressBucket = -1;
        markStarted(true);
      } else if (percent + 5 < lastProgressBucket) {
        // Large rewind — allow progress buckets to fire again for OASIS recovery.
        lastProgressBucket = -1;
      }

      for (const bucket of [90, 95, 98]) {
        if (percent < bucket || lastProgressBucket >= bucket)
          continue;
        lastProgressBucket = bucket;
        send("youtube.progress", {
          percent: bucket,
          positionSec: video.currentTime,
          durationSec: video.duration
        });
      }

      // Only the real ended event flips OASIS to end options permanently.
      // Near-end still sends progress@98 so labels can peek without sticking.
      if (video.ended)
        markEnded();
    }
  }

  function getMainVideoEl() {
    return attachedVideo || findMainVideo();
  }

  function setPlaybackRate(rate) {
    const video = getMainVideoEl();
    if (!(video instanceof HTMLVideoElement))
      return;
    try {
      if (!rateBoosted) {
        // An ad speed-up may still be applied; never memorise that as the user's rate.
        const current = video.playbackRate || 1;
        savedRate = current > 4 ? 1 : current;
        rateBoosted = true;
      }
      video.playbackRate = rate;
    } catch {
      // Ignore.
    }
  }

  function restorePlaybackRate() {
    const video = getMainVideoEl();
    if (!(video instanceof HTMLVideoElement)) {
      rateBoosted = false;
      return;
    }
    try {
      video.playbackRate = savedRate || 1;
    } catch {
      // Ignore.
    }
    rateBoosted = false;
  }

  function applySettings(next) {
    if (!next || typeof next !== "object")
      return;
    settings = {
      autoSkipAds: next.autoSkipAds !== false,
      gesturesEnabled: next.gesturesEnabled !== false,
      holdPlaybackRate: next.holdPlaybackRate != null
        ? clampHoldRate(next.holdPlaybackRate)
        : settings.holdPlaybackRate
    };
    pushSettingsToPage();
  }

  function handleOasisMessage(message) {
    if (!message || typeof message !== "object")
      return;

    if (message.type === "oasis.settings" && message.youtube)
      applySettings(message.youtube);

    if (message.type === "oasis.holdRate")
      settings.holdPlaybackRate = clampHoldRate(message.holdPlaybackRate);

    if (message.type === "oasis.command") {
      const cmd = String(message.command || "");
      // Hold speed is configurable in the extension popup (not fixed at 2×).
      if (cmd === "playbackRate.hold2x" || cmd === "playbackRate.hold") {
        if (isShortSurface()) {
          restorePlaybackRate();
          return;
        }
        const fromPayload = message.payload && message.payload.rate != null
          ? clampHoldRate(message.payload.rate)
          : settings.holdPlaybackRate;
        setPlaybackRate(fromPayload);
      } else if (cmd === "playbackRate.restore") {
        restorePlaybackRate();
      } else if (cmd === "playback.pause") {
        pauseIfPlaying();
      } else if (cmd === "playback.resume") {
        resumeIfPausedByOasis();
      } else if (cmd === "playback.play") {
        resumeActiveVideo();
      } else if (cmd === "settings.push" && message.payload?.youtube) {
        applySettings(message.payload.youtube);
      }
    }
  }

  function getActiveVideo() {
    return attachedVideo
      || document.querySelector("#movie_player video.html5-main-video")
      || document.querySelector("#movie_player video")
      || document.querySelector("ytd-player video")
      || document.querySelector("video.html5-main-video")
      || document.querySelector("video");
  }

  // OASIS pauses the video while its microphone is open and resumes it afterwards. Only a
  // video that was playing is paused, and only a video paused here is resumed: one the viewer
  // paused themselves must stay paused. The stamp is when OASIS paused it (0 = it did not).
  let pausedByOasisAt = 0;
  const DUPLICATE_COMMAND_MS = 2000;      // OASIS sends each command twice, 80 ms apart.
  const PAUSE_OWNERSHIP_MS = 10 * 60000;  // A resume that never came must not fire much later.

  function pauseIfPlaying() {
    const video = getActiveVideo();
    const playing = video instanceof HTMLVideoElement && !video.paused && !video.ended;
    if (playing) {
      pausedByOasisAt = Date.now();
      pauseActiveVideo();
      return;
    }

    // Already paused. Unless this is the echo of the command that just paused it, the viewer
    // paused it, and any older claim is stale.
    if (Date.now() - pausedByOasisAt > DUPLICATE_COMMAND_MS)
      pausedByOasisAt = 0;
  }

  function resumeIfPausedByOasis() {
    const owned = pausedByOasisAt > 0 && Date.now() - pausedByOasisAt < PAUSE_OWNERSHIP_MS;
    pausedByOasisAt = 0;
    if (owned)
      resumeActiveVideo();
  }

  function pauseActiveVideo() {
    try {
      const player = document.getElementById("movie_player");
      if (player && typeof player.pauseVideo === "function") {
        player.pauseVideo();
        return;
      }
    } catch {
      // Fall through to <video>.
    }

    const video = getActiveVideo();
    if (video && !video.paused)
      video.pause();
  }

  function resumeActiveVideo() {
    try {
      const player = document.getElementById("movie_player");
      if (player && typeof player.playVideo === "function") {
        player.playVideo();
        return;
      }
    } catch {
      // Fall through to <video>.
    }

    const video = getActiveVideo();
    if (video && video.paused)
      void video.play();
  }

  function detach() {
    if (!attachedVideo)
      return;
    attachedVideo.removeEventListener("play", onPlay);
    attachedVideo.removeEventListener("playing", onPlaying);
    attachedVideo.removeEventListener("ended", onEnded);
    attachedVideo.removeEventListener("timeupdate", onTimeUpdate);
    attachedVideo = null;
  }

  function attach(video) {
    if (!(video instanceof HTMLVideoElement))
      return;
    if (attachedVideo === video)
      return;

    detach();
    attachedVideo = video;
    video.addEventListener("play", onPlay);
    video.addEventListener("playing", onPlaying);
    video.addEventListener("ended", onEnded);
    video.addEventListener("timeupdate", onTimeUpdate);

    const id = getVideoId();
    const readyKey = `${id}|${location.href}`;
    if (id && readyKey !== lastReadyKey) {
      lastReadyKey = readyKey;
      // Don't spam ready while an ad is covering the player.
      const player = document.querySelector(".html5-video-player");
      const adShowing = player?.classList?.contains("ad-showing")
        || player?.classList?.contains("ad-interrupting");
      if (!adShowing)
        send("youtube.ready");
    }

    if (!video.paused)
      markStarted();

    if (video.ended && !isShortSurface())
      markEnded();
  }

  function findMainVideo() {
    const candidates = [
      ...document.querySelectorAll("video.html5-main-video"),
      ...document.querySelectorAll("ytd-player video"),
      ...document.querySelectorAll("#movie_player video"),
      ...document.querySelectorAll("video")
    ];
    for (const video of candidates) {
      if (!(video instanceof HTMLVideoElement))
        continue;
      const rect = video.getBoundingClientRect();
      if (rect.width >= 160 && rect.height >= 90)
        return video;
    }
    return candidates.find((v) => v instanceof HTMLVideoElement) || null;
  }

  function scan() {
    // Don't thrash the DOM / video attach while the user is typing in search.
    if (isTypingInField())
      return;

    const id = getVideoId();
    if (id !== lastVideoId) {
      lastVideoId = id;
      lastStartedId = "";
      lastEndedId = "";
      lastProgressBucket = -1;
      lastReadyKey = "";
      rateBoosted = false;
    }

    // Leaving a watch page for Shorts must never keep a hold-2× boost.
    if (isShortSurface() && rateBoosted)
      restorePlaybackRate();

    const video = findMainVideo();
    if (video)
      attach(video);
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
    if (el.closest("ytd-searchbox, #search-input, #search-form, form#search-form"))
      return true;
    return false;
  }

  chrome.runtime.onMessage.addListener((message, _sender, sendResponse) => {
    handleOasisMessage(message);
    try { sendResponse({ ok: true }); } catch { /* ignore */ }
    return false;
  });

  const observer = new MutationObserver(() => scan());
  observer.observe(document.documentElement, { childList: true, subtree: true });

  document.addEventListener("yt-navigate-finish", () => scan(), true);
  document.addEventListener("yt-page-data-updated", () => scan(), true);
  window.addEventListener("yt-navigate-finish", () => scan(), true);

  pushSettingsToPage();
  scan();
  setInterval(scan, 1000);
  setInterval(pushSettingsToPage, 5000);
})();
