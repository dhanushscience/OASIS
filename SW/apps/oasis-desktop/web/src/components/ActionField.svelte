<script>
  import { installed, running, loadRunning } from '../apps.svelte.js';
  import { detect, suggestions, chordFromKeyEvent, parseChord, MEDIA } from '../actionKinds.js';

  /**
   * seg: 0 keys, 1 app or site, 2 text, -1 another kind named by `kind`.
   * `onedit` gets whatever changed; `name` is a suggested name on the overlay.
   * @type {{ value: string, seg: number, kind: string, readonly: boolean, label: string, placeholder: string, help: string, pulse?: number, onedit: (patch: { value?: string, seg?: number, kind?: string, name?: string }) => void }}
   */
  let { value, seg, kind, readonly, label, placeholder, help, pulse = 0, onedit } = $props();

  // ---- what is offered under the box
  let focused = $state(false);
  let active = $state(0);
  let dismissed = $state(false);
  const recording = $derived(seg === 0 && !readonly);
  let mediaOpen = $state(false);
  // App / site: what is open now comes first (all of it while the box is empty), then installed apps and sites.
  const items = $derived.by(() => {
    if (!focused || dismissed || readonly || seg === 0) return [];
    const q = value.trim().toLowerCase();
    const open = seg === 1
      ? running.list.filter((r) => !q || r.name.toLowerCase().includes(q) || r.title.toLowerCase().includes(q)).slice(0, 8)
          .map((r) => ({ kind: 'app', name: r.name, value: r.path, note: 'Running' }))
      : [];
    const openPaths = new Set(open.map((o) => o.value.toLowerCase()));
    return [...open, ...suggestions(value, installed.names, seg).filter((i) => !openPaths.has(i.value.toLowerCase()))];
  });
  $effect(() => { value; active = 0; });

  // The list is fixed to the window (so the panel does not clip or scroll it), under the box, or above
  // it when there is little room below; it never runs past the panel it belongs to.
  /** @type {HTMLElement | undefined} */
  let boxEl = $state();
  let listStyle = $state('');
  function place() {
    if (!boxEl) return;
    const r = boxEl.getBoundingClientRect();
    // stay inside the panel the box is in (else the window)
    const room = (boxEl.closest('aside') ?? document.documentElement).getBoundingClientRect();
    const below = Math.min(room.bottom, window.innerHeight) - r.bottom - 14, above = r.top - Math.max(room.top, 0) - 14;
    const up = below < 150 && above > below;
    const max = Math.max(100, Math.min(260, up ? above : below));
    listStyle = `left: ${r.left}px; width: ${r.width}px; max-height: ${max}px; ${up ? `bottom: ${window.innerHeight - r.top + 4}px` : `top: ${r.bottom + 4}px`}`;
  }
  $effect(() => { if (items.length) place(); });

  /** @param {{ kind: string, name: string, value: string }} item */
  function pick(item) {
    dismissed = true;
    if (item.kind === 'quick') onedit({ value: item.value, seg: 0, kind: '', name: item.name.toUpperCase() });
    else onedit({ value: item.value, seg: 1, kind: item.kind === 'site' ? 'OpenUrl' : 'LaunchApp' });
  }

  /** @param {Event & { currentTarget: HTMLInputElement }} e */
  function typed(e) {
    const text = e.currentTarget.value;
    dismissed = false;
    const found = seg === 0 || seg === 1 ? detect(text, installed.names) : null;
    if (found === 'keys' && seg !== 0) onedit({ value: text, seg: 0, kind: '' });
    else if (found === 'site' && seg !== 1) onedit({ value: text, seg: 1, kind: 'OpenUrl' });
    else if (found === 'app' && seg !== 1) onedit({ value: text, seg: 1, kind: '' });
    else onedit({ value: text, ...(seg === 1 ? { kind: '' } : {}) });
  }

  /** @param {KeyboardEvent & { currentTarget: HTMLInputElement }} e */
  function keys(e) {
    if (readonly) return;
    if (items.length) {
      if (e.key === 'ArrowDown') { e.preventDefault(); active = (active + 1) % items.length; return; }
      if (e.key === 'ArrowUp') { e.preventDefault(); active = (active - 1 + items.length) % items.length; return; }
      if (e.key === 'Enter') { e.preventDefault(); pick(items[active]); return; }
      if (e.key === 'Escape') { e.preventDefault(); e.stopPropagation(); dismissed = true; return; }
    }
    if (recording) {
      // Keys are pressed, not typed: Tab is taken like any key; Esc leaves the box, Backspace clears it.
      const mods = e.ctrlKey || e.altKey || e.metaKey || e.shiftKey;
      e.preventDefault();
      if (e.key === 'Escape' && !mods) { e.stopPropagation(); e.currentTarget.blur(); return; }
      if ((e.key === 'Backspace' || e.key === 'Delete') && !mods) return onedit({ value: '' });
      const chord = chordFromKeyEvent(e, true);
      if (chord) onedit({ value: chord });
    }
  }

  // ---- a word of warning while typing, before the engine has its say on Save
  const warning = $derived.by(() => {
    if (!value.trim()) return '';
    if (seg === 0 && !parseChord(value)) return 'OASIS does not know one of these keys.';
    if (seg === -1 && kind === 'MouseClick' && !/^\s*\d+\s*,\s*\d+\s*(,\s*(left|right|middle)\s*)?$/i.test(value)) return 'Use x,y (for example 120,340), or x,y,right.';
    return '';
  });
</script>

<div class="field">
  <span class="labrow">
    <span class="lab">{label}</span>
    {#if recording}
      <button type="button" class="more" class:on={mediaOpen} aria-expanded={mediaOpen} aria-label="More keys" title="More keys: play / pause, next, previous, stop" onclick={() => (mediaOpen = !mediaOpen)}>
        <svg viewBox="0 0 24 24" aria-hidden="true"><circle cx="5" cy="12" r="2" /><circle cx="12" cy="12" r="2" /><circle cx="19" cy="12" r="2" /></svg>
      </button>
    {/if}
  </span>
  <span class="box" bind:this={boxEl}>
    {#key pulse}{#if pulse}<span class="ring" aria-hidden="true"></span>{/if}{/key}
    <input id="action-box" class="mono" class:rec={recording} {value} readonly={readonly || recording} placeholder={recording ? (focused ? 'Press the keys…' : 'Click to set keys') : placeholder} autocomplete="off" spellcheck="false" role="combobox"
      aria-expanded={items.length > 0} aria-controls="action-list" aria-autocomplete="list" aria-activedescendant={items.length ? `action-opt-${active}` : undefined}
      oninput={typed} onkeydown={keys} onfocus={() => { focused = true; dismissed = false; if (seg === 1) loadRunning(); }} onblur={() => (focused = false)} />
  </span>

  {#if items.length}
    <ul id="action-list" class="list" role="listbox" aria-label="Suggestions" style={listStyle}>
      {#each items as item, i}
        <!-- mousedown, not click: the box must not lose focus (and close the list) first -->
        <li id="action-opt-{i}" role="option" aria-selected={i === active} class:on={i === active} onmousedown={(e) => { e.preventDefault(); pick(item); }}>
          <span class="n">{item.name}</span><span class="note mono">{item.note}</span>
        </li>
      {/each}
    </ul>
  {/if}

  {#if recording && mediaOpen}
    <!-- the media keys often never reach a page, so they are picked here -->
    <span class="media" role="group" aria-label="Media keys">
      {#each MEDIA as m}<button type="button" class:on={value === m.keys} onmousedown={(e) => { e.preventDefault(); onedit({ value: m.keys, name: m.name.toUpperCase() }); }}>{m.name}</button>{/each}
    </span>
  {/if}

  {#if warning}<span class="warn" role="status">{warning}</span>{/if}
  <span class="help">{help}</span>
</div>

<style>
  /* the heart of the panel: large, and centred in the room that is left */
  .field { position: relative; display: flex; flex-direction: column; gap: 14px; margin-top: 12px; }
  .labrow { display: flex; align-items: center; justify-content: space-between; min-height: 24px; }
  .more { width: 24px; height: 24px; display: flex; align-items: center; justify-content: center; padding: 0; border-radius: 50%; border: 1px solid var(--line); background: transparent; color: var(--ink); }
  .more.on { background: var(--ink); color: var(--inv); border-color: var(--ink); }
  .more svg { width: 14px; height: 14px; fill: currentColor; }
  .lab { font: 600 13px/1.3 var(--sans); letter-spacing: .08em; text-transform: uppercase; color: var(--muted); }
  .mono { font-family: var(--mono); }
  .media { display: flex; flex-wrap: wrap; gap: 6px; }
  .media button { height: 28px; padding: 0 12px; border-radius: var(--r-pill); border: 1px solid var(--line); background: transparent; color: var(--ink); font-size: 12px; font-weight: 600; }
  .media button.on { background: var(--ink); color: var(--inv); border-color: var(--ink); }
  .box { position: relative; height: 64px; border-radius: var(--r-row); border: 1.5px solid var(--ink); background: var(--surface); display: flex; align-items: center; gap: 8px; padding: 0 10px 0 18px; }
  .box:has(input:focus-visible) { outline: 2px solid var(--signal); outline-offset: 2px; }
  /* a saved box: one ring that grows and fades (240 ms, transform and opacity only) */
  .ring { position: absolute; inset: -1.5px; border: 2px solid var(--signal); border-radius: var(--r-row); pointer-events: none; animation: pulse .24s var(--ease) forwards; }
  @keyframes pulse { from { opacity: .9; transform: scale(1); } to { opacity: 0; transform: scale(1.03, 1.14); } }
  input { border: 0; background: transparent; flex: 1; height: 100%; padding: 0; min-width: 0; font-size: 22px; font-weight: 600; color: var(--ink); outline: none; }
  input.rec { cursor: pointer; caret-color: transparent; }
  .box:has(input.rec:focus) { border-color: var(--signal); }
  input::placeholder { font-size: 16px; font-weight: 400; }
  .list { position: fixed; z-index: 60; margin: 0; padding: 4px; list-style: none; background: var(--surface); border: 1px solid var(--line); border-radius: var(--r-row); box-shadow: 0 8px 28px var(--shadow); max-height: 260px; overflow: auto; }
  .list li { display: flex; align-items: center; justify-content: space-between; gap: 10px; padding: 8px 10px; border-radius: var(--r-field); font-size: 13px; cursor: pointer; }
  .list li.on { background: var(--sunk); }
  .n { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .note { font-size: 11px; color: var(--muted); flex: none; }
  .help { font-size: 13px; color: var(--muted); }
  .warn { font-size: 12px; color: var(--bad); }
  @media (max-height: 800px) { .box { height: 52px; } }
  @media (max-height: 700px) { .help { display: none; } }
</style>
