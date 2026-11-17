<script>
  import { status } from '../status.svelte.js';

  import { guide, send } from '../guide/onboarding.svelte.js';
  import { NOTICE } from '../guide/lines.js';

  const engine = $derived(status.engine);
</script>

<!-- The voice model arrived after the first part of the guide: one offer to do the voice part. -->
{#if guide.loaded && guide.status === 'voice_offered'}
  <div class="banner offer" role="status"><span class="bar"></span>
    <span class="grow">{NOTICE}</span>
    <button class="start" onclick={() => send('next')}>Start</button>
    <button class="later" onclick={() => send('skip_all')}>Not now</button>
  </div>
{/if}

<!-- A black strip with an orange bar, on every screen (DESIGN.md 5). -->
{#if status.error}
  <div id="problem" class="banner" role="alert"><span class="bar"></span>OASIS could not start: {status.error}</div>
{/if}
{#if engine && !engine.integrity_ok}
  <div id="integrity" class="banner" role="alert"><span class="bar"></span>
    Your profiles file was changed outside OASIS, so no OASIS interface press will run an action and nothing can be saved until it is restored.
  </div>
{/if}
{#if engine?.pad === 'Unavailable'}
  <div id="padProblem" class="banner" role="alert"><span class="bar"></span>
    Bluetooth is not available, so the OASIS interface cannot connect.{engine.last_error ? ` ${engine.last_error}` : ''}
  </div>
{/if}

<style>
  .banner { display: flex; align-items: center; gap: 12px; flex: none; background: var(--ink); color: var(--inv); border-radius: var(--r-row); padding: 10px 14px 10px 0; font-size: 13px; overflow: hidden; }
  .bar { align-self: stretch; width: 4px; background: var(--signal); flex: none; }
  /* the one banner with buttons */
  .offer { padding-top: 6px; padding-bottom: 6px; }
  .grow { flex: 1; min-width: 0; }
  .start { height: 30px; padding: 0 16px; border: 0; border-radius: var(--r-pill); background: var(--signal); color: var(--on-signal); font-size: 13px; font-weight: 600; }
  /* Dark: a sunk strip with a hairline, not a full-width white bar across the top of the window. */
  :global(:root[data-theme='dark']) .banner { background: var(--sunk); color: var(--ink); box-shadow: inset 0 0 0 1px var(--line); }
  :global(:root[data-theme='dark']) .later { color: var(--ink); }
  .later { height: 30px; padding: 0 8px; border: 0; background: transparent; color: var(--inv); opacity: .75; font-size: 13px; text-decoration: underline; text-underline-offset: 3px; }
  .later:hover { opacity: 1; }
</style>
