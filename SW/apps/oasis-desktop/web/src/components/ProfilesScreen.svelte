<script>
  import { onMount } from 'svelte';
  import { data, load, current, currentConfig, order, jump } from '../profiles.svelte.js';
  import { status } from '../status.svelte.js';
  import { loadApps } from '../apps.svelte.js';
  import { pad } from '../pad.svelte.js';
  import ProfileStrip from './ProfileStrip.svelte';
  import ProfileHeader from './ProfileHeader.svelte';
  import PadView from './PadView.svelte';
  import ConfigurePanel from './ConfigurePanel.svelte';
  import DuplicateDialog from './DuplicateDialog.svelte';

  /** @type {{ active: boolean }} */
  let { active } = $props();

  const p = $derived(current());
  const cfg = $derived(currentConfig());
  const buttons = $derived(cfg?.buttons ?? []);
  const waiting = $derived(!!status.engine && status.engine.pad !== 'Connected');

  // The first time, open on the app you were just in (or the one used most); then leave the choice alone.
  let started = false;
  $effect(() => {
    if (started || !data.loaded || !status.loaded) return;
    started = true;
    const front = status.engine?.front_profile;
    const first = front != null && data.profiles[front] ? front : order()[0];
    if (first != null) jump(first);
  });

  // Coming back to the screen, or a change from outside, reads the file again.
  $effect(() => { if (active) load(); });
  onMount(() => {
    load(false);
    loadApps(); // slow the first time, so it starts now
    const off = window.__TAURI__?.event.listen('engine-event', (/** @type {{ payload: { type: string } }} */ e) => {
      if (e.payload.type === 'settings_changed') load();
    });
    return () => off?.then((/** @type {() => void} */ f) => f());
  });

</script>

<div class="screen">
  <div class="centre">
    <ProfileStrip />
    {#if data.error}
      <div class="problem" role="alert">The profiles could not be read: {data.error}</div>
    {:else if p && cfg}
      <ProfileHeader>
        <div class="padwrap" inert={data.settings}>
          <PadView name={p.name} {buttons} selected={Math.min(data.button, buttons.length - 1)} out={data.out} dir={data.dir} {waiting} flash={pad.lit.indexOf(true)}
            onpick={(i) => (data.button = i)} />
        </div>
      </ProfileHeader>
    {:else if data.loaded}
      <div class="empty">
        <div class="lab">No profiles yet</div>
        <p>Use the + above to make the first one, or open an app and press “Make profile”.</p>
      </div>
    {/if}
  </div>
  {#if p && cfg && buttons.length}<ConfigurePanel />{/if}
  <DuplicateDialog />
</div>

<style>
  .screen { flex: 1; min-width: 0; min-height: 0; display: flex; gap: 18px; }
  .centre { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 12px; }
  .padwrap { flex: 1; min-width: 0; display: flex; }
  .padwrap > :global(.stage) { flex: 1; }
  .problem { color: var(--bad); font-size: 13px; }
  .empty { padding: 28px 0; }
  .empty p { color: var(--muted); font-size: 15px; margin: 8px 0 0; max-width: 46ch; }
  .lab { font: 500 11px/1.3 var(--mono); letter-spacing: .12em; text-transform: uppercase; color: var(--muted); }
</style>
