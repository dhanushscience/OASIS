<script>
  import { models, modelsCall, etaText, setName } from '../models.svelte.js';
  import { restart, restartNow } from '../restart.svelte.js';

  /** @type {{ ondetails: () => void }} */
  let { ondetails } = $props();

  const info = $derived(models.info);
  // "Voice is ready" shows for a few seconds after a download, then the strip goes away.
  let justReady = $state(false);
  let last = '';
  $effect(() => {
    const phase = info?.phase ?? '';
    if (phase === 'ready' && ['downloading', 'paused', 'offline', 'metered', 'failed', 'testing', 'cleaning'].includes(last)) {
      justReady = true;
      setTimeout(() => (justReady = false), 4000);
    }
    last = phase;
  });

  const percent = $derived(info && info.total_mb ? Math.min(100, Math.round((info.done_mb / info.total_mb) * 100)) : 0);
  const size = $derived(info ? `${info.done_mb} of ${info.total_mb} MB` : '');
  const show = $derived(!!info && (['downloading', 'paused', 'offline', 'metered', 'failed', 'testing', 'cleaning'].includes(info.phase) || justReady));
</script>

<!-- A restart waiting for a quiet moment (section 5): it happens by itself when nothing is open or unsaved. -->
{#if restart.reason && !restart.later}
  <div class="strip" role="status" aria-live="polite">
    <span class="bar"></span>
    <span class="text">{restart.reason}. Restart OASIS to finish. It restarts by itself when nothing is open or unsaved.</span>
    <button onclick={restartNow}>Restart now</button>
    <button class="ghost" onclick={() => (restart.later = true)}>Later</button>
  </div>
{/if}
{#if restart.error}
  <div class="strip" role="alert"><span class="bar"></span><span class="text">{restart.error}</span><button class="ghost" onclick={() => (restart.error = null)}>Close</button></div>
{/if}

<!-- On every screen while voice models download or wait (VOICE_MODEL_SETUP.md section 3). -->
{#if show && info}
  <div class="strip" role="status" aria-live="polite">
    <span class="bar"></span>
    <span class="text">
      {#if info.phase === 'downloading' && info.voice_ready}
        Voice works now with the small set. Downloading the full set ({setName(info.target)}) · {size}{info.eta_s !== null ? ` · ${etaText(info.eta_s)}` : ''}
      {:else if info.phase === 'downloading'}
        Downloading voice models ({setName(info.target)}) · {size}{info.eta_s !== null ? ` · ${etaText(info.eta_s)}` : ''}
      {:else if info.phase === 'paused'}
        Voice model download paused · {size}
      {:else if info.phase === 'metered'}
        Waiting: you are on a metered connection, and the voice models are a large download.
      {:else if info.phase === 'offline'}
        Waiting for a connection to download the voice models. OASIS tries again every few minutes.
      {:else if info.phase === 'testing'}
        Checking the voice models on this PC… about 20 seconds.
      {:else if info.phase === 'cleaning'}
        Clearing old voice models before the update…
      {:else if info.phase === 'failed'}
        {info.error ?? 'The voice model download failed.'}
      {:else}
        Voice is ready.
      {/if}
    </span>
    {#if info.phase === 'downloading'}
      <span class="meter" aria-hidden="true"><span style="width:{percent}%"></span></span>
      <button onclick={() => modelsCall('models_pause')}>Pause</button>
    {:else if info.phase === 'paused'}
      <button onclick={() => modelsCall('models_resume')}>Resume</button>
    {:else if info.phase === 'metered'}
      <button onclick={() => modelsCall('models_download_anyway')}>Download anyway</button>
    {:else if info.phase === 'offline' || info.phase === 'failed'}
      <button onclick={() => modelsCall('models_resume')}>Try again</button>
    {/if}
    {#if info.phase !== 'ready'}<button class="ghost" onclick={ondetails}>Details</button>{/if}
  </div>
{/if}

<style>
  .strip { display: flex; align-items: center; gap: 12px; flex: none; background: var(--ink); color: var(--inv); border-radius: var(--r-row); padding: 6px 8px 6px 0; font-size: 13px; overflow: hidden; min-height: 40px; }
  .bar { align-self: stretch; width: 4px; background: var(--signal); flex: none; }
  .text { flex: 1; min-width: 0; line-height: 1.4; }
  .meter { flex: 0 1 160px; height: 6px; border-radius: 3px; background: color-mix(in srgb, var(--inv) 22%, transparent); overflow: hidden; }
  .meter span { display: block; height: 100%; background: var(--signal); transition: width .25s var(--ease); }
  button { flex: none; height: 28px; padding: 0 14px; border: 0; border-radius: var(--r-pill); background: var(--inv); color: var(--ink); font: 600 12px/1 var(--mono); }
  .ghost { background: transparent; color: var(--inv); border: 1.5px solid color-mix(in srgb, var(--inv) 40%, transparent); }
  @media (prefers-reduced-motion: reduce) { .meter span { transition: none; } }
</style>
