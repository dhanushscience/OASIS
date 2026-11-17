<script>
  import { data, order, select, addProfile, isEverywhere, looksLikeSite, askIfExists, moveProfileTo, editExisting } from '../profiles.svelte.js';
  import { loadApps } from '../apps.svelte.js';
  import { status } from '../status.svelte.js';
  import AppInput from './AppInput.svelte';
  import Popover from './Popover.svelte';
  import OsLogo from './OsLogo.svelte';

  const STEP = 62; // a 52px tile and a 10px gap
  const EDGE = 26; // room at each end, so the edge fade never cuts into a tile
  const ids = $derived(order());
  const at = $derived(Math.max(0, ids.indexOf(data.profile)));
  const front = $derived(status.engine?.front_profile ?? null);
  const counter = $derived(ids.length ? `${String(at + 1).padStart(2, '0')} / ${String(ids.length).padStart(2, '0')}` : '');

  let width = $state(420);
  // Keep the chosen tile in view: the strip slides, the tiles do not move on their own.
  const shift = $derived(
    ids.length * STEP - 10 + 2 * EDGE <= width ? 0 : Math.max(width - (ids.length * STEP - 10 + 2 * EDGE), Math.min(0, width / 2 - (EDGE + at * STEP + 26))),
  );

  // ---- rearranging: hold a tile and drag it sideways (the default profile stays first)
  const HOLD_MS = 350;
  const firstMovable = $derived(ids.filter((id) => isEverywhere(data.profiles[id])).length);
  /** positions are places in `ids`; dx is how far the pointer has moved */
  let drag = $state(/** @type {{ id: number, from: number, to: number, dx: number } | null} */ (null));
  let pressX = 0;
  let holdTimer = 0;
  let justDragged = false;

  function down(/** @type {PointerEvent & { currentTarget: HTMLElement }} */ e, /** @type {number} */ id) {
    if (e.button !== 0 || isEverywhere(data.profiles[id])) return;
    pressX = e.clientX;
    const el = e.currentTarget, pointer = e.pointerId, from = ids.indexOf(id);
    clearTimeout(holdTimer);
    holdTimer = setTimeout(() => { drag = { id, from, to: from, dx: 0 }; el.setPointerCapture(pointer); }, HOLD_MS);
  }
  function move(/** @type {PointerEvent} */ e) {
    if (!drag) { if (holdTimer && Math.abs(e.clientX - pressX) > 8) { clearTimeout(holdTimer); holdTimer = 0; } return; }
    const dx = e.clientX - pressX;
    drag = { ...drag, dx, to: Math.max(firstMovable, Math.min(ids.length - 1, Math.round(drag.from + dx / STEP))) };
  }
  async function up() {
    clearTimeout(holdTimer);
    holdTimer = 0;
    if (!drag) return;
    const { from, to } = drag;
    drag = null;
    justDragged = true; // the click that follows the release is not a choice
    setTimeout(() => (justDragged = false), 60);
    if (to !== from) { try { await moveProfileTo(ids[from], ids[to]); } catch (e) { problem = String(e); } }
  }
  const cancel = () => { clearTimeout(holdTimer); holdTimer = 0; drag = null; };
  /** How far a tile sits from its place while another is being dragged: the ones it passes step aside. */
  const slide = (/** @type {number} */ k) => (!drag ? 0 : k === drag.from ? drag.dx : drag.from < k && k <= drag.to ? -STEP : drag.to <= k && k < drag.from ? STEP : 0);
  /** Alt + arrow moves the chosen tile too, for the keyboard. */
  async function nudge(/** @type {KeyboardEvent} */ e, /** @type {number} */ id) {
    const by = e.altKey ? ({ ArrowLeft: -1, ArrowRight: 1 })[/** @type {'ArrowLeft'} */ (e.key)] : 0;
    const from = ids.indexOf(id), to = Math.max(firstMovable, Math.min(ids.length - 1, from + (by ?? 0)));
    if (!by || isEverywhere(data.profiles[id]) || to === from) return;
    e.preventDefault();
    try { await moveProfileTo(ids[from], ids[to]); } catch (err) { problem = String(err); }
  }

  const step = (/** @type {number} */ d) => ids[at + d] != null && select(ids[at + d]);

  // "Just used: Figma": the last app in front has no profile of its own yet.
  const just = $derived.by(() => {
    const app = status.engine?.front_app;
    if (!app || !data.loaded) return null;
    const matched = front != null ? data.profiles[front] : null;
    return !matched || isEverywhere(matched) ? app : null;
  });
  const nice = (/** @type {string} */ process) => process.charAt(0).toUpperCase() + process.slice(1);

  let busy = $state(false);
  let problem = $state('');
  async function make() {
    if (!just || askIfExists(just.process)) return;
    busy = true;
    problem = '';
    try { await addProfile(nice(just.process), just.process, 'app'); }
    catch (e) { problem = String(e); }
    busy = false;
  }

  // The new-profile form.
  let target = $state('');
  let name = $state('');
  let formError = $state('');
  /** @param {() => void} close */
  async function create(close) {
    formError = '';
    if (askIfExists(target)) return close();
    try {
      await addProfile(name, target, looksLikeSite(target) ? 'website' : 'app');
      target = name = '';
      close();
    } catch (e) {
      formError = String(e);
    }
  }
</script>

<section aria-label="Application profiles">
  <div class="top">
    <div class="title"><span class="lab">Application profile</span><span class="count mono">{counter}</span></div>
    {#if just}
      <button class="just" onclick={make} disabled={busy} aria-label="Make a profile for {nice(just.process)}">
        <span class="dim">Just used:</span><span class="app">{nice(just.process)}</span><span class="cta">Make profile</span>
      </button>
    {/if}
  </div>
  {#if problem}<div class="problem" role="alert">{problem}</div>{/if}

  <div class="strip">
    <button class="arrow" aria-label="Previous profile" onclick={() => step(-1)} disabled={at <= 0}>‹</button>
    <div class="window" role="listbox" aria-label="Profiles" aria-orientation="horizontal" bind:clientWidth={width}>
      <div class="track" style="transform: translateX({shift}px); padding: 0 {EDGE}px">
        {#each ids as id, k (id)}
          {@const p = data.profiles[id]}
          {@const on = id === data.profile}
          <button role="option" aria-selected={on} class="tile" class:on class:drag={drag?.id === id} title={p.name}
            style="transform: translateX({slide(k)}px){drag?.id === id ? ' scale(1.1)' : ''}"
            onclick={() => !justDragged && select(id)} ondblclick={() => !justDragged && editExisting(id)} onpointerdown={(e) => down(e, id)} onpointermove={move} onpointerup={up} onpointercancel={cancel} onkeydown={(e) => nudge(e, id)}
            aria-label="{p.name}, {isEverywhere(p) ? 'everything else' : p.context_match}{front === id ? ', in front now' : ''}">
            {#if data.icons[id]}<img class="pic" src={data.icons[id]} alt="" />{:else if p.icon_emoji}{p.icon_emoji}{:else if isEverywhere(p)}<OsLogo size={28} />{:else}{p.name.charAt(0).toUpperCase()}{/if}
            {#if front === id}<span class="front"></span>{/if}
            <span class="notch" class:on></span>
          </button>
        {/each}
      </div>
    </div>
    <button class="arrow" aria-label="Next profile" onclick={() => step(1)} disabled={at >= ids.length - 1}>›</button>

    <Popover label="New profile" align="right">
      {#snippet trigger({ toggle })}
        <button class="plus" data-trigger aria-label="New profile" onclick={() => { toggle(); loadApps(); }}>+</button>
      {/snippet}
      {#snippet children({ close })}
        <form class="form" onsubmit={(e) => { e.preventDefault(); create(close); }}>
          <label><span class="lab">App or website</span>
            <AppInput bind:value={target} maxlength={200} placeholder="chrome, figma, github.com" label="App or website" />
          </label>
          <label><span class="lab">Name (optional)</span>
            <input bind:value={name} autocomplete="off" maxlength="40" placeholder="taken from the app or site" />
          </label>
          {#if formError}<div class="problem" role="alert">{formError}</div>{/if}
          <button class="primary" type="submit" disabled={!target.trim()}>Create profile</button>
        </form>
      {/snippet}
    </Popover>
  </div>
</section>

<style>
  .pic { width: 30px; height: 30px; object-fit: contain; border-radius: 7px; display: block; }
  section { display: flex; flex-direction: column; gap: 10px; }
  .top { display: flex; flex-wrap: wrap; align-items: center; justify-content: space-between; gap: 6px 10px; min-height: 28px; }
  .title { display: flex; align-items: baseline; gap: 10px; white-space: nowrap; flex: none; }
  .lab { font: 600 13px/1.3 var(--sans); letter-spacing: .08em; text-transform: uppercase; color: var(--ink); }
  .count { font-size: 11px; color: var(--muted); }
  .mono { font-family: var(--mono); }

  .just { height: 28px; padding: 0 6px 0 12px; border-radius: var(--r-pill); border: 0; background: var(--ink); color: var(--inv); font-size: 12px; display: flex; align-items: center; gap: 8px; white-space: nowrap; flex: none; }
  .just:disabled { opacity: .6; }
  .dim { opacity: .7; }
  .app { font-weight: 600; max-width: 14ch; overflow: hidden; text-overflow: ellipsis; }
  .cta { height: 20px; padding: 0 9px; border-radius: var(--r-pill); background: var(--inv); color: var(--ink); font-weight: 600; display: flex; align-items: center; white-space: nowrap; }

  .strip { display: flex; align-items: flex-start; gap: 8px; }
  .window { flex: 1; min-width: 0; overflow: hidden; padding: 4px 0 10px; margin-top: -4px; mask-image: linear-gradient(to right, transparent 0, #000 6%, #000 94%, transparent 100%); }
  .track { display: flex; gap: 10px; transition: transform .25s var(--ease); }
  .tile { position: relative; display: flex; align-items: center; justify-content: center; padding: 0; width: 52px; height: 52px; flex: none; border-radius: var(--r-bar); font-size: 19px; font-weight: 700; background: var(--surface); color: var(--muted); border: 1.5px solid var(--line); touch-action: pan-y; user-select: none; transition: transform .2s var(--ease), background-color .12s var(--ease), color .12s var(--ease), border-color .12s var(--ease); }
  .tile.drag { transition: background-color .12s var(--ease); z-index: 3; cursor: grabbing; box-shadow: 0 8px 18px var(--shadow); }
  /* the chosen tile keeps its light face: a dark border and ring say it is chosen, so a picture icon is never put on black */
  .tile.on { background: var(--surface); color: var(--ink); border-color: var(--ink); box-shadow: 0 0 0 1.5px var(--ink); }
  .front { position: absolute; top: -3px; right: -3px; width: 10px; height: 10px; border-radius: 50%; background: var(--ok); border: 2px solid var(--surface); }
  .notch { position: absolute; left: 50%; bottom: -9px; width: 18px; height: 3px; border-radius: 2px; background: var(--signal); transform: translateX(-50%) scaleX(.4); opacity: 0; transition: transform .28s var(--ease), opacity .28s var(--ease); }
  .notch.on { transform: translateX(-50%) scaleX(1); opacity: 1; }
  .arrow { width: 30px; height: 52px; border-radius: 10px; border: 1px solid var(--line); background: transparent; flex: none; font-size: 15px; }
  .arrow:disabled { opacity: .35; cursor: default; }
  .plus { width: 52px; height: 52px; border-radius: var(--r-bar); border: 1.5px dashed var(--faint); background: transparent; flex: none; font-size: 22px; color: var(--muted); }

  .form { display: flex; flex-direction: column; gap: 10px; }
  .form label { display: flex; flex-direction: column; gap: 4px; }
  input { height: 36px; border-radius: var(--r-field); border: 1.5px solid var(--line); background: var(--surface); color: var(--ink); padding: 0 12px; font-size: 13px; outline: none; }
  input:focus-visible { border-color: var(--ink); }
  .primary { height: 36px; border-radius: var(--r-pill); border: 0; background: var(--ink); color: var(--inv); font-weight: 600; font-size: 13px; }
  .primary:disabled { opacity: .4; cursor: default; }
  .problem { font-size: 12px; color: var(--bad); }
</style>
