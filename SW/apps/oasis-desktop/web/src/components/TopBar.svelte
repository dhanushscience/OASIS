<script>
  import { data } from '../profiles.svelte.js';
  import { settingsUi, openSettings } from '../settings.svelte.js';

  /** @type {{ narrow?: boolean, menu?: boolean, onmenu?: () => void, onprofile?: (index: number) => void }} */
  let { narrow = false, menu = false, onmenu, onprofile } = $props();

  const win = () => window.__TAURI__?.window.getCurrentWindow();

  let query = $state('');
  let focused = $state(false);
  let active = $state(0);
  /** @type {HTMLInputElement | undefined} */
  let box = $state();

  // Profiles by their name or the app or site they apply to; then one row that searches the settings.
  const items = $derived.by(() => {
    const q = query.trim().toLowerCase();
    if (!q) return [];
    const profiles = data.profiles
      .filter((p) => `${p.name} ${p.context_match}`.toLowerCase().includes(q))
      .sort((a, b) => Number(!a.name.toLowerCase().startsWith(q)) - Number(!b.name.toLowerCase().startsWith(q)))
      .slice(0, 6)
      .map((p) => ({ kind: 'profile', index: p.index, name: p.name, note: p.context_match && p.context_match !== '*' ? p.context_match : 'Profile' }));
    return [...profiles, { kind: 'settings', index: -1, name: `Search settings for “${query.trim()}”`, note: 'Settings' }];
  });
  $effect(() => { void query; active = 0; });

  /** @param {{ kind: string, index: number }} item */
  function pick(item) {
    const q = query.trim();
    query = '';
    box?.blur();
    if (item.kind === 'profile') onprofile?.(item.index);
    else openSettings(undefined, q);
  }

  /** @param {KeyboardEvent} e */
  function keys(e) {
    if (e.key === 'Escape') { e.preventDefault(); e.stopPropagation(); query = ''; box?.blur(); return; }
    if (!items.length) return;
    if (e.key === 'ArrowDown' || e.key === 'ArrowUp') { e.preventDefault(); active = (active + (e.key === 'ArrowDown' ? 1 : -1) + items.length) % items.length; }
    else if (e.key === 'Enter') { e.preventDefault(); pick(items[active]); }
  }

  // Ctrl+F from anywhere on the home screen (the Settings pop-up has its own search).
  function shortcut(/** @type {KeyboardEvent} */ e) {
    if (settingsUi.open || !(e.ctrlKey || e.metaKey) || e.key.toLowerCase() !== 'f') return;
    e.preventDefault();
    box?.focus();
    box?.select();
  }
</script>

<svelte:window onkeydown={shortcut} />

<div class="bar" data-tauri-drag-region>
  {#if narrow}
    <button class="menu" aria-label="Menu" aria-expanded={menu} onclick={onmenu}><svg viewBox="0 0 24 24" aria-hidden="true"><path d="M4 7h16M4 12h16M4 17h16" /></svg></button>
  {/if}

  <div class="find">
    <label class="search">
      <svg class="mag" viewBox="0 0 24 24" aria-hidden="true"><circle cx="11" cy="11" r="7" /><path d="m20 20-3.5-3.5" /></svg>
      <input bind:this={box} bind:value={query} onkeydown={keys} onfocus={() => (focused = true)} onblur={() => (focused = false)}
        role="combobox" aria-label="Search profiles and settings" aria-expanded={focused && items.length > 0} aria-controls="top-results" aria-autocomplete="list"
        aria-activedescendant={items.length ? `top-hit-${active}` : undefined} placeholder="Search profiles, settings" autocomplete="off" spellcheck="false" />
      {#if !query}<span class="kbd" aria-hidden="true">Ctrl F</span>{/if}
    </label>
    {#if focused && items.length}
      <ul id="top-results" class="list" role="listbox" aria-label="Results">
        {#each items as item, i}
          <!-- mousedown, not click: the box must not lose focus (and close the list) first -->
          <li id="top-hit-{i}" role="option" aria-selected={i === active} class:on={i === active} onmouseenter={() => (active = i)} onmousedown={(e) => { e.preventDefault(); pick(item); }}>
            <span class="n">{item.name}</span><span class="note mono">{item.note}</span>
          </li>
        {/each}
      </ul>
    {/if}
  </div>

  <div class="grow" data-tauri-drag-region></div>

  <div class="win">
    <button aria-label="Minimise" onclick={() => win()?.minimize()}><svg viewBox="0 0 12 12" aria-hidden="true"><path d="M2 6h8" /></svg></button>
    <button aria-label="Maximise" onclick={() => win()?.toggleMaximize()}><svg viewBox="0 0 12 12" aria-hidden="true"><rect x="2.5" y="2.5" width="7" height="7" rx="1" /></svg></button>
    <button aria-label="Close to tray" onclick={() => win()?.close()}><svg viewBox="0 0 12 12" aria-hidden="true"><path d="m2.5 2.5 7 7M9.5 2.5l-7 7" /></svg></button>
  </div>
</div>

<style>
  .bar { height: 40px; flex: none; display: flex; align-items: center; gap: 10px; position: relative; z-index: 20; }
  /* the spacer comes first, so the search sits at the right, beside the window controls */
  .grow { flex: 1; align-self: stretch; order: 1; }
  .find { position: relative; width: min(320px, 100%); order: 2; }
  .win { order: 3; }
  .search { height: 34px; border-radius: var(--r-pill); background: var(--surface); box-shadow: 0 1px 3px var(--shadow), 0 0 0 1px var(--line); display: flex; align-items: center; gap: 10px; padding: 0 14px; color: var(--muted); transition: box-shadow .2s var(--ease); }
  .search:focus-within { box-shadow: 0 1px 3px var(--shadow), 0 0 0 1.5px var(--ink); }
  .search input { border: 0; background: transparent; flex: 1; min-width: 0; font-size: 13px; outline: none; color: var(--ink); }
  .search input::placeholder { color: var(--muted); }
  .mag { width: 15px; height: 15px; flex: none; stroke-width: 1.9; }
  .kbd { font: 400 11px/1 var(--mono); border: 1px solid var(--line); border-radius: 6px; padding: 2px 6px; }

  .list { position: absolute; top: 40px; right: 0; width: max(100%, 320px); margin: 0; padding: 6px; list-style: none; background: var(--surface); border: 1px solid var(--line); border-radius: var(--r-row); box-shadow: 0 14px 36px var(--shadow); }
  .list li { display: flex; align-items: baseline; justify-content: space-between; gap: 12px; padding: 9px 12px; border-radius: 10px; font-size: 13px; cursor: pointer; }
  .list li.on { background: var(--sunk); }
  .n { font-weight: 600; color: var(--ink); min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .note { flex: none; max-width: 45%; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: 11px; color: var(--muted); }
  .mono { font-family: var(--mono); }

  .menu { width: 34px; height: 34px; flex: none; border-radius: 50%; border: 1px solid var(--line); background: var(--surface); color: var(--ink); display: flex; align-items: center; justify-content: center; padding: 0; }
  .menu svg { width: 18px; height: 18px; stroke-width: 1.7; }
  svg { fill: none; stroke: currentColor; stroke-linecap: round; stroke-linejoin: round; }

  .win { display: flex; gap: 6px; padding-left: 10px; border-left: 1px solid var(--line); }
  .win button { width: 26px; height: 26px; border-radius: 50%; border: 0; background: var(--ink); color: var(--inv); display: flex; align-items: center; justify-content: center; padding: 0; }
  /* Dark: quiet graphite circles; three white dots in the corner pulled the eye from the page. */
  :global(:root[data-theme='dark']) .win button { background: var(--side); color: var(--ink); }
  .win svg { width: 12px; height: 12px; stroke-width: 1.6; }
</style>
