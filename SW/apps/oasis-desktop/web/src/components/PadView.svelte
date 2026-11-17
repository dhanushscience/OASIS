<script>
  import KeyboardView from './KeyboardView.svelte';
  import { view } from '../pad.svelte.js';
  import { kindWord, doesWord, OUT_MS, data } from '../profiles.svelte.js';

  // `out` is the old profile fading away: the labels drift with the way the strip moves, then come
  // back with the new one, one after another (DESIGN.md section 0).
  /** @type {{ name: string, buttons: any[], selected: number, out: boolean, dir: number, waiting: boolean, flash: number, onpick: (i: number) => void }} */
  let { name, buttons, selected, out, dir, waiting, flash, onpick } = $props();

  // The whole group follows the room it has: smaller in a small window (never below 55%, so the labels stay inside a half-width window), larger on a big
  // screen so it does not float in empty space (the render is drawn at twice this size, so it stays sharp).
  let sw = $state(0), sh = $state(0);
  const scale = $derived(Math.max(0.4, Math.min(1.7, sh / 470, sw / 640)) || 1);
  // The keyboard takes the pad's place whenever the pad is not there to press, so the screen never looks switched off.
  const showKeys = $derived(data.input.keyboard && (waiting || (data.input.hardware && view.keys)));
  const KEY_LABELS = ['01', '02', '03'];
  // oasis-pad.png is a still render of the device model (3D/OASIS_ASM.usdz). Where its three keys sit, as
  // fractions of the picture; the picture is IMG px wide, and the labels stand in a column beside it.
  const IMG = 380, IMG_H = 388, PILL_X = 404;
  const KEYS = [{ x: 0.456, y: 0.157 }, { x: 0.592, y: 0.257 }, { x: 0.739, y: 0.362 }];
  const PILL_Y = [-45, 31, 107];
  /** The leader: from the key's disc a steady slope up (about 60 degrees), then level into the label's middle. */
  const start = (/** @type {number} */ i) => [KEYS[i].x * IMG, KEYS[i].y * IMG_H + 1];
  const lead = (/** @type {number} */ i) => {
    const [kx, ky] = start(i), py = PILL_Y[i] + 29;
    return `M${kx} ${ky} L${kx + Math.max(16, (ky - py) * 0.58)} ${py} H${PILL_X - 4}`;
  };
</script>

<div class="stage" bind:clientWidth={sw} bind:clientHeight={sh}>
  <div class="grid" aria-hidden="true" style="--s: {scale}"><div class="floor"></div></div>
  {#if showKeys}<KeyboardView {buttons} {selected} {flash} {onpick} />{/if}

  {#if !showKeys}
  <div class="pad" class:waiting style="width: {IMG}px; height: {IMG_H}px; transform: scale({scale})">
    <img class="device" src="./oasis-pad.png" alt="" width={IMG} height={IMG_H} draggable="false" />

    {#each buttons as _, i}
      {@const on = i === selected}
      <button class="key" class:empty={!buttons[i].label} class:on class:flash={flash === i} data-guide={i === 1 ? 'button-2' : undefined} style="left: {KEYS[i].x * 100}%; top: {KEYS[i].y * 100}%"
        aria-label="Button {KEY_LABELS[i]}: {buttons[i].label || 'not named'} ({name})" aria-pressed={on} onclick={() => onpick(i)}>
        <span class="num mono">{KEY_LABELS[i]}</span>
      </button>
    {/each}

    <svg class="leads" width="640" height={IMG_H} aria-hidden="true">
      {#each buttons as b, i}
      {#if b.label}
        <g class:on={i === selected} style="opacity: {out ? 0 : 1}; transition-delay: {out ? 0 : i * 20}ms"><path class="halo" d={lead(i)} /><path class="ln" d={lead(i)} /><circle class="dot" cx={start(i)[0]} cy={start(i)[1]} r="2.5" /></g>
      {/if}
      {/each}
    </svg>

    {#each buttons as b, i}
      {#if b.label}
      {@const on = i === selected}
      <div class="row" style="left: {PILL_X}px; top: {PILL_Y[i]}px; opacity: {out ? 0 : 1}; transform: translateY({out ? dir * 16 : 0}px); transition-duration: {out ? OUT_MS : 200}ms; transition-delay: {out ? 0 : i * 20}ms">
        <button class="pill" class:on onclick={() => onpick(i)}>
          <span class="head"><span class="mono n">{KEY_LABELS[i]}</span><span class="label">{b.label || 'Not set'}</span></span>
          <span class="mono sub">{kindWord(b)}{doesWord(b) ? ` · ${doesWord(b)}` : ''}</span>
        </button>
      </div>
      {/if}
    {/each}
  </div>
  {/if}
</div>

<style>
  .stage { position: relative; flex: 1; min-height: 0; }

  /* A floor grid seen from the render's angle; it shows round the device and fades out before the edges. */
  .grid { position: absolute; inset: 0; overflow: hidden; pointer-events: none;
    -webkit-mask-image: radial-gradient(ellipse calc(470px * var(--s)) calc(310px * var(--s)) at calc(50% - 90px * var(--s)) calc(50% + 40px * var(--s)), #000 25%, transparent 100%);
    mask-image: radial-gradient(ellipse calc(470px * var(--s)) calc(310px * var(--s)) at calc(50% - 90px * var(--s)) calc(50% + 40px * var(--s)), #000 25%, transparent 100%); }
  .floor { position: absolute; left: -50%; right: -50%; top: -30%; bottom: -30%; transform-origin: calc(50% + 45px * var(--s)) calc(50% + 40px * var(--s)); transform: perspective(800px) rotateX(52deg) rotateZ(-6deg);
    background-image: linear-gradient(color-mix(in srgb, var(--ink) 16%, transparent) 1px, transparent 1px), linear-gradient(90deg, color-mix(in srgb, var(--ink) 16%, transparent) 1px, transparent 1px);
    background-size: calc(36px * var(--s)) calc(36px * var(--s)); }

  .pad { position: absolute; left: calc(50% - 280px); top: calc(50% - 150px); transform-origin: 280px 192px; pointer-events: none; transition: opacity .3s var(--ease); }
  .pad.waiting { opacity: .4; }
  .device { display: block; width: 100%; height: 100%; user-select: none; pointer-events: none; }
  .mono { font-family: var(--mono); }

  /* a clear, invisible target over each key; the light sits on the key's round disc */
  .key { pointer-events: auto; position: absolute; width: 17%; height: 16%; transform: translate(-50%, -50%); border: 0; padding: 0; border-radius: 30%; background: transparent; display: flex; align-items: center; justify-content: center; }
  .num { width: 46px; height: 32px; margin-top: 3px; border-radius: 50%; display: flex; align-items: center; justify-content: center; font-size: 11px; color: rgb(255 255 255 / .55);
    box-shadow: 0 0 0 0 var(--signal); transition: box-shadow .12s var(--ease), background-color .12s var(--ease), color .12s var(--ease); }
  /* a button with no name yet: a faint grey patch on its disc says "click here to set it up" */
  .key.empty .num { background: rgb(170 175 185 / .3); box-shadow: inset 0 0 0 1px rgb(255 255 255 / .22); color: rgb(255 255 255 / .8); }
  .key.empty:hover .num { background: rgb(190 195 205 / .45); }
  .key:hover .num { background: rgb(255 255 255 / .1); }
  .key.on .num { color: var(--signal); box-shadow: 0 0 0 2px var(--signal), 0 0 18px 3px var(--halo); }
  /* a real press of this key on the pad: a signal-coloured layer fades in fast and out slowly */
  .key.flash .num { background: color-mix(in srgb, var(--signal) 60%, transparent); transition-duration: .12s; }

  .leads { position: absolute; left: 0; top: 0; pointer-events: none; overflow: visible; }
  .leads g { transition: opacity .2s var(--ease); --lc: var(--muted); }
  .leads g.on { --lc: var(--signal); }
  .leads path { fill: none; stroke-linejoin: round; stroke-linecap: round; }
  .leads .halo { stroke: rgb(255 255 255 / .85); stroke-width: 4; }
  .leads .ln { stroke: var(--lc); stroke-width: 1.25; transition: stroke .12s var(--ease); }
  .leads g.on .ln { stroke-width: 1.75; }
  .leads .dot { fill: var(--lc); stroke: rgb(255 255 255 / .85); stroke-width: 1.5; transition: fill .12s var(--ease); }

  .row { position: absolute; height: 58px; display: flex; align-items: center; transition-property: transform, opacity; transition-timing-function: var(--ease); }
  .pill { pointer-events: auto; display: flex; flex-direction: column; gap: 3px; text-align: left; padding: 10px 16px; border-radius: var(--r-bar); background: var(--surface); color: var(--ink); border: 1.5px solid var(--line); transition: background-color .12s var(--ease), border-color .12s var(--ease), color .12s var(--ease); max-width: 210px; }
  .pill.on { background: var(--ink); color: var(--inv); border-color: var(--ink); }
  /* Dark: the chosen label is a raised graphite card edged in the lead line's orange, not a white slab. */
  :global(:root[data-theme='dark']) .pill.on { background: var(--side); color: var(--ink); border-color: var(--signal); }
  .head { display: flex; align-items: baseline; gap: 8px; }
  .n { font-size: 12px; opacity: .7; }
  .label { font-size: 17px; font-weight: 600; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .sub { font-size: 12px; opacity: .7; white-space: nowrap; padding-left: 26px; overflow: hidden; text-overflow: ellipsis; max-width: 240px; }


</style>
