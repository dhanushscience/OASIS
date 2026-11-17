<script>
  // Mr. Science inside the application: a card in the corner that says what to do at the guide's
  // current step and pulses the control it is about. It finds that control by a `data-guide` mark on
  // the real control, never by where it sits; with no such control he just talks.
  import { onMount } from 'svelte';
  import MrScience from './MrScience.svelte';
  import { guide, startGuide, send, at } from '../guide/onboarding.svelte.js';
  import { LINES, BLOCKED, DOWNLOAD, REFUSED } from '../guide/lines.js';
  import { models } from '../models.svelte.js';
  import { status } from '../status.svelte.js';
  import { sure } from '../confirm.svelte.js';

  const tauri = /** @type {any} */ (window).__TAURI__;
  const invoke = (/** @type {string} */ c, /** @type {object} */ a = {}) => tauri.core.invoke(c, a);

  const running = $derived(guide.loaded && (guide.status === 'running' || guide.status === 'new'));
  const here = $derived(running && ['device', 'first_press', 'voice_key'].includes(guide.step));

  /** What button 2 said when it would not run. */
  let refused = $state('');
  $effect(() => { guide.step; refused = ''; });

  const blocked = $derived(guide.blocked ? /** @type {Record<string, string>} */ (BLOCKED)[guide.blocked] : '');
  const text = $derived.by(() => {
    if (refused) return refused;
    if (blocked) return blocked;
    if (guide.step === 'device') return LINES.device[status.engine?.pad_name ? 1 : 0];
    return /** @type {Record<string, string[]>} */ (LINES)[guide.step]?.[0] ?? '';
  });
  const mood = $derived(refused || blocked ? 'retry' : guide.step === 'first_press' ? 'point' : guide.step === 'voice_key' ? 'idle' : 'idle');

  // The last word, shown once when the guide finishes while this window is open.
  let finished = $state(false);
  let seenRunning = false;
  $effect(() => {
    if (running) seenRunning = true;
    else if (guide.status === 'complete' && seenRunning) { seenRunning = false; finished = true; setTimeout(() => (finished = false), 9000); }
  });

  // Pulse the control the step is about.
  const TARGET = /** @type {Record<string, string>} */ ({ device: 'device', first_press: 'button-2' });
  $effect(() => {
    const name = here ? TARGET[guide.step] : '';
    if (!name) return;
    const found = [...document.querySelectorAll(`[data-guide="${name}"]`)];
    found.forEach((el) => el.classList.add('guide-pulse'));
    return () => found.forEach((el) => el.classList.remove('guide-pulse'));
  });

  // Before the first press, on a PC with data from an earlier release: button 2 is in use, or there
  // is no way to press a button. Asked once, in one pop-up (beta testers only; a fresh install never sees it).
  let asking = false;
  $effect(() => {
    if (!(at('first_press') && guide.ask) || asking) return;
    asking = true;
    const parts = [
      guide.button_used ? 'Button 2 will open the OASIS website for this demo, then be cleared. What it does now is replaced.' : '',
      !guide.keyboard && guide.has_device !== true ? 'Keyboard keys will be switched on, so you can press a button without the OASIS pad.' : '',
    ].filter(Boolean);
    sure({ title: 'Change this for the demo?', body: parts.join(' '), yes: 'Go ahead', keep: 'Skip this part' }).then(async (ok) => {
      if (ok) await invoke('onboarding_prepare_press').catch((/** @type {unknown} */ e) => (refused = String(e)));
      else send('skip_step');
      asking = false;
    });
  });

  // Voice is switched on when the voice part begins. The talk key is not touched: button 1 is the default.
  let voiceOn = false;
  $effect(() => {
    if (!at('voice_key') || voiceOn) return;
    voiceOn = true;
    invoke('set_setting', { id: 'voice.enabled', value: true }).catch(() => {});
  });

  onMount(startGuide);

  const waiting = $derived(guide.loaded && guide.status === 'waiting_for_voice');
  const download = $derived.by(() => {
    const info = /** @type {any} */ (models.info);
    if (!info) return '';
    const line = /** @type {Record<string, string>} */ (DOWNLOAD)[info.phase] ?? '';
    return info.phase === 'downloading' && info.total_mb ? `${line} ${Math.round((info.done_mb / info.total_mb) * 100)}%` : line;
  });
</script>

<svelte:window onkeydown={(e) => here && e.key === 'Escape' && !e.defaultPrevented && send('skip_all')} />

{#if here}
  <aside class="guide" aria-label="Getting Started">
    <div class="who"><MrScience {mood} scale={4} /></div>
    <div class="say">
      {#key text}<p class="bubble" role="status">{text}</p>{/key}
      {#if guide.step === 'first_press' && guide.keyboard && guide.chord}
        <div class="chord"><span>Button 2 on your keyboard</span><kbd>{guide.chord}</kbd></div>
      {/if}
      <div class="acts">
        {#if guide.step === 'device'}<button class="plain" onclick={() => send('device_answer', false)}>Use the keyboard instead</button>{/if}
        {#if guide.step === 'voice_key'}<button class="go" disabled={!!blocked} onclick={() => send('next')}>Got it</button>{/if}
        {#if guide.blocked === 'no_microphone'}
          <button class="go" onclick={() => invoke('onboarding_open_mic_settings').catch(() => {})}>Windows microphone settings</button>
        {/if}
        {#if guide.step === 'return'}<button class="go" onclick={() => send('skip_step')}>Carry on</button>{/if}
        {#if guide.step === 'voice_open' || guide.step === 'voice_type' || guide.step === 'voice_close'}
          <button class="plain" onclick={() => send('skip_all')}>Stop the guide</button>
        {:else if guide.step !== 'return'}
          <button class="plain" onclick={() => send('skip_step')}>Skip</button>
        {/if}
      </div>
    </div>
  </aside>
{:else if finished}
  <aside class="guide" aria-label="Getting Started">
    <div class="who"><MrScience mood="success" scale={4} /></div>
    <div class="say"><p class="bubble" role="status">{LINES.done[0]}</p></div>
  </aside>
{:else if waiting && download}
  <!-- the voice part waits for a model: one quiet row that says how the download is doing -->
  <aside class="wait" aria-label="Getting Started">
    <span class="lock" aria-hidden="true"><svg viewBox="0 0 24 24"><path d="M7 11V8a5 5 0 0 1 10 0v3M6 11h12v9H6z" /></svg></span>
    <span class="wtext"><b>Voice guide comes next.</b> {download}</span>
    <button class="plain" onclick={() => send('skip_all')}>Dismiss</button>
  </aside>
{/if}

<style>
  /* low on the left of the stage: clear of the Configure panel's Save, which sits bottom right */
  .guide { position: fixed; z-index: 40; left: 248px; bottom: 36px; max-width: 420px; display: flex; align-items: flex-end; gap: 12px; animation: in .24s var(--ease); }
  .who { flex: none; }
  .say { display: flex; flex-direction: column; align-items: flex-start; gap: 10px; padding: 16px 18px; border-radius: 20px 20px 20px 4px; background: var(--surface); border: 1px solid var(--line); box-shadow: 0 12px 34px var(--shadow); }
  .bubble { margin: 0; font-size: 16px; font-weight: 600; line-height: 1.35; letter-spacing: -.01em; animation: in .24s var(--ease); }
  .chord { display: flex; align-items: center; gap: 10px; font-size: 12px; color: var(--muted); }
  kbd { padding: 4px 10px; border-radius: 8px; background: var(--ink); color: var(--inv); font: 600 13px/1.2 var(--mono); }
  .acts { display: flex; flex-wrap: wrap; align-items: center; gap: 8px; }
  .go { height: 36px; padding: 0 18px; border: 0; border-radius: var(--r-row); background: var(--signal); color: var(--on-signal); font-size: 14px; font-weight: 600; }
  .plain { height: 32px; padding: 0 4px; border: 0; background: transparent; font-size: 13px; color: var(--muted); text-decoration: underline; text-underline-offset: 3px; }
  .plain:hover { color: var(--ink); }
  button:disabled { opacity: .4; cursor: default; }

  .wait { position: fixed; z-index: 40; left: 248px; bottom: 36px; max-width: 460px; display: flex; align-items: center; gap: 10px; padding: 10px 14px; border-radius: var(--r-row); background: var(--surface); border: 1px solid var(--line); box-shadow: 0 8px 24px var(--shadow); font-size: 13px; color: var(--muted); }
  .wait b { color: var(--ink); font-weight: 600; }
  .lock { flex: none; width: 18px; height: 18px; }
  .lock svg { width: 18px; height: 18px; fill: none; stroke: currentColor; stroke-width: 1.8; stroke-linecap: round; stroke-linejoin: round; }
  .wtext { min-width: 0; }

  /* the control the step is about: a soft ring that breathes, in the app's own orange */
  :global(.guide-pulse) { animation: guide-pulse 1.4s ease-in-out infinite; }
  @keyframes -global-guide-pulse { 0%, 100% { box-shadow: 0 0 0 0 var(--halo); } 50% { box-shadow: 0 0 0 9px var(--halo); } }
  @keyframes in { from { opacity: 0; transform: translateY(8px); } }
  @media (prefers-reduced-motion: reduce) { :global(.guide-pulse) { animation: none; box-shadow: 0 0 0 4px var(--halo); } }
</style>
