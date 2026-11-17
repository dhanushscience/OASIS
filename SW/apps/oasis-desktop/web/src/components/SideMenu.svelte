<script>
  let { sections, current, onpick } = $props();

  let version = $state('');
  window.__TAURI__?.app.getVersion().then((/** @type {string} */ v) => (version = v), () => {});

  // 20px stroke icons, one per section (24x24 viewBox).
  const icons = /** @type {Record<string, string>} */ ({
    profiles: "M5 4h14v16H5zM9 8h6M9 12h6M9 16h3",
    "quick-launch": "M13 3 5 14h6l-1 7 8-11h-6z",
    voice: "M12 3a3 3 0 0 0-3 3v6a3 3 0 0 0 6 0V6a3 3 0 0 0-3-3zM6 11a6 6 0 0 0 12 0M12 17v4",
    insights: "M5 20V11M12 20V4M19 20v-6M3 20h18",
    input: "M3 7h18v10H3zM7 11h.01M11 11h.01M15 11h.01M8 14h8",
    overlay: "M3 5h18v11H3zM8 20h8M12 16v4",
    settings: "M4 7h10M18 7h2M4 17h2M10 17h10M16 4v6M8 14v6",
  });

  // Up and down move through the sections, as a tab list does.
  /** @param {KeyboardEvent} event */
  function keys(event) {
    const step = /** @type {Record<string, number>} */ ({ ArrowDown: 1, ArrowUp: -1 })[event.key];
    if (!step) return;
    const real = sections.filter((/** @type {{ id: string }} */ s) => s.id);
    const next = real[(real.findIndex((/** @type {{ id: string }} */ s) => s.id === current) + step + real.length) % real.length];
    event.preventDefault();
    onpick(next.id);
    queueMicrotask(() => /** @type {HTMLElement | null} */ (document.querySelector(`[data-section="${next.id}"]`))?.focus());
  }
</script>

<div class="logo">
  <span class="bar"></span>
  <div class="lockup">
    <img class="mark ink" src="./oasis-logo-ink.png" alt="Project OASIS" />
    <img class="mark paper" src="./oasis-logo-paper.png" alt="" aria-hidden="true" />
    <div class="ver"><span>VERSION {version}</span><span>BETA</span></div>
  </div>
</div>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions: the keys come from the buttons inside -->
<nav aria-label="Sections" onkeydown={keys}>
  {#each sections as s}
    {#if s.id}
      <button data-section={s.id} aria-current={s.id === current ? 'page' : undefined} class:cur={s.id === current}
        tabindex={s.id === current ? 0 : -1} onclick={() => onpick(s.id)}>
        <svg class="ico" viewBox="0 0 24 24" aria-hidden="true"><path d={icons[s.id]} /></svg>
        <span class="word">{s.name}</span>
      </button>
    {:else}
      <span class="gap" aria-hidden="true"></span>
    {/if}
  {/each}
</nav>

<style>
  .logo { margin-left: -12px; display: flex; align-items: stretch; gap: 14px; padding: 0 6px 0 0; }
  .bar { width: 10px; background: var(--signal); flex: none; }
  .lockup { width: 160px; }
  .mark { display: block; width: 160px; height: auto; }
  .paper { display: none; }
  :global(:root[data-theme='dark']) .ink { display: none; }
  :global(:root[data-theme='dark']) .paper { display: block; }
  .ver { display: flex; justify-content: space-between; font: 400 11px/1.3 var(--mono); letter-spacing: .22em; text-transform: uppercase; color: var(--muted); margin-top: 10px; }
  nav { margin-top: 56px; display: flex; flex-direction: column; gap: 4px; }
  button { display: flex; align-items: center; gap: 12px; height: 44px; border: 0; border-radius: var(--r-row) 0 0 var(--r-row); margin-right: -12px; padding: 0 14px; font-size: 16px; background: transparent; color: var(--muted);
    transition: background-color .24s var(--ease), color .24s var(--ease); }
  button.cur { background: var(--ink); color: var(--inv); font-weight: 600; }
  /* Dark: a raised graphite row, not a white slab that outshines the page. */
  :global(:root[data-theme='dark']) button.cur { background: var(--side); color: var(--ink); box-shadow: inset 0 0 0 1px var(--line); }
  .ico { width: 20px; height: 20px; flex: none; fill: none; stroke: currentColor; stroke-width: 1.7; stroke-linecap: round; stroke-linejoin: round; }
  .word { flex: 1; text-align: left; }
  /* Settings sits apart from the three sections above it, lower down */
  .gap { height: clamp(16px, 9dvh, 76px); }
</style>
