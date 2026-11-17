<script>
  // A small panel under its trigger. Esc or a click outside closes it and puts focus back.
  /** @type {{ label: string, align?: 'left' | 'right', trigger: import('svelte').Snippet<[{ open: boolean, toggle: () => void }]>, children: import('svelte').Snippet<[{ close: () => void }]> }} */
  let { label, align = 'left', trigger, children } = $props();

  let open = $state(false);
  /** @type {HTMLElement | undefined} */
  let wrap = $state();

  const toggle = () => (open = !open);
  function close() {
    if (!open) return;
    open = false;
    /** @type {HTMLElement | null | undefined} */ (wrap?.querySelector('[data-trigger]'))?.focus();
  }

  /** @param {PointerEvent} e */
  function outside(e) {
    if (open && wrap && !wrap.contains(/** @type {Node} */ (e.target))) open = false;
  }

  // Focus moves into the panel when it opens, to its first field or button.
  /** @param {HTMLElement} node */
  function focusFirst(node) {
    /** @type {HTMLElement | null} */ (node.querySelector('input, select, textarea, button'))?.focus();
  }
</script>

<svelte:window onpointerdown={outside} onkeydown={(e) => open && e.key === 'Escape' && close()} />

<span class="wrap" bind:this={wrap}>
  <span data-trigger-host>{@render trigger({ open, toggle })}</span>
  {#if open}
    <div class="pop" class:right={align === 'right'} role="dialog" aria-label={label} use:focusFirst>
      {@render children({ close })}
    </div>
  {/if}
</span>

<style>
  .wrap { position: relative; display: inline-flex; }
  .pop { position: absolute; top: calc(100% + 8px); left: 0; z-index: 20; min-width: 280px; max-width: 340px; background: var(--surface); border: 1px solid var(--line); border-radius: var(--r-bar); padding: 14px; display: flex; flex-direction: column; gap: 10px; box-shadow: 0 8px 28px var(--shadow); animation: rise .2s var(--ease); }
  .pop.right { left: auto; right: 0; }
  @keyframes rise { from { opacity: 0; transform: translateY(6px); } }
</style>
