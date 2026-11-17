<script>
  import { data } from '../profiles.svelte.js';
  import { view } from '../pad.svelte.js';
  import { HUES, paint } from '../hues.js';

  // The keyboard standing in for the pad: the keys that press the three buttons light up on it, numbered
  // 01 to 03, and the three cards under it say which key does what. Nothing here needs the pad.
  /** @type {{ buttons: any[], selected: number, flash: number, onpick: (i: number) => void }} */
  let { buttons, selected, flash, onpick } = $props();

  // The minimal view (only the keys that are used) is what Settings asks for, or on a small window where the full keyboard is clutter.
  let w = $state(0), h = $state(0);
  const compact = $derived(view.layout === 'minimal' || (view.layout === 'auto' && h > 0 && (h < 470 || w < 660)));
  const NUMS = ['01', '02', '03'];
  /** [id, label, width in key units]; a number alone is a gap. Every row adds up to 15 units. */
  const ROWS = /** @type {(string | number)[][][]} */ ([
    [['esc', 'Esc', 1], 1, ...Array.from({ length: 12 }, (_, i) => [`f${i + 1}`, `F${i + 1}`, 1]), 1],
    [...'`1234567890-='.split('').map((c) => [c, c, 1]), ['backspace', 'Bksp', 2]],
    [['tab', 'Tab', 1.5], ...'qwertyuiop[]'.split('').map((c) => [c, c.toUpperCase(), 1]), ['\\', '\\', 1.5]],
    [['capslock', 'Caps', 1.75], ...`asdfghjkl;'`.split('').map((c) => [c, c.toUpperCase(), 1]), ['enter', 'Enter', 2.25]],
    [['shift', 'Shift', 2.25], ...'zxcvbnm,./'.split('').map((c) => [c, c, 1]), ['shift', 'Shift', 2.75]],
    [['ctrl', 'Ctrl', 1.25], ['win', 'Win', 1.25], ['alt', 'Alt', 1.25], ['space', '', 5.25], ['alt', 'Alt', 1.25], ['ctrl', 'Ctrl', 1.25],
      ['left', '←', 0.875], ['up', '↑', 0.875], ['down', '↓', 0.875], ['right', '→', 0.875]],
  ]);
  const ALIAS = /** @type {Record<string, string>} */ ({ escape: 'esc', control: 'ctrl', meta: 'win', ' ': 'space', arrowleft: 'left', arrowright: 'right', arrowup: 'up', arrowdown: 'down' });
  const MODS = ['ctrl', 'alt', 'shift', 'win'];

  /** Per button: its modifier keys and its main key, by id. */
  const chords = $derived([0, 1, 2].map((i) => {
    const parts = (data.input.keys[i] || '').split('+').map((p) => p.trim().toLowerCase()).filter(Boolean).map((p) => ALIAS[p] ?? p);
    // "Shift" alone is a modifier used as the key, so it is the main key then
    const main = parts.length ? parts[parts.length - 1] : '';
    return { main, mods: parts.slice(0, -1).filter((m) => MODS.includes(m)) };
  }));
  const placed = $derived(new Set(ROWS.flat().map((k) => (Array.isArray(k) ? k[0] : null)).filter(Boolean)));
  const none = $derived(!data.input.keys.some(Boolean));

  /** @param {string} id */
  function roles(id) {
    const main = [0, 1, 2].filter((i) => chords[i].main === id);
    const mod = [0, 1, 2].filter((i) => chords[i].mods.includes(id));
    return { main, mod, lit: main.length || mod.length };
  }
</script>

<div class="kb" bind:clientWidth={w} bind:clientHeight={h} role="group" aria-label="Keyboard shortcuts for the three buttons">
  <div class="cards">
    {#each buttons as b, i}
      {@const chord = data.input.keys[i]}
      <button class="card" class:on={i === selected} class:unset={!chord} data-guide={i === 1 ? 'button-2' : undefined} onclick={() => onpick(i)} aria-pressed={i === selected} style="--h:{HUES[i].c}">
        <span class="mono n">{NUMS[i]}</span>
        <span class="mid"><span class="what">{b.label || '—'}</span><span class="mono sub">{chord ? `press ${chord}` : 'no key chosen'}</span></span>
        {#if !compact && chord && !placed.has(chords[i].main)}<span class="mono off" title="This key is not drawn on the keyboard here, but it works">off-map</span>{/if}
      </button>
    {/each}
  </div>
  {#if compact}
  <div class="board compact" aria-hidden="true">
    {#each [0, 1, 2] as i}
      {@const parts = (data.input.keys[i] || '').split('+').map((p) => p.trim()).filter(Boolean)}
      <div class="grp">
        {#each parts as p, j}
          {@const last = j === parts.length - 1}
          <span class="key" class:main={last} class:mod={!last} class:sel={i === selected} class:flash={i === flash} style={paint(i, last)}>
            {p}{#if last}<b class="badge mono" style="background:{HUES[i].c}">{NUMS[i]}</b>{/if}
          </span>
        {:else}
          <span class="key none" style={paint(i, false)}>—<b class="badge mono" style="background:{HUES[i].c}">{NUMS[i]}</b></span>
        {/each}
      </div>
    {/each}
  </div>
  {:else}
  <div class="board" aria-hidden="true">
    {#each ROWS as row}
      <div class="row">
        {#each row as k}
          {#if Array.isArray(k)}
            {@const r = roles(String(k[0]))}
            <span class="key" class:main={r.main.length} class:mod={!r.main.length && r.mod.length} class:sel={r.main.includes(selected) || (!r.main.length && r.mod.includes(selected))}
              class:flash={r.lit && [...r.main, ...r.mod].includes(flash)} style="flex: {k[2]} 1 0;{r.lit ? paint(r.main.length ? r.main[0] : r.mod[0], r.main.length > 0) : ''}">
              {k[1]}{#if r.main.length}<b class="badge mono" style="background:{HUES[r.main[0]].c}">{r.main.map((i) => NUMS[i]).join(' ')}</b>{/if}
            </span>
          {:else}
            <span style="flex: {k} 1 0"></span>
          {/if}
        {/each}
      </div>
    {/each}
  </div>
  {/if}

  {#if none}<p class="hint">No keys chosen yet. Open <b>Input</b> in the side menu, click a Button box and press the keys you want.</p>{/if}
</div>

<style>
  .kb { position: absolute; inset: 0; container-type: size; overflow-x: hidden; overflow-y: auto; display: flex; flex-direction: column; justify-content: flex-start; align-items: center; gap: 22px; padding: clamp(12px, 4.5cqh, 40px) 12px 44px; }
  /* Seen from the same angle as the floor grid and the pad render: a dark slab with raised caps, standing on a soft shadow. */
  .board { --d: #a9abb2; position: relative; width: min(80%, 600px, max(300px, calc((100cqh - 150px) * 2))); flex: none; margin: auto 0; /* the cards stay up by the title; the keyboard centres in the room under them */ display: flex; flex-direction: column; gap: 6px; padding: 14px; border-radius: 18px;
    background: linear-gradient(160deg, #2b2e35, #16181c); box-shadow: inset 0 1px 0 rgb(255 255 255 / .14), 0 1px 0 #0c0d0f, 0 2px 0 #0c0d0f, 0 3px 0 #0c0d0f, 0 4px 0 #0c0d0f, 0 5px 0 #0c0d0f, 0 6px 0 #0c0d0f, 0 7px 0 #0c0d0f, 0 8px 0 #0c0d0f;
    transform: perspective(900px) rotateX(34deg) rotateZ(-5deg); transform-origin: 50% 60%; }
  .board::after { content: ''; position: absolute; inset: 10px -6px -14px; z-index: -1; border-radius: 24px; background: rgb(0 0 0 / .4); filter: blur(18px); transform: translateY(8px); }
  /* on a dark window the slab is a lighter graphite with a bright rim, so it stands off the background */
  :global(:root[data-theme='dark']) .board { background: linear-gradient(160deg, #565b68, #353943); box-shadow: inset 0 1px 0 rgb(255 255 255 / .28), 0 0 0 1px rgb(255 255 255 / .16), 0 1px 0 #2a2d34, 0 2px 0 #2a2d34, 0 3px 0 #2a2d34, 0 4px 0 #2a2d34, 0 5px 0 #2a2d34, 0 6px 0 #2a2d34, 0 7px 0 #2a2d34, 0 8px 0 #2a2d34; }
  :global(:root[data-theme='dark']) .board::after { background: rgb(150 160 190 / .16); }
  .row { display: flex; gap: 6px; }
  .board.compact { width: auto; max-width: 94%; flex-direction: row; justify-content: center; gap: 26px; padding: 22px 26px; }
  .grp { display: flex; gap: 6px; }
  /* a narrow area: the three groups stand in a column, so none is cut off at the sides */
  @container (max-width: 700px) {
    .board.compact { flex-direction: column; align-items: center; gap: 12px; padding: 16px 22px; }
    .compact .key { height: 36px; font-size: 12px; }
  }
  .compact .key { flex: none; min-width: 48px; padding: 0 12px; height: 44px; font-size: 13px; }
  .key.none { opacity: .45; }
  .key { --c: #e9eaee; --e: var(--d); position: relative; min-width: 0; height: clamp(26px, 5.2vh, 40px); display: flex; align-items: center; justify-content: center; font: 600 11px/1 var(--mono); color: #4a4d55;
    border-radius: 6px; background: linear-gradient(#f7f7f9, var(--c)); transform: translateY(-4px);
    box-shadow: 0 1px 0 var(--e), 0 2px 0 var(--e), 0 3px 0 var(--e), 0 4px 0 var(--e), 0 5px 5px rgb(0 0 0 / .45);
    transition: background-color .24s var(--ease), color .24s var(--ease), box-shadow .12s var(--ease), transform .12s var(--ease); }
  .key.mod { background: var(--c); }
  .key.main { color: #fff; font-weight: 700; background: var(--c); box-shadow: 0 1px 0 var(--e), 0 2px 0 var(--e), 0 3px 0 var(--e), 0 4px 0 var(--e), 0 5px 5px rgb(0 0 0 / .45), 0 0 18px 2px color-mix(in srgb, var(--c) 55%, transparent); }
  .key.sel { outline: 2px solid #fff; outline-offset: 2px; }
  /* a real press: the cap sinks onto the slab */
  .key.flash { transform: translateY(0); box-shadow: 0 1px 0 var(--e), 0 0 22px 4px color-mix(in srgb, var(--c) 70%, transparent); transition-duration: .12s; }
  .badge { position: absolute; top: -9px; right: -5px; border: 1px solid rgb(255 255 255 / .6); padding: 2px 4px; border-radius: 6px; background: var(--ink); color: var(--inv); font-size: 9px; font-weight: 600; letter-spacing: .04em; }
  .mono { font-family: var(--mono); }

  .cards { flex: none; width: min(100%, 720px); display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: 10px; }
  .card { display: flex; align-items: center; gap: 10px; padding: 10px 12px; text-align: left; border-radius: var(--r-row); background: var(--surface); color: var(--ink); border: 1.5px solid var(--line); min-width: 0; transition: background-color .24s var(--ease), border-color .24s var(--ease), color .24s var(--ease); }
  .card.on { background: var(--ink); color: var(--inv); border-color: var(--ink); }
  .card.unset:not(.on) { border-style: dashed; }
  .card { border-color: color-mix(in srgb, var(--h) 45%, var(--line)); }
  .card.on { border-color: var(--h); box-shadow: 0 0 0 2px color-mix(in srgb, var(--h) 40%, transparent); }
  .n { flex: none; width: 30px; height: 30px; display: flex; align-items: center; justify-content: center; border-radius: 50%; background: var(--h); color: #fff; font-size: 12px; font-weight: 600; }
  .mid { display: flex; flex-direction: column; gap: 3px; min-width: 0; flex: 1; }
  .what { font-size: 15px; font-weight: 600; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .sub { font-size: 11px; opacity: .75; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .off { font-size: 9px; opacity: .6; border: 1px solid currentColor; border-radius: 4px; padding: 1px 4px; }
  .hint { margin: 0; max-width: 54ch; text-align: center; font-size: 13px; color: var(--muted); }
  .hint b { color: var(--ink); }
</style>
