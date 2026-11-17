<script>
  import { installed, running, loadRunning, loadApps } from '../apps.svelte.js';

  // An app's name, typed or picked: what is open now first, then the installed apps. The list is the
  // same rounded one the other drop-downs use.
  /** @type {{ value: string, placeholder?: string, label?: string, disabled?: boolean, maxlength?: number }} */
  let { value = $bindable(''), placeholder = 'An app, for example code', label = 'App', disabled = false, maxlength = undefined } = $props();

  let focused = $state(false);
  let active = $state(0);
  let box = $state('');
  /** @type {HTMLInputElement | undefined} */
  let input = $state();

  const items = $derived.by(() => {
    if (!focused) return [];
    const q = value.trim().toLowerCase();
    const open = running.list.filter((r) => !q || r.name.toLowerCase().includes(q) || r.title.toLowerCase().includes(q)).slice(0, 6).map((r) => ({ name: r.name, note: 'Open now' }));
    const have = new Set(open.map((o) => o.name.toLowerCase()));
    const rest = /** @type {string[]} */ (installed.names).filter((n) => q && n.toLowerCase().includes(q) && !have.has(n.toLowerCase())).slice(0, 8).map((n) => ({ name: n, note: '' }));
    return [...open, ...rest];
  });
  $effect(() => { void value; active = 0; });
  $effect(() => {
    if (!items.length || !input) return;
    const r = input.getBoundingClientRect(); // fixed, so the card's clipping cannot cut the list
    box = `left:${r.left}px;top:${r.bottom + 6}px;width:${Math.max(r.width, 220)}px`;
  });

  /** @param {{ name: string }} item */
  function pick(item) { value = item.name; focused = false; }
  /** @param {KeyboardEvent} e */
  function key(e) {
    if (!items.length) return;
    if (e.key === 'ArrowDown') { e.preventDefault(); active = (active + 1) % items.length; }
    else if (e.key === 'ArrowUp') { e.preventDefault(); active = (active - 1 + items.length) % items.length; }
    else if (e.key === 'Enter') { e.preventDefault(); pick(items[active]); }
    else if (e.key === 'Escape') { e.stopPropagation(); focused = false; }
  }
</script>

<input bind:this={input} bind:value {placeholder} {disabled} {maxlength} aria-label={label} autocomplete="off" spellcheck="false" role="combobox" aria-expanded={items.length > 0} aria-controls="app-list"
  onfocus={() => { focused = true; loadRunning(); loadApps(); }} onblur={() => (focused = false)} onkeydown={key} />
{#if items.length}
  <ul id="app-list" class="app-list" role="listbox" aria-label="Apps" style={box}>
    {#each items as item, i}
      <!-- mousedown, not click: the box must not lose focus (and close the list) first -->
      <li role="option" aria-selected={i === active} class:on={i === active} onmousedown={(e) => { e.preventDefault(); pick(item); }} onmouseenter={() => (active = i)}>
        <span>{item.name}</span><span class="note">{item.note}</span>
      </li>
    {/each}
  </ul>
{/if}

<style>
  input { height: 36px; min-width: 0; border-radius: var(--r-field); border: 1.5px solid var(--line); background: var(--surface); color: var(--ink); padding: 0 12px; font-size: 13px; outline: none; }
  input:focus-visible { border-color: var(--ink); }
  .app-list { position: fixed; z-index: 60; margin: 0; padding: 4px; list-style: none; background: var(--surface); border: 1px solid var(--line); border-radius: var(--r-row); box-shadow: 0 8px 28px var(--shadow); max-height: 240px; overflow: auto; }
  .app-list li { display: flex; align-items: center; justify-content: space-between; gap: 10px; padding: 8px 10px; border-radius: var(--r-field); font-size: 13px; cursor: pointer; }
  .app-list li.on { background: var(--sunk); }
  .note { font: 400 10px/1 var(--mono); color: var(--muted); text-transform: uppercase; letter-spacing: .08em; }
</style>
