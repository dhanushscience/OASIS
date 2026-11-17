<script>
  import { trackUnsaved } from '../busy.svelte.js';
  /** An AutoHotkey script, written or pasted in a sheet that opens on the left, beside the Configure panel. */
  /** @type {{ value: string, step: number, onsave: (script: string) => void, onclose: () => void }} */
  let { value, step, onsave, onclose } = $props();

  // svelte-ignore state_referenced_locally
  let text = $state(value); // a draft: starts from the saved script and is only kept on Save
  const lines = $derived(text ? text.split('\n').length : 0);
  const changed = $derived(text !== value);
  trackUnsaved('script', () => changed);
  const TOO_LONG = 64 * 1024;
</script>

<svelte:window onkeydown={(e) => { if (e.key === 'Escape') onclose(); if (e.key === 's' && e.ctrlKey && text.trim() && text.length <= TOO_LONG) { e.preventDefault(); onsave(text); } }} />

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="scrim" onclick={onclose}></div>
<div class="sheet" role="dialog" tabindex="-1" aria-modal="true" aria-label="AutoHotkey script for step {step}">
  <div class="top">
    <div><span class="lab">Step {step} · AutoHotkey</span><h3>Script</h3></div>
    <button class="x" aria-label="Close" onclick={onclose}><svg viewBox="0 0 24 24"><path d="M6 6l12 12M18 6 6 18"/></svg></button>
  </div>

  <!-- svelte-ignore a11y_autofocus -->
  <textarea class="code" bind:value={text} autofocus spellcheck="false" autocomplete="off" wrap="off"
    placeholder={'; AutoHotkey v2. For example:\nSend "^c"\nSleep 200\nRun "notepad.exe"'}></textarea>

  <div class="meta mono"><span>{lines} {lines === 1 ? 'line' : 'lines'}</span>{#if changed}<span class="dirty">edited</span>{/if}</div>

  <ul class="rules">
    <li>Written for <b>AutoHotkey v2</b>, installed for all users (the normal installer).</li>
    <li>Runs only when <b>Allow scripts</b> is on in Settings. Never from voice or the browser.</li>
    <li>Stops after one minute, so a script left running does not stay hidden.</li>
  </ul>

  {#if text.length > TOO_LONG}<p class="problem" role="alert">The script is too long (64 KB at most).</p>{/if}
  <div class="foot">
    <button class="ghost" onclick={onclose}>Cancel</button>
    <button class="save" disabled={!text.trim() || text.length > TOO_LONG} onclick={() => onsave(text)}>Save script</button>
  </div>
</div>

<style>
  .scrim { position: fixed; inset: 0; z-index: 50; background: rgba(0, 0, 0, .25); animation: fade .2s var(--ease); }
  .sheet { position: fixed; z-index: 51; top: 16px; bottom: 16px; left: 16px; width: min(560px, calc(100vw - 32px)); display: flex; flex-direction: column; gap: 14px; padding: 22px; border-radius: var(--r-bar); background: var(--surface); box-shadow: 0 12px 40px var(--shadow); outline: none; animation: slide .26s var(--ease); }
  .top { display: flex; justify-content: space-between; align-items: flex-start; gap: 12px; }
  .lab { font: 500 11px/1.3 var(--mono); letter-spacing: .12em; text-transform: uppercase; color: var(--muted); }
  h3 { margin: 4px 0 0; font-size: 20px; font-weight: 700; letter-spacing: -.01em; }
  .x { flex: none; width: 32px; height: 32px; display: grid; place-items: center; border: 0; border-radius: 50%; background: var(--hair); color: var(--ink); }
  .x svg { width: 16px; height: 16px; fill: none; stroke: currentColor; stroke-width: 2; stroke-linecap: round; }
  .code { flex: 1; min-height: 160px; resize: none; padding: 14px 16px; border-radius: var(--r-row); border: 1.5px solid var(--line); background: var(--sunk, var(--surface)); color: var(--ink); font: 13px/1.6 var(--mono); tab-size: 4; outline: none; }
  .code:focus-visible { border-color: var(--ink); }
  .meta { display: flex; gap: 10px; margin-top: -6px; font-size: 11px; color: var(--muted); }
  .dirty { color: var(--signal); }
  .mono { font-family: var(--mono); }
  .rules { margin: 0; padding: 12px 14px 12px 30px; border-radius: var(--r-row); background: var(--hair); display: flex; flex-direction: column; gap: 4px; font-size: 12px; line-height: 1.45; color: var(--muted); }
  .rules b { color: var(--ink); font-weight: 600; }
  .problem { margin: 0; font-size: 12px; color: var(--bad); }
  .foot { display: flex; gap: 10px; }
  .foot button { flex: 1; height: 46px; border-radius: var(--r-bar); font-size: 15px; font-weight: 600; }
  .ghost { border: 1.5px solid var(--line); background: transparent; color: var(--muted); }
  .save { border: 0; background: var(--ink); color: var(--inv); }
  .save:disabled { opacity: .4; cursor: default; }
  @keyframes fade { from { opacity: 0; } }
  @keyframes slide { from { opacity: 0; transform: translateX(-24px); } }
  @media (prefers-reduced-motion: reduce) { .sheet, .scrim { animation: none; } }
</style>
