<script>
  import { onMount, tick } from 'svelte';
  import { guide } from '../guide/onboarding.svelte.js';
  import { shouldPrompt } from '../updatePrompt.js';

  // The pop-up when a newer OASIS is out. It installs only on "Update now"; "Later" waits a day for this version.
  const tauri = /** @type {any} */ (window).__TAURI__;
  const KEY = 'oasis.update.seen';
  /** @type {{ current: string, available: string | null, notes: string | null, error: string | null, progress: number | null }} */
  let info = $state({ current: '', available: null, notes: null, error: null, progress: null });
  let open = $state(false);
  let installing = $state(false);
  /** @type {HTMLButtonElement} */
  let goBtn;

  const seen = () => { try { return JSON.parse(localStorage.getItem(KEY) ?? 'null'); } catch { return null; } };

  onMount(() => {
    if (!tauri) return;
    tauri.core.invoke('update_status').then((/** @type {any} */ i) => (info = i)).catch(() => {});
    const off = tauri.event.listen('update-status', (/** @type {any} */ e) => (info = e.payload));
    return () => off.then((/** @type {() => void} */ f) => f());
  });

  $effect(() => {
    if (open || !shouldPrompt(info, guide, seen(), Date.now())) return;
    try { localStorage.setItem(KEY, JSON.stringify({ version: info.available, at: Date.now() })); } catch { /* shows again next check */ }
    open = true;
    tick().then(() => goBtn?.focus());
  });

  const later = () => { if (!installing) open = false; };
  async function update() {
    installing = true;
    // On success the installer closes OASIS; a failure comes back in `info.error`.
    await tauri?.core.invoke('install_update').catch(() => {});
    installing = false;
  }

  function esc(/** @type {KeyboardEvent} */ e) {
    if (!open || e.key !== 'Escape') return;
    e.preventDefault(); e.stopPropagation(); later();
  }
</script>

<svelte:window onkeydowncapture={esc} />

<div class="scrim" class:open inert={!open} role="presentation" onclick={(e) => e.target === e.currentTarget && later()}>
  <div class="box" role="dialog" aria-modal="true" aria-labelledby="update-title" aria-describedby="update-body">
    <h2 id="update-title">OASIS {info.available} is available</h2>
    <p id="update-body">
      {#if info.progress !== null}Downloading… {info.progress}%. OASIS restarts when it is installed.
      {:else if installing}Starting the download…
      {:else if info.error}The update did not install: {info.error}
      {:else}You have {info.current}. It takes a minute and OASIS restarts on its own.{/if}
    </p>
    {#if info.notes && !installing}<div class="notes">{info.notes}</div>{/if}
    {#if info.progress !== null}<div class="meter" role="progressbar" aria-label="Update download" aria-valuemin="0" aria-valuemax="100" aria-valuenow={info.progress}><span style="width:{info.progress}%"></span></div>{/if}
    <div class="btns">
      <button class="later" onclick={later} disabled={installing}>Later</button>
      <button bind:this={goBtn} class="go" onclick={update} disabled={installing}>Update now</button>
    </div>
  </div>
</div>

<style>
  .scrim { position: fixed; inset: 0; z-index: 190; display: flex; align-items: center; justify-content: center; padding: 24px; background: rgb(20 20 24 / .45);
    opacity: 0; visibility: hidden; transition: opacity .18s var(--ease), visibility 0s linear .18s; }
  .scrim.open { opacity: 1; visibility: visible; transition-delay: 0s; }
  .box { width: min(440px, 100%); padding: 24px; background: var(--surface); border: 1px solid var(--line); border-radius: var(--r-panel); box-shadow: 0 24px 60px var(--shadow);
    transform: translateY(8px) scale(.98); transition: transform .22s var(--ease); }
  .scrim.open .box { transform: none; }
  h2 { margin: 0 0 8px; font-size: 18px; font-weight: 700; letter-spacing: -.02em; }
  p { margin: 0 0 14px; font-size: 14px; line-height: 1.5; color: var(--muted); }
  .notes { max-height: 40vh; overflow: auto; margin: 0 0 18px; font-size: 13px; line-height: 1.5; white-space: pre-line; color: var(--ink); }
  .meter { height: 6px; margin: 0 0 18px; border-radius: 3px; background: var(--hair); overflow: hidden; }
  .meter span { display: block; height: 100%; background: var(--signal); transition: width .25s var(--ease); }
  .btns { display: flex; justify-content: flex-end; gap: 10px; }
  button { height: 38px; padding: 0 18px; border-radius: var(--r-pill); font-size: 13px; font-weight: 600; }
  button:disabled { opacity: .5; }
  .later { border: 1px solid var(--line); background: transparent; color: var(--ink); }
  .go { border: 0; background: var(--signal); color: var(--on-signal); }
  button:focus-visible { outline: 2px solid var(--signal); outline-offset: 2px; }
</style>
