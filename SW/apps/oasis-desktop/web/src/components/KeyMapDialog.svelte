<script>
  import { data, saveInput } from '../profiles.svelte.js';
  import { sameChord } from '../actionKinds.js';
  import { HUES, paint } from '../hues.js';
  import KeyBox from './KeyBox.svelte';

  /** The three keys in one place: each row is drawn in its button's colour, and clicking a row listens for the new keys. */
  /** @type {{ onclose: () => void }} */
  let { onclose } = $props();

  let clash = $state('');
  const parts = (/** @type {string} */ k) => (k || '').split('+').map((s) => s.trim()).filter(Boolean);

  // Same rule as Settings › Input: two buttons cannot share a key. Saves at once.
  async function setKey(/** @type {number} */ i, /** @type {string} */ value) {
    const other = value ? data.input.keys.findIndex((k, j) => j !== i && sameChord(k, value)) : -1;
    if (other >= 0) { clash = `${value} is already the key for Button ${other + 1}. Clear that one first.`; return; }
    clash = '';
    try { await saveInput({ ...$state.snapshot(data.input), keys: data.input.keys.map((k, j) => (j === i ? value : k)) }); }
    catch (e) { clash = String(e); }
  }
</script>

<svelte:window onkeydown={(e) => { if (e.key === 'Escape') onclose(); }} />

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="scrim" onclick={onclose}>
  <div class="pop" role="dialog" tabindex="-1" aria-modal="true" aria-label="Key Mapping" onclick={(e) => e.stopPropagation()}>
    <div class="top">
      <div><span class="lab">Key Mapping</span><h3>Keys for your buttons</h3></div>
      <button class="x" aria-label="Close" onclick={onclose}><svg viewBox="0 0 24 24"><path d="M6 6l12 12M18 6 6 18"/></svg></button>
    </div>

    <div class="rows">
      {#each [0, 1, 2] as i}
        {@const ps = parts(data.input.keys[i])}
        <div class="row" class:cur={i === data.button} style="--h:{HUES[i].c}">
          <span class="badge" role="img" aria-label="Button {i + 1}">{i + 1}</span>
          <div class="what">
            <span class="caps">
              {#each ps as part, j}
                {#if j}<span class="plus" aria-hidden="true">+</span>{/if}
                <span class="cap mono" style={paint(i, j === ps.length - 1)}>{part}</span>
              {:else}<span class="none">No key set</span>{/each}
            </span>
          </div>
          <span class="act"><span class="idle">Change</span><span class="live">Press the keys…</span></span>
          <KeyBox bare autofocus={i === data.button} label="Button {i + 1}" value={data.input.keys[i] || ''} onchange={(v) => setKey(i, v)} />
        </div>
      {/each}
    </div>

    {#if clash}<p class="problem" role="alert">{clash}</p>{/if}
    <div class="foot">
      <span>Click a button, then press its new keys. Backspace clears. Saved as you press.</span>
      <button class="done" onclick={onclose}>Done</button>
    </div>
  </div>
</div>

<style>
  .scrim { position: fixed; inset: 0; z-index: 50; display: grid; place-items: center; padding: 16px; background: rgba(0, 0, 0, .35); animation: fade .2s var(--ease); }
  .pop { width: min(400px, 100%); max-height: 100%; overflow: auto; display: flex; flex-direction: column; gap: 16px; padding: 22px; border-radius: var(--r-bar); background: var(--surface); box-shadow: 0 12px 40px var(--shadow); outline: none; }
  .top { display: flex; justify-content: space-between; align-items: flex-start; gap: 12px; }
  .lab { font: 500 11px/1.3 var(--mono); letter-spacing: .12em; text-transform: uppercase; color: var(--muted); }
  h3 { margin: 4px 0 0; font-size: 20px; font-weight: 700; letter-spacing: -.01em; }
  .x { flex: none; width: 32px; height: 32px; display: grid; place-items: center; border: 0; border-radius: 50%; background: var(--hair); color: var(--ink); }
  .x svg { width: 16px; height: 16px; fill: none; stroke: currentColor; stroke-width: 2; stroke-linecap: round; }
  .rows { display: flex; flex-direction: column; gap: 10px; }
  .row { position: relative; display: flex; align-items: center; gap: 14px; padding: 12px 16px 12px 12px; border-radius: var(--r-row); border: 1.5px solid var(--line); background: var(--surface); cursor: pointer; transition: border-color .2s var(--ease), background-color .2s var(--ease), box-shadow .2s var(--ease); }
  .row:hover { border-color: var(--h); }
  .row.cur { background: color-mix(in srgb, var(--h) 7%, var(--surface)); }
  .row:focus-within { border-color: var(--h); box-shadow: 0 0 0 4px color-mix(in srgb, var(--h) 22%, transparent); animation: glow 1.4s ease-in-out infinite; }
  @keyframes glow { 50% { box-shadow: 0 0 0 7px color-mix(in srgb, var(--h) 8%, transparent); } }
  .badge { flex: none; width: 34px; height: 34px; display: grid; place-items: center; border-radius: 50%; background: var(--h); color: #fff; font: 700 15px var(--mono); }
  .what { flex: 1; min-width: 0; }
  .caps { display: flex; flex-wrap: wrap; align-items: center; gap: 6px 8px; }
  .cap { min-width: 40px; height: 34px; padding: 0 12px; display: inline-flex; align-items: center; justify-content: center; border-radius: 8px; font-size: 15px; font-weight: 600; color: #fff; background: var(--c); box-shadow: 0 3px 0 var(--e); margin-bottom: 3px; }
  .mono { font-family: var(--mono); }
  .plus { color: var(--muted); font-size: 14px; }
  .none { font-size: 13px; color: var(--muted); }
  .act { flex: none; font-size: 12px; font-weight: 600; color: var(--muted); }
  .live { display: none; color: var(--h); }
  .row:focus-within .idle { display: none; }
  .row:focus-within .live { display: inline; }
  .problem { margin: 0; font-size: 12px; color: var(--bad); }
  .foot { display: flex; align-items: center; justify-content: space-between; gap: 14px; font-size: 12px; line-height: 1.45; color: var(--muted); }
  .done { flex: none; height: 40px; padding: 0 22px; border: 0; border-radius: var(--r-pill); background: var(--ink); color: var(--inv); font-size: 14px; font-weight: 600; }
  @keyframes fade { from { opacity: 0; } }
  @media (prefers-reduced-motion: reduce) { .row:focus-within { animation: none; } }
</style>
