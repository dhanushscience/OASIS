<script>
  import { models, modelsCall, etaText, setName, upgradeText } from '../models.svelte.js';
  import { sure } from '../confirm.svelte.js';

  const info = $derived(models.info);
  const percent = $derived(info && info.total_mb ? Math.min(100, Math.round((info.done_mb / info.total_mb) * 100)) : 0);
  const busy = $derived(info?.phase === 'downloading');

  const line = $derived.by(() => {
    if (!info) return 'Checking this PC…';
    const set = setName(info.target);
    switch (info.phase) {
      case 'checking': return 'Checking this PC…';
      case 'downloading': return `Downloading ${set}: ${info.done_mb} of ${info.total_mb} MB${info.eta_s !== null ? `, ${etaText(info.eta_s)}` : ''}.`;
      case 'paused': return `${set} is paused at ${info.done_mb} of ${info.total_mb} MB.`;
      case 'metered': return `${set} waits: this is a metered connection. It downloads on an ordinary one, or now if you choose.`;
      case 'offline': return `${set} waits for a connection${info.error ? ` (${info.error})` : ''}. OASIS tries again every few minutes.`;
      case 'failed': return info.error ?? 'The download failed.';
      case 'cleaning': return 'Clearing old voice models before the update…';
      case 'testing': return `Testing ${set} on this PC (about 20 seconds)…`;
      case 'ready': return `${set} is in use.`;
      default: return info.choice === 'none' ? 'No voice models are downloaded. Choose a set to download one.' : 'No set fits this PC. Choose one below, or use Groq (online) in Voice settings.';
    }
  });

  /** @param {string} choice */
  function choose(choice) {
    if (info && choice !== info.choice) modelsCall('models_choose', { choice });
  }
  async function remove() {
    if (await sure({ title: 'Remove downloaded models?', body: 'Voice stops until a set is downloaded again.', yes: 'Remove', keep: 'Keep them' })) modelsCall('models_remove');
  }
</script>

<section class="group" id="voice-models" aria-labelledby="models-head">
  <div class="ghead">
    <span class="lab" id="models-head">Status</span>
    {#if busy}
      <button class="pillbtn" onclick={() => modelsCall('models_pause')}>Pause</button>
    {:else if info?.phase === 'paused' || info?.phase === 'offline' || info?.phase === 'failed'}
      <button class="pillbtn dark" onclick={() => modelsCall('models_resume')}>{info.phase === 'paused' ? 'Resume' : 'Try again'}</button>
    {:else if info?.phase === 'metered'}
      <button class="pillbtn dark" onclick={() => modelsCall('models_download_anyway')}>Download anyway</button>
    {/if}
  </div>
  <p class="gh" role="status">{line}</p>
  {#if busy || info?.phase === 'paused'}
    <div class="meter" role="progressbar" aria-label="Voice model download" aria-valuemin="0" aria-valuemax="100" aria-valuenow={percent}><span style="width:{percent}%"></span></div>
  {/if}
  {#if info?.upgrade}
    <!-- Basic in use on a PC that can run Balanced: one click switches, the same way as choosing Balanced below -->
    <div class="srow">
      <div class="rtext"><div class="rname">Basic makes more mistakes</div>
        <div class="hint">This PC can run Balanced, which gets about five times fewer words wrong.</div></div>
      <button class="pillbtn dark" onclick={() => choose('balanced')}>{upgradeText(info)}</button>
    </div>
  {/if}
  {#if info}
    <div class="srow">
      <div class="rtext"><div class="rname">Check this PC again</div><div class="hint">Run the speed check again, for example after a driver or hardware change.</div></div>
      <button class="pillbtn" onclick={() => modelsCall('models_check_again')}>Check again</button>
    </div>
    <div class="srow">
      <div class="rtext"><div class="rname">Remove downloaded models</div>
        <div class="hint">Frees disk space. You can download a set again at any time.</div></div>
      <button class="pillbtn" onclick={remove} disabled={!info.sets.some((s) => s.installed)}>Remove…</button>
    </div>
  {/if}
</section>

{#if info}
  <section class="group" aria-label="This PC">
    <div class="ghead"><span class="lab">This PC</span></div>
    <ul class="facts">{#each info.pc as fact}<li class="chip">{fact}</li>{/each}</ul>
    <p class="gh">{info.reason}</p>
    {#if info.check}<p class="gh check" class:slow={info.check_slow}><strong>Test on this PC:</strong> {info.check}</p>{/if}
  </section>

  <section class="group" aria-label="Model set">
    <div class="ghead"><span class="lab">Model set</span></div>
    <div class="sets" role="radiogroup" aria-label="Model set">
      <label class="set" class:on={info.choice === 'auto'}>
        <input type="radio" name="model-set" value="auto" checked={info.choice === 'auto'} onchange={() => choose('auto')} />
        <span class="radio" aria-hidden="true"></span>
        <span class="body">
          <span class="name">Automatic{info.recommended ? ` · ${setName(info.recommended)}` : ''}<span class="tag">Recommended</span></span>
          <span class="desc">The set this PC suits, from the check above.</span>
        </span>
      </label>
      {#each info.sets as set (set.id)}
        <label class="set" class:on={info.choice === set.id}>
          <input type="radio" name="model-set" value={set.id} checked={info.choice === set.id} onchange={() => choose(set.id)} />
          <span class="radio" aria-hidden="true"></span>
          <span class="body">
            <span class="name">{set.label}<span class="size">{set.size_mb} MB</span>{#if set.installed}<span class="tag dl">Downloaded</span>{/if}</span>
            <span class="desc">{set.what}</span>
            {#if !set.suits}<span class="warn">This PC is below what this set needs. It still works, but may be slow.</span>{/if}
          </span>
        </label>
      {/each}
    </div>
  </section>
{/if}

<style>
  .meter { height: 6px; margin: 4px 0 12px; border-radius: 3px; background: var(--hair); overflow: hidden; }
  .meter span { display: block; height: 100%; background: var(--signal); transition: width .25s var(--ease); }
  .facts { margin: 0 0 12px; padding: 0; list-style: none; display: flex; flex-wrap: wrap; gap: 6px; }
  .check { color: var(--ink); }
  .check.slow { color: var(--signal-ink); }
  .gh:last-child { margin-bottom: 14px; }

  /* the sets: a list; the chosen one is lifted with a dark ring, and its radio fills in orange */
  .sets { display: flex; flex-direction: column; margin: 0 -8px 8px; }
  .set { position: relative; display: flex; gap: 14px; align-items: flex-start; padding: 13px 12px; border-radius: var(--r-row); border: 1.5px solid transparent; cursor: pointer; transition: border-color .2s var(--ease), background-color .2s var(--ease); }
  .set + .set { border-top-color: var(--hair); }
  .set:hover { background: var(--sunk); }
  .set.on { border-color: var(--ink); background: var(--sunk); }
  .set + .set.on, .set.on + .set { border-top-color: transparent; }
  .set input { position: absolute; opacity: 0; inset: 0; margin: 0; cursor: pointer; }
  .radio { flex: none; width: 18px; height: 18px; margin-top: 2px; border-radius: 50%; border: 1.5px solid var(--line); background: var(--surface); transition: border-color .2s var(--ease), box-shadow .2s var(--ease); }
  .set.on .radio { border-color: var(--ink); box-shadow: inset 0 0 0 4px var(--surface), inset 0 0 0 9px var(--signal); }
  .set:has(input:focus-visible) { outline: 2px solid var(--signal); outline-offset: 2px; }
  .body { display: flex; flex-direction: column; gap: 3px; min-width: 0; }
  .name { font-size: 14px; font-weight: 600; display: flex; flex-wrap: wrap; align-items: center; gap: 8px; }
  .size { font: 500 11px/1 var(--mono); color: var(--muted); }
  .tag { font: 600 10px/1 var(--mono); letter-spacing: .06em; text-transform: uppercase; padding: 4px 7px; border-radius: var(--r-pill); background: var(--hair); color: var(--ink); }
  .tag.dl { background: var(--ink); color: var(--inv); }
  .desc { font-size: 13px; color: var(--muted); line-height: 1.45; }
  .warn { font-size: 12px; color: var(--signal-ink); line-height: 1.4; }
  @media (prefers-reduced-motion: reduce) { .meter span, .set, .radio { transition: none; } }
</style>
