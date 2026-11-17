<script>
  import { data, editExisting } from '../profiles.svelte.js';

  const found = $derived(data.duplicate);
  const cancel = () => (data.duplicate = null);
</script>

<svelte:window onkeydown={(e) => found && e.key === 'Escape' && cancel()} />

{#if found}
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions: Esc cancels (below); a click outside is a convenience -->
  <div class="scrim" role="presentation" onclick={cancel}>
    <div class="card" role="alertdialog" aria-modal="true" aria-labelledby="dup-title" tabindex="-1" onclick={(e) => e.stopPropagation()}>
      <h3 id="dup-title">A profile for this already exists</h3>
      <p>“{found.name}” already applies to <strong>{found.target}</strong>. Do you want to modify it instead?</p>
      <div class="row">
        <button class="quiet" onclick={cancel}>Cancel</button>
        <!-- svelte-ignore a11y_autofocus -->
        <button class="primary" autofocus onclick={() => editExisting(found.index)}>Modify it</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .scrim { position: fixed; inset: 0; z-index: 100; display: flex; align-items: center; justify-content: center; background: rgb(20 20 24 / .35); animation: in .2s var(--ease); }
  .card { width: min(380px, calc(100vw - 48px)); background: var(--surface); border: 1px solid var(--line); border-radius: var(--r-panel); padding: 22px; box-shadow: 0 18px 50px var(--shadow); display: flex; flex-direction: column; gap: 10px; animation: rise .24s var(--ease); }
  h3 { margin: 0; font-size: 17px; font-weight: 700; letter-spacing: -.01em; }
  p { margin: 0; font-size: 14px; line-height: 1.45; color: var(--muted); }
  p strong { color: var(--ink); }
  .row { display: flex; justify-content: flex-end; gap: 8px; margin-top: 8px; }
  button { height: 38px; padding: 0 18px; border-radius: var(--r-pill); font-size: 13px; font-weight: 600; }
  .quiet { border: 1px solid var(--line); background: transparent; color: var(--ink); }
  .primary { border: 0; background: var(--ink); color: var(--inv); }
  @keyframes in { from { opacity: 0; } }
  @keyframes rise { from { opacity: 0; transform: translateY(8px); } }
</style>
