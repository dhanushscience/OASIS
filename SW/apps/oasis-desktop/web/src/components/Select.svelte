<script>
  // A drop-down with the rounded list the action field uses (the browser's own list cannot be rounded).
  /** @type {{ value: any, options: { value: any, label: string }[], label?: string, disabled?: boolean, onchange?: (value: any) => void }} */
  let { value = $bindable(''), options, label = '', disabled = false, onchange } = $props();

  let open = $state(false);
  let active = $state(0);
  let box = $state('');
  /** @type {HTMLButtonElement | undefined} */
  let btn = $state();

  const shown = $derived(options.find((o) => o.value === value)?.label ?? '');

  function show() {
    if (disabled || !btn) return;
    const r = btn.getBoundingClientRect(); // fixed, so the panel's clipping cannot cut the list
    box = `left:${r.left}px;top:${r.bottom + 6}px;width:${r.width}px`;
    active = Math.max(0, options.findIndex((o) => o.value === value));
    open = true;
  }
  /** @param {any} v */
  function pick(v) { const changed = v !== value; value = v; open = false; btn?.focus(); if (changed) onchange?.(v); }
  /** @param {KeyboardEvent} e */
  function key(e) {
    if (!open) { if (e.key === 'ArrowDown' || e.key === 'ArrowUp') { e.preventDefault(); show(); } return; }
    if (e.key === 'ArrowDown') { e.preventDefault(); active = (active + 1) % options.length; }
    else if (e.key === 'ArrowUp') { e.preventDefault(); active = (active - 1 + options.length) % options.length; }
    else if (e.key === 'Enter') { e.preventDefault(); pick(options[active].value); }
    else if (e.key === 'Escape') { e.stopPropagation(); open = false; }
  }
</script>

<svelte:window onpointerdown={(e) => open && !btn?.contains(/** @type {Node} */ (e.target)) && !(/** @type {Element} */ (e.target)).closest?.('.sel-list') && (open = false)} />

<button type="button" class="sel" bind:this={btn} {disabled} aria-label={label || undefined} aria-haspopup="listbox" aria-expanded={open}
  onclick={() => (open ? (open = false) : show())} onkeydown={key}>
  <span>{shown}</span>
  <svg viewBox="0 0 24 24" aria-hidden="true"><path d="m6 9 6 6 6-6" /></svg>
</button>
{#if open}
  <ul class="sel-list" role="listbox" aria-label={label} style={box}>
    {#each options as o, i}
      <li role="option" aria-selected={o.value === value} class:on={i === active} onmousedown={(e) => { e.preventDefault(); pick(o.value); }} onmouseenter={() => (active = i)}>{o.label}</li>
    {/each}
  </ul>
{/if}

<style>
  .sel { height: 36px; display: flex; align-items: center; justify-content: space-between; gap: 8px; border-radius: var(--r-field); border: 1.5px solid var(--line); background: var(--surface); color: var(--ink); padding: 0 12px; font-size: 13px; text-align: left; }
  .sel:focus-visible, .sel[aria-expanded='true'] { border-color: var(--ink); outline: none; }
  .sel svg { width: 14px; height: 14px; flex: none; fill: none; stroke: currentColor; stroke-width: 2; stroke-linecap: round; stroke-linejoin: round; }
  .sel:disabled { opacity: .4; }
  .sel-list { position: fixed; z-index: 60; margin: 0; padding: 4px; list-style: none; background: var(--surface); border: 1px solid var(--line); border-radius: var(--r-row); box-shadow: 0 8px 28px var(--shadow); max-height: 260px; overflow: auto; }
  .sel-list li { padding: 8px 10px; border-radius: var(--r-field); font-size: 13px; cursor: pointer; }
  .sel-list li.on { background: var(--sunk); }
  .sel-list li[aria-selected='true'] { font-weight: 600; }
</style>
