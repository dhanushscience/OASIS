<script>
  import { status } from '../status.svelte.js';
  import { data } from '../profiles.svelte.js';

  /** @type {{ onpick?: (id: string) => void }} */
  let { onpick } = $props();

  // What the engine's pad state means for the card (DESIGN.md 6.7: pad lost is amber, with words).
  /** @type {Record<string, { tone: string, text: string, sub?: string }>} */
  const words = {
    Connected: { tone: 'ok', text: 'Connected' },
    Connecting: { tone: 'warn', text: 'Connecting…', sub: 'one moment' },
    Scanning: { tone: 'warn', text: 'Interface lost', sub: 'searching…' },
    Disconnected: { tone: 'warn', text: 'Interface lost', sub: 'searching…' },
    Unavailable: { tone: 'bad', text: 'Bluetooth off', sub: 'unavailable' },
    Off: { tone: 'off', text: 'Interface off', sub: 'switched off in Input' },
  };
  const WAKE = 'If it stays like this, press a button on the OASIS interface to wake it.';

  const engine = $derived(status.engine);
  const known = $derived(
    engine ? (words[engine.pad] ?? { tone: 'warn', text: engine.pad, sub: '' }) : { tone: 'bad', text: 'Not running', sub: '' },
  );
  const linked = $derived(known.tone === 'ok');
  const percent = $derived(linked ? engine.battery_percent : null);
  // Same mark as the engine's one-time warning (oasis-core battery.rs LOW_PERCENT); clears itself on charge.
  const low = $derived(linked && percent != null && percent <= 20 && !engine.charging);

  // The input ways that are switched on (Input section); a pad that is off is not shown at all.
  const pad = $derived(data.input.hardware);
  const keys = $derived(data.input.keyboard);

  const label = $derived(
    [
      pad ? (linked ? `OASIS interface connected${percent != null ? `, battery ${percent} percent${engine.charging ? ', charging' : ''}` : ''}` : `${known.text}${known.sub ? `, ${known.sub}` : ''}`) : '',
      keys ? 'Keyboard keys on' : '',
    ].filter(Boolean).join('. ') + '. Open Input',
  );
</script>

<button type="button" class="card" class:low data-guide="device" aria-label={label} title={pad && engine?.pad === 'Scanning' ? WAKE : 'Open Input'} onclick={() => onpick?.('input')}>
  <span class="col">
    <span class="lab">{pad ? 'Battery' : 'Keyboard'}</span>
    {#if pad}
      <span class="big" id="battery">{percent != null ? `${percent}%` : '––'}</span>
      <span class="cap state {low ? 'bad' : known.tone}"><span id="padDot" class="dot"></span><span id="padText">{low ? 'Low battery' : known.text}</span></span>
    {:else}
      <span class="big">On</span>
      <span class="cap state ok"><span class="dot"></span>Keys active</span>
    {/if}
  </span>
  <span class="rule" aria-hidden="true"></span>
  <span class="col">
    <span class="lab">Input</span>
    <span class="big icons">
      {#if pad}
        <svg viewBox="0 0 48 22" aria-hidden="true"><rect x="1.5" y="1.5" width="45" height="19" rx="6" /><circle cx="12" cy="11" r="3.4" /><circle cx="24" cy="11" r="3.4" /><circle cx="36" cy="11" r="3.4" /></svg>
      {/if}
      {#if keys}
        <svg viewBox="0 0 48 22" aria-hidden="true"><rect x="1.5" y="1.5" width="45" height="19" rx="3" />{#each [0, 1, 2, 3, 4, 5] as i}<rect class="k" x={6 + i * 6.6} y="6" width="4" height="3" />{/each}{#each [0, 1, 2, 3, 4] as i}<rect class="k" x={9 + i * 6.6} y="11" width="4" height="3" />{/each}<rect class="k" x="12" y="16" width="24" height="2.4" /></svg>
      {/if}
    </span>
    <span class="cap">{pad && keys ? 'Interface + Keys' : pad ? 'Interface' : 'Keys'}</span>
  </span>
  <span id="padHint" class="sr">{pad && engine?.pad === 'Scanning' ? WAKE : ''}</span>
</button>

<style>
  .card { border: 1px solid var(--line); background: var(--surface); border-radius: var(--r-card); padding: 12px; display: grid; grid-template-columns: minmax(0, 1fr) 1px minmax(0, 1fr); align-items: stretch; column-gap: 9px; overflow: hidden; color: var(--ink); text-align: left; cursor: pointer;
    box-shadow: inset 0 1px 4px var(--shadow), 0 1px 0 rgb(255 255 255 / .7); }
  /* low battery: the inner depth turns into a red fade from the border */
  .card.low { border-color: color-mix(in srgb, var(--bad) 45%, var(--line)); box-shadow: inset 0 0 14px color-mix(in srgb, var(--bad) 40%, transparent), inset 0 1px 4px color-mix(in srgb, var(--bad) 30%, transparent), 0 1px 0 rgb(255 255 255 / .7); }
  /* Dark: the light theme's white emboss under the card read as a hard white line; a faint one instead. */
  :global(:root[data-theme='dark']) .card { box-shadow: inset 0 1px 4px var(--shadow), 0 1px 0 rgb(255 255 255 / .05); }
  :global(:root[data-theme='dark']) .card.low { box-shadow: inset 0 0 14px color-mix(in srgb, var(--bad) 40%, transparent), inset 0 1px 4px color-mix(in srgb, var(--bad) 30%, transparent), 0 1px 0 rgb(255 255 255 / .05); }
  .low .dot { background: var(--bad); }
  .card:focus-visible { outline: 2px solid var(--ink); outline-offset: 2px; }
  /* both columns read the same way: a small label, a value on one line, a caption */
  .col { display: flex; flex-direction: column; gap: 5px; min-width: 0; }
  .rule { background: var(--hair); }
  .lab { font: 500 10px/1 var(--mono); letter-spacing: .12em; text-transform: uppercase; color: var(--muted); }
  .big { height: 28px; display: flex; align-items: center; font: 400 28px/1 var(--mono); letter-spacing: -.04em; }
  .big.off { color: var(--muted); }
  .icons { gap: 5px; }
  .icons svg { width: 26px; height: 13px; fill: none; stroke: currentColor; stroke-width: 2.6; }
  .icons circle, .icons .k { fill: currentColor; stroke: none; }
  .cap { font: 400 10px/1.2 var(--mono); color: var(--muted); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .state { display: flex; align-items: center; gap: 4px; }
  .state.warn { color: var(--warn); }
  .state.bad { color: var(--bad); }
  .dot { width: 7px; height: 7px; border-radius: 50%; background: currentColor; flex: none; }
  .ok .dot { background: var(--ok); }
</style>
