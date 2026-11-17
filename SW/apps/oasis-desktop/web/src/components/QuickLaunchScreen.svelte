<script>
  import { trackUnsaved } from '../busy.svelte.js';
  import { sure } from '../confirm.svelte.js';
  import { status } from '../status.svelte.js';
  import { loadQuick } from '../quicksetups.svelte.js';
  import AppInput from './AppInput.svelte';

  const { invoke } = window.__TAURI__?.core ?? { invoke: async () => { throw new Error('Not running inside OASIS.'); } };

  /** One item as the page edits it: where it goes is a mode and a region, not the engine's two fields. */
  /** @typedef {{ kind: string, target: string, open_with: string, screen: string, mode: string, region: { x: number, y: number, w: number, h: number }, open_another: boolean }} Item */
  /** @typedef {{ id: string, name: string, call_name: string, start_with_oasis: boolean, items: Item[] }} Setup */
  /** @typedef {{ spec: string, primary: boolean, left: number, top: number, width: number, height: number }} Screen */

  const WHOLE = { x: 0, y: 0, w: 1000, h: 1000 };
  const COLS = 12, ROWS = 6;
  /** one colour per item, in order; the 9th takes the 1st again */
  const COLORS = ['#f15025', '#3b8fd9', '#35b43a', '#9b5de5', '#e6a100', '#13a89e', '#e5508f', '#6b7a8f'];
  const color = (/** @type {number} */ n) => COLORS[n % COLORS.length];

  /** @param {any} i @returns {Item} */
  function fromEngine(i) {
    const half = i.layout === 2 ? { x: 0, y: 0, w: 500, h: 1000 } : i.layout === 3 ? { x: 500, y: 0, w: 500, h: 1000 } : null;
    const mode = i.region || half ? 'region' : i.layout === 1 ? 'full' : 'asis';
    return { kind: i.kind, target: i.target, open_with: i.open_with, screen: i.screen, mode, region: { ...(i.region ?? half ?? WHOLE) }, open_another: i.open_another };
  }
  /** @param {Item} i */
  const toEngine = (i) => ({ kind: i.kind, target: i.target, open_with: i.open_with, screen: i.screen, layout: i.mode === 'full' ? 1 : 0, region: i.mode === 'region' ? i.region : null, open_another: i.open_another });
  /** @returns {Setup} */
  const blankSetup = () => ({ id: '', name: '', call_name: '', start_with_oasis: false, items: [] });

  /** @type {Setup[]} */
  let setups = $state([]);
  /** @type {Screen[]} */
  let screens = $state([]);
  let loaded = $state(false);
  let query = $state('');
  /** the setup open on the right: its id, '' for a new one, null for none */
  let current = $state(/** @type {string | null} */ (null));
  let draft = $state(blankSetup());
  let baseline = $state('');
  /** the item the boxes on the screens are drawn for */
  let picked = $state(-1);
  let problem = $state('');
  let note = $state('');
  let busy = $state('');
  /** only the words "Primary" and "Secondary" change; nothing about the screens or the apps does */
  let swapped = $state(false);

  const ready = $derived(!!status.engine);
  const locked = $derived(!!status.engine && !status.engine.integrity_ok);
  const enabled = $derived(current !== null && !locked);
  const dirty = $derived(current !== null && JSON.stringify($state.snapshot(draft)) !== baseline);
  trackUnsaved('quick-launch', () => dirty);
  const shown = $derived(setups.filter((s) => !query.trim() || `${s.name} ${s.call_name}`.toLowerCase().includes(query.trim().toLowerCase())));

  /** @param {Setup | null} s */
  function open(s) {
    draft = s ? structuredClone($state.snapshot(s)) : blankSetup();
    current = s ? s.id : '';
    baseline = JSON.stringify($state.snapshot(draft));
    picked = draft.items.length ? 0 : -1;
    problem = note = '';
  }
  /** A setup being edited with changes not saved: opening another (or Create) asks first, so it is not lost by a slip. */
  let ask = $state(/** @type {{ to: Setup | null } | null} */ (null));
  const go = (/** @type {Setup | null} */ to) => { if (dirty) ask = { to }; else open(to); };
  const discard = () => { const s = setups.find((o) => o.id === current); if (s) open(s); else { current = null; draft = blankSetup(); } };

  async function load(keep = /** @type {string | null} */ (null)) {
    try {
      const list = await invoke('quick_setups');
      setups = list.map((/** @type {any} */ s) => ({ ...s, items: s.items.map(fromEngine) }));
      screens = await invoke('quick_screens');
      loaded = true;
      loadQuick();
      const again = keep !== null ? setups.find((s) => s.id === keep) : null;
      if (again) open(again);
    } catch (e) { problem = String(e); }
  }
  $effect(() => { if (ready && !loaded) load(); });

  /** A setup saved from somewhere else (a spoken "save my layout") appears in the list at once. The one
   *  being edited is left alone, so nothing typed is lost or reset. */
  async function refresh() {
    if (!loaded) return;
    try {
      const list = await invoke('quick_setups');
      setups = list.map((/** @type {any} */ s) => ({ ...s, items: s.items.map(fromEngine) }));
      loadQuick();
    } catch { /* the list stays as it was */ }
  }
  $effect(() => {
    const off = window.__TAURI__?.event.listen('engine-event', (/** @type {{ payload: { type: string } }} */ e) => {
      if (e.payload.type === 'settings_changed') refresh();
    });
    return () => off?.then((/** @type {() => void} */ f) => f());
  });

  async function save() {
    problem = note = '';
    busy = 'save';
    try {
      const snap = $state.snapshot(draft);
      const id = await invoke('save_quick_setup', { setup: { ...snap, items: snap.items.map(toEngine) } });
      await load(id);
      note = `Saved. Say “open ${snap.call_name.split(',')[0].trim() || snap.name.trim()}”.`;
    } catch (e) { problem = String(e); }
    busy = '';
  }
  async function remove() {
    busy = 'delete';
    try { await invoke('delete_quick_setup', { id: draft.id }); current = null; draft = blankSetup(); await load(); } catch (e) { problem = String(e); }
    busy = '';
  }
  async function launch(/** @type {string} */ id) {
    problem = note = '';
    busy = 'launch';
    try { note = await invoke('run_quick_setup', { id }); } catch (e) { problem = String(e); }
    busy = '';
  }
  /** Every window open now becomes an item, on its screen and in its place; a browser keeps its page. */
  async function capture() {
    problem = note = '';
    busy = 'capture';
    try {
      const got = (await invoke('capture_windows')).map(fromEngine);
      const have = new Set(draft.items.map((i) => `${i.kind}|${i.target.trim().toLowerCase()}`));
      const fresh = got.filter((/** @type {Item} */ i) => !have.has(`${i.kind}|${i.target.toLowerCase()}`) || i.open_another);
      draft.items = [...draft.items.filter((i) => i.target.trim()), ...fresh];
      if (picked < 0 && draft.items.length) picked = 0;
      note = fresh.length ? `Captured ${fresh.length} window${fresh.length === 1 ? '' : 's'}. Remove any you do not want.` : 'Nothing new is open.';
    } catch (e) { problem = String(e); }
    busy = '';
  }
  function addApp() {
    draft.items = [...draft.items, { kind: 'app', target: '', open_with: '', screen: screens.find((s) => s.primary)?.spec ?? '', mode: 'full', region: { ...WHOLE }, open_another: false }];
    picked = draft.items.length - 1;
  }
  function drop(/** @type {number} */ n) {
    draft.items = draft.items.filter((_, i) => i !== n);
    picked = Math.min(picked, draft.items.length - 1);
  }

  // ---- the screens, drawn where they sit and to scale, as large as the panel allows
  const GAP = 18;
  let stripW = $state(0);
  let stripH = $state(0);
  const layout = $derived.by(() => {
    if (!screens.length) return { width: 0, height: 0, boxes: [] };
    const minL = Math.min(...screens.map((s) => s.left)), minT = Math.min(...screens.map((s) => s.top));
    const maxR = Math.max(...screens.map((s) => s.left + s.width)), maxB = Math.max(...screens.map((s) => s.top + s.height));
    const gaps = GAP * (screens.length - 1);
    // what is left of the tray's height, less the line of words under the screens
    const maxH = Math.max(90, (stripH || 260) - 34);
    const scale = Math.min((Math.max(stripW, 240) - gaps) / (maxR - minL), maxH / (maxB - minT));
    // left to right, each nudged by the gaps before it so they never touch
    const boxes = screens.map((s, i) => ({ s, x: (s.left - minL) * scale + GAP * i, y: (s.top - minT) * scale, w: s.width * scale, h: s.height * scale }));
    return { width: (maxR - minL) * scale + gaps, height: (maxB - minT) * scale, boxes };
  });

  /** Which screen an item's spec lands on, as the engine reads it. */
  function screenOf(/** @type {string} */ spec) {
    if (!spec) return -1;
    if (spec === 'main') return screens.findIndex((s) => s.primary);
    if (spec.startsWith('other:')) return screens.map((s, i) => ({ s, i })).filter((o) => !o.s.primary)[Number(spec.slice(6)) - 1]?.i ?? -1;
    if (spec === 'left') return screens.length > 1 ? 0 : -1;
    if (spec === 'right') return screens.length > 1 ? screens.length - 1 : -1;
    return -1; // `screen:N` (Windows' own numbering) is kept as it is but not drawn
  }
  /** The words under a screen: which one it is called, and its number. */
  const isMain = (/** @type {Screen} */ s) => s.primary !== swapped;
  const secondaryNumber = (/** @type {number} */ si) => 1 + screens.filter((s, i) => i <= si && !isMain(s)).length;

  /** a drag in progress: which screen, from and to cell */
  let drag = $state(/** @type {{ screen: number, from: [number, number], to: [number, number] } | null} */ (null));
  /** @param {PointerEvent} e @param {HTMLElement} el */
  function cellAt(e, el) {
    const r = el.getBoundingClientRect();
    return /** @type {[number, number]} */ ([
      Math.min(COLS - 1, Math.max(0, Math.floor(((e.clientX - r.left) / r.width) * COLS))),
      Math.min(ROWS - 1, Math.max(0, Math.floor(((e.clientY - r.top) / r.height) * ROWS))),
    ]);
  }
  /** @param {PointerEvent & { currentTarget: HTMLElement }} e @param {number} screen */
  function down(e, screen) {
    if (!enabled || picked < 0) return;
    e.currentTarget.setPointerCapture(e.pointerId);
    const c = cellAt(e, e.currentTarget);
    drag = { screen, from: c, to: c };
  }
  /** @param {PointerEvent & { currentTarget: HTMLElement }} e */
  function move(e) { if (drag) drag.to = cellAt(e, e.currentTarget); }
  function up() {
    if (!drag || picked < 0) { drag = null; return; }
    const item = draft.items[picked];
    const r = cellsToRegion(drag.from, drag.to);
    item.screen = screens[drag.screen].spec;
    if (r.w === 1000 && r.h === 1000) item.mode = 'full';
    else { item.mode = 'region'; item.region = r; }
    drag = null;
  }
  /** The smallest box, in cells: smaller ones could not show their number and would misrepresent the window. */
  const MIN_CELLS = 2;
  /** A drag's span on one axis, at least MIN_CELLS long, grown away from the screen's edge. */
  function span(/** @type {number} */ a, /** @type {number} */ b, /** @type {number} */ cells) {
    let lo = Math.min(a, b), hi = Math.max(a, b);
    while (hi - lo + 1 < MIN_CELLS) { if (hi < cells - 1) hi++; else lo--; }
    return [lo, hi];
  }
  /** @param {[number, number]} a @param {[number, number]} b */
  function cellsToRegion(a, b) {
    const edge = (/** @type {number} */ n, /** @type {number} */ of) => Math.round((n * 1000) / of);
    const [[c0, c1], [r0, r1]] = [span(a[0], b[0], COLS), span(a[1], b[1], ROWS)];
    return { x: edge(c0, COLS), y: edge(r0, ROWS), w: edge(c1 + 1, COLS) - edge(c0, COLS), h: edge(r1 + 1, ROWS) - edge(r0, ROWS) };
  }
  /** Where an item's box sits on its screen, in percent; null when it opens wherever the app puts it. */
  function boxOf(/** @type {Item} */ item, /** @type {number} */ n) {
    if (drag && n === picked) {
      const r = cellsToRegion(drag.from, drag.to);
      return { screen: drag.screen, x: r.x / 10, y: r.y / 10, w: r.w / 10, h: r.h / 10 };
    }
    const screen = screenOf(item.screen);
    if (screen < 0 || item.mode === 'asis') return null;
    const r = item.mode === 'full' ? WHOLE : item.region;
    return { screen, x: r.x / 10, y: r.y / 10, w: r.w / 10, h: r.h / 10 };
  }
  /** The selected item's quick choices. */
  const PRESETS = [
    { name: 'Full', mode: 'full' },
    { name: 'Left', region: { x: 0, y: 0, w: 500, h: 1000 } },
    { name: 'Right', region: { x: 500, y: 0, w: 500, h: 1000 } },
    { name: 'As opened', mode: 'asis' },
  ];
  /** @param {{ mode?: string, region?: { x: number, y: number, w: number, h: number } }} p */
  function preset(p) {
    const item = draft.items[picked];
    if (!item) return;
    if (!item.screen) item.screen = screens.find((s) => s.primary)?.spec ?? '';
    if (p.region) { item.mode = 'region'; item.region = { ...p.region }; } else item.mode = p.mode ?? 'asis';
  }
  /** @param {{ mode?: string, region?: { x: number, y: number, w: number, h: number } }} p */
  function presetOn(p) {
    const it = draft.items[picked];
    if (!it) return false;
    return p.mode ? it.mode === p.mode : it.mode === 'region' && it.region.x === p.region?.x && it.region.w === p.region?.w && it.region.y === 0 && it.region.h === 1000;
  }

  /** How an item reads in the list. */
  const itemName = (/** @type {Item} */ i) => (i.kind === 'url' ? i.target.replace(/^https?:\/\//, '') : i.target) || 'Choose an app';
  const itemSub = (/** @type {Item} */ i) => (i.kind === 'url' ? `Browser: ${i.open_with || 'default'}` : i.kind === 'folder' ? 'Folder' : i.mode === 'asis' && !i.screen ? 'Where it opens' : '');
</script>

<div class="qlwrap"><div class="ql">
  <!-- left: every setup, searchable -->
  <section class="card list" aria-label="Setups">
    <div class="find">
      <input class="search" bind:value={query} placeholder="Search quick profile" aria-label="Search setups" autocomplete="off" />
      <button class="create" disabled={!ready || locked} onclick={() => go(null)}>Create +</button>
    </div>
    <div class="rows">
      {#if current === ''}
        <div class="row on"><span class="pick"><span class="ico" style="background: {color(0)}">{(draft.name.trim() || 'N').charAt(0).toUpperCase()}</span>
          <span class="txt"><span class="nm">{draft.name.trim() || 'New setup'}</span><span class="vc"><span>Voice cmd:</span> {draft.call_name || '—'}</span></span></span></div>
      {/if}
      {#each shown as s, i}
        <div class="row" class:on={current === s.id}>
          <button class="pick" onclick={() => go(s)} aria-current={current === s.id}>
            <span class="ico" style="background: {color(i)}">{s.name.charAt(0).toUpperCase()}</span>
            <span class="txt"><span class="nm">{s.name}</span><span class="vc"><span>Voice cmd:</span> {s.call_name || s.name}</span></span>
          </button>
          <button class="run" title="Launch now" aria-label="Launch {s.name} now" disabled={!!busy} onclick={() => launch(s.id)}><svg viewBox="0 0 24 24" aria-hidden="true"><path d="M8 5v14l11-7z" /></svg></button>
        </div>
      {:else}
        {#if current !== ''}<p class="hint pad">{!ready ? 'Setups appear here once OASIS is running.' : query ? 'No setup matches.' : 'No setups yet. Press Create to make one.'}</p>{/if}
      {/each}
    </div>
  </section>

  <!-- right: the setup being edited; greyed out until one is chosen -->
  <section class="card config" class:off={!enabled} inert={!enabled} aria-label="Configuration">
    <div class="chead">
      <h3>Configuration</h3>
      <span class="acts">
        <button class="pill" onclick={addApp}>Add app</button>
        <!-- the quicker way: one press takes every open window, on its screen and in its place -->
        <span class="tipwrap">
          <button class="pill main" disabled={!!busy} onclick={capture} aria-describedby="capture-tip">{busy === 'capture' ? 'Reading…' : 'Capture open windows'}</button>
          <span class="tip" id="capture-tip" role="tooltip">Saves the windows you have open right now, each on its screen and in its place. Next time, one launch puts them all back.</span>
        </span>
      </span>
    </div>

    <!-- the screens, as big as fits, centred, with what to do about them underneath -->
    <div class="board">
      <div class="bhead">
        <span class="lab">Screens</span>
        {#if screens.length === 2}<button class="swap" title="Swap the words Primary and Secondary" onclick={() => (swapped = !swapped)}>⇄ Swap labels</button>{/if}
      </div>
      <div class="strip" bind:clientWidth={stripW} bind:clientHeight={stripH}>
        <div class="stage" style="width: {layout.width}px; height: {layout.height + 30}px">
          {#each layout.boxes as b, si}
            <div class="screen" style="left: {b.x}px; top: {b.y}px; width: {b.w}px; height: {b.h}px" class:drawing={picked >= 0}
              role="application" aria-label="{isMain(b.s) ? 'Primary' : 'Secondary'} screen: drag to place the chosen app"
              onpointerdown={(e) => down(e, si)} onpointermove={move} onpointerup={up} onpointercancel={up}>
              {#each draft.items as item, n}
                {@const box = boxOf(item, n)}
                {#if box && box.screen === si}
                  <span class="win" class:sel={n === picked} class:tiny={(b.w * box.w) / 100 < 44 || (b.h * box.h) / 100 < 44} style="--c: {color(n)}; left: {box.x}%; top: {box.y}%; width: {box.w}%; height: {box.h}%"><span class="badge">{n + 1}</span></span>
                {/if}
              {/each}
            </div>
            <span class="cap mono" style="left: {b.x}px; top: {b.y + b.h + 8}px; width: {b.w}px">
              <span class="role" class:sec={!isMain(b.s)}>{isMain(b.s) ? 'Primary' : 'Secondary'}</span> Screen {isMain(b.s) ? 1 : secondaryNumber(si) + 1} · {b.s.width}×{b.s.height}
            </span>
          {/each}
          {#if !screens.length}<span class="hint">No screen information yet.</span>{/if}
        </div>
      </div>
      <div class="tools">
        {#if picked >= 0 && draft.items[picked]}
          <span class="hint">Drag on a screen to place <b style="color: {color(picked)}">{picked + 1}</b> (at least 2 × 2 boxes), or</span>
          {#each PRESETS as p}<button class="mini" class:on={presetOn(p)} onclick={() => preset(p)}>{p.name}</button>{/each}
        {:else}
          <span class="hint">Open the windows you usually work with, then press Capture open windows. Or add apps yourself, then drag on a screen to place them.</span>
        {/if}
      </div>
    </div>

    <div class="bottom">
      <div class="panel apps">
        <span class="lab">Apps in this setup</span>
        <div class="alist" role="listbox" aria-label="Apps in this setup">
          {#each draft.items as item, n}
            <div class="app" class:sel={n === picked} role="option" aria-selected={n === picked} tabindex="0" onclick={() => (picked = n)} onkeydown={(e) => e.key === 'Enter' && (picked = n)}>
              <span class="chip" style="--c: {color(n)}">{n + 1}</span>
              {#if n === picked}
                <span class="edit">
                  <span class="kinds" role="radiogroup" aria-label="It is">
                    {#each [['app', 'App'], ['url', 'Website']] as [v, t]}<button type="button" role="radio" aria-checked={item.kind === v} class:on={item.kind === v} onclick={() => (item.kind = v)}>{t}</button>{/each}
                  </span>
                  {#if item.kind === 'url'}
                    <input bind:value={item.target} placeholder="https://…" aria-label="Link" autocomplete="off" />
                    <input bind:value={item.open_with} placeholder="Browser (blank = default)" aria-label="Browser" autocomplete="off" />
                  {:else}
                    <AppInput bind:value={item.target} label="App {n + 1}" />
                  {/if}
                  <label class="another"><input type="checkbox" bind:checked={item.open_another} /><span>Open a new window</span></label>
                </span>
              {:else}
                <span class="txt"><span class="nm">{itemName(item)}</span>{#if itemSub(item)}<span class="sub">{itemSub(item)}</span>{/if}</span>
              {/if}
              <button class="del" aria-label="Remove item {n + 1}" title="Remove" onclick={(e) => { e.stopPropagation(); drop(n); }}><svg viewBox="0 0 24 24" aria-hidden="true"><path d="M6 6l12 12M18 6 6 18" /></svg></button>
            </div>
          {:else}
            <p class="hint pad">No apps yet. Press Add app, or Capture open windows.</p>
          {/each}
        </div>
      </div>

      <div class="panel form">
        <label><span class="lab">Profile name</span><input bind:value={draft.name} maxlength="40" autocomplete="off" placeholder="Project mode" /></label>
        <label><span class="lab">Voice command</span><input bind:value={draft.call_name} maxlength="80" autocomplete="off" placeholder="time to work" />
          <span class="hint">Say this or the name to launch. Commas separate several. A button can run it with a “Quick Launch” step.</span></label>
        <span class="grow"></span>
        {#if problem}<span class="problem" role="alert">{problem}</span>{:else if note}<span class="note" role="status">{note}</span>{/if}
        <div class="btns">
          {#if current}<button class="ghost bad" disabled={!!busy} onclick={async () => { if (await sure({ title: `Delete “${draft.name || 'this setup'}”?`, body: 'This Quick Launch setup will be removed. This cannot be undone.', yes: 'Delete setup' })) remove(); }}>Delete</button>{/if}
          <button class="ghost" disabled={!dirty || !!busy} onclick={discard}>Discard</button>
          <button class="save" disabled={!!busy || (!dirty && current !== '')} onclick={save}>{busy === 'save' ? 'Saving…' : 'Save setup'}</button>
        </div>
      </div>
    </div>
  </section>
</div></div>

{#if ask}
  <div class="scrim" role="presentation" onclick={() => (ask = null)}></div>
  <div class="ask" role="alertdialog" aria-modal="true" aria-labelledby="ask-t">
    <h4 id="ask-t">You have unsaved changes</h4>
    <p>You are in the middle of {current === '' ? 'creating a setup' : 'editing “' + (draft.name || 'this setup') + '”'}. Save it first, or discard it to {ask.to ? 'open another' : 'start a new one'}.</p>
    <div class="btns">
      <button class="ghost" onclick={() => (ask = null)}>Keep editing</button>
      <button class="ghost" onclick={() => { const to = ask?.to ?? null; ask = null; open(to); }}>Discard changes</button>
      <button class="save" disabled={!!busy || locked} onclick={async () => { const to = ask?.to ?? null; ask = null; await save(); if (!problem) open(to); }}>Save and continue</button>
    </div>
  </div>
{/if}

<style>
  .scrim { position: fixed; inset: 0; background: rgb(0 0 0 / 0.35); z-index: 50; }
  .ask { position: fixed; z-index: 51; left: 50%; top: 50%; transform: translate(-50%, -50%); width: min(420px, calc(100vw - 32px)); background: var(--surface, var(--bg)); color: var(--ink); border: 1px solid var(--line); border-radius: 18px; padding: 22px; box-shadow: 0 24px 60px rgb(0 0 0 / 0.3); }
  .ask h4 { margin: 0 0 8px; font-size: 17px; }
  .ask p { margin: 0 0 18px; font-size: 14px; line-height: 1.45; color: var(--muted); }
  .ask .btns { display: flex; flex-wrap: wrap; gap: 8px; justify-content: flex-end; }
  /* the layout follows the room the section has, not the window: a narrow window stacks it */
  .qlwrap { container-type: inline-size; container-name: ql; min-width: 0; flex: 1 0 auto; display: flex; flex-direction: column; }
  /* tall enough screens show everything at once; a shorter window keeps this height and the section scrolls */
  .ql { flex: 1; min-height: 590px; display: grid; grid-template-columns: minmax(240px, 320px) minmax(0, 1fr); grid-template-rows: minmax(0, 1fr); gap: 20px; align-items: stretch; min-width: 0; }
  .card { border: 1px solid var(--line); background: var(--surface); border-radius: var(--r-card); padding: 20px; min-width: 0; }
  .mono { font-family: var(--mono); }
  .lab { display: block; font: 500 11px/1.3 var(--mono); letter-spacing: .1em; text-transform: uppercase; color: var(--muted); }
  .hint { margin: 0; font-size: 12px; color: var(--muted); }
  .pad { padding: 12px 6px; }
  button:disabled { opacity: .4; cursor: default; }
  input:not([type='checkbox']) { height: 36px; min-width: 0; border-radius: var(--r-field); border: 1.5px solid var(--line); background: var(--surface); color: var(--ink); padding: 0 12px; font-size: 13px; outline: none; }
  input:not([type='checkbox']):focus-visible { border-color: var(--ink); }

  /* ---- left */
  .list { display: flex; flex-direction: column; gap: 16px; min-height: 0; }
  .find { display: flex; gap: 10px; }
  .search { flex: 1; border-radius: var(--r-pill) !important; font-family: var(--mono); font-size: 12px !important; padding: 0 16px !important; }
  .create { height: 36px; padding: 0 18px; border-radius: var(--r-pill); border: 0; background: var(--ink); color: var(--inv); font: 500 12px/1 var(--mono); white-space: nowrap; }
  .rows { flex: 1; min-height: 0; display: flex; flex-direction: column; border-top: 1px solid var(--line); overflow-y: auto; }
  .row { position: relative; display: flex; align-items: center; gap: 8px; border-bottom: 1px solid var(--line); padding: 12px 8px; }
  .row.on::after { content: ''; position: absolute; right: 0; top: 12px; bottom: 12px; width: 3px; border-radius: 2px; background: var(--signal); }
  .pick { flex: 1; min-width: 0; display: flex; align-items: center; gap: 12px; border: 0; background: transparent; padding: 0; text-align: left; color: var(--ink); }
  .ico { width: 36px; height: 36px; flex: none; border-radius: 6px; display: flex; align-items: center; justify-content: center; color: #fff; font-weight: 700; font-size: 14px; }
  .txt { display: flex; flex-direction: column; gap: 3px; min-width: 0; }
  .nm { font: 500 13px/1.25 var(--mono); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .vc { font: 400 11px/1.25 var(--mono); color: var(--ink); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .vc span { color: var(--muted); text-transform: uppercase; }
  .run { width: 28px; height: 28px; flex: none; margin-right: 8px; border-radius: 50%; border: 1px solid var(--line); background: var(--surface); color: var(--ink); display: flex; align-items: center; justify-content: center; padding: 0; opacity: 0; transition: opacity .16s var(--ease); }
  .row:hover .run, .run:focus-visible { opacity: 1; }
  .run svg { width: 12px; height: 12px; fill: currentColor; }

  /* ---- right */
  .config { container-type: inline-size; container-name: cfg; min-height: 0; display: flex; flex-direction: column; gap: 16px; transition: opacity .2s var(--ease), filter .2s var(--ease); }
  .config.off { opacity: .45; filter: grayscale(1); }
  .chead { display: flex; align-items: center; justify-content: space-between; gap: 12px; flex-wrap: wrap; }
  h3 { margin: 0; font-size: 22px; font-weight: 700; letter-spacing: -.02em; }
  .acts { display: flex; gap: 10px; }
  .pill { height: 36px; padding: 0 18px; border-radius: var(--r-pill); border: 1.5px solid var(--line); background: var(--surface); color: var(--ink); font: 500 12px/1 var(--mono); }
  .tipwrap { position: relative; display: inline-flex; }
  .tip { position: absolute; z-index: 20; top: calc(100% + 8px); right: 0; width: 250px; padding: 10px 12px; border-radius: 12px; background: var(--ink); color: var(--inv); font-size: 12px; line-height: 1.4; font-weight: 500; text-align: left; box-shadow: 0 8px 24px rgb(0 0 0 / .25); pointer-events: none; opacity: 0; transform: translateY(-3px); transition: opacity .15s var(--ease), transform .15s var(--ease); }
  .tipwrap:hover .tip, .tipwrap:focus-within .tip { opacity: 1; transform: none; transition-delay: .25s; }
  .pill.main { background: var(--ink); color: var(--inv); border-color: var(--ink); }
  .ghost { height: 38px; padding: 0 16px; border-radius: var(--r-pill); border: 1.5px solid var(--line); background: transparent; color: var(--muted); font: 500 12px/1 var(--mono); }
  .ghost.bad { color: var(--bad); }

  /* the screens sit on a pale tray, centred, as large as the panel lets them be */
  .board { flex: 1; min-height: 0; display: flex; flex-direction: column; align-items: center; gap: 10px; padding: 12px 18px 14px; border-radius: var(--r-card); background: var(--sunk); min-width: 0; overflow: hidden; }
  .bhead { align-self: stretch; display: flex; align-items: center; justify-content: space-between; gap: 10px; min-height: 26px; }
  .swap { flex: none; height: 26px; padding: 0 12px; border-radius: var(--r-pill); border: 1px solid var(--line); background: var(--surface); color: var(--ink); font-size: 11px; font-weight: 600; }
  /* the screens are sized from the strip, so they must not size it back (or each frame they shrink it a little
     more, which looks like an animation): centred on top of it, taking no room */
  .strip { flex: 1; min-height: 0; width: 100%; position: relative; }
  .stage { position: absolute; left: 50%; top: 50%; transform: translate(-50%, -50%); }
  .screen { position: absolute; border-radius: 6px; overflow: hidden; background-color: var(--surface); touch-action: none;
    background-image: linear-gradient(90deg, color-mix(in srgb, var(--ink) 9%, transparent) 1px, transparent 1px), linear-gradient(color-mix(in srgb, var(--ink) 9%, transparent) 1px, transparent 1px);
    background-size: calc(100% / 12) 100%, 100% calc(100% / 6);
    box-shadow: inset 0 -1px 6.1px rgb(0 0 0 / .25), inset 0 2px 4px rgb(0 0 0 / .25); }
  .screen.drawing { cursor: crosshair; }
  /* an app's box is a flat wash of its colour: no grid of its own */
  .win { position: absolute; box-sizing: border-box; border-radius: 5px; border: 1.5px solid var(--c); background: color-mix(in srgb, var(--c) 34%, transparent); display: flex; align-items: center; justify-content: center; pointer-events: none; }
  .win.sel { border-width: 2.5px; background: color-mix(in srgb, var(--c) 44%, transparent); z-index: 2; }
  .win.tiny .badge { width: 20px; height: 20px; font-size: 11px; border-width: 1px; }
  .badge { width: 32px; height: 32px; border-radius: 50%; border: 1.5px solid var(--c); color: var(--c); display: flex; align-items: center; justify-content: center; font-size: 15px; font-weight: 600; background: color-mix(in srgb, var(--surface) 55%, transparent); }
  .cap { position: absolute; font-size: 11px; color: var(--muted); text-transform: uppercase; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; text-align: center; }
  .role { color: var(--signal-ink); margin-right: 6px; }
  .role.sec { color: var(--muted); }

  .tools { display: flex; align-items: center; justify-content: center; flex-wrap: wrap; gap: 6px; }
  .mini { height: 26px; padding: 0 12px; border-radius: var(--r-pill); border: 1px solid var(--line); background: var(--surface); color: var(--ink); font-size: 11px; font-weight: 600; }
  .mini.on { background: var(--ink); color: var(--inv); border-color: var(--ink); }

  /* the apps and the name, side by side and of equal height */
  .bottom { flex: none; height: 270px; display: grid; grid-template-columns: minmax(0, 1.3fr) minmax(240px, 1fr); gap: 18px; align-items: stretch; }
  .panel { display: flex; flex-direction: column; gap: 10px; padding: 14px 16px; border: 1px solid var(--line); border-radius: var(--r-card); min-width: 0; min-height: 0; overflow: hidden; }
  .form { overflow-y: auto; }
  .form .btns, .form .note, .form .problem { flex: none; }
  .alist { flex: 1; min-height: 0; overflow-y: auto; margin: 0 -4px; padding: 0 4px; }
  .app { position: relative; display: flex; align-items: center; gap: 14px; padding: 10px 30px 10px 4px; border-bottom: 1px solid var(--hair); cursor: pointer; outline: none; }
  .app:last-child { border-bottom: 0; }
  .app.sel { align-items: flex-start; }
  .app:focus-visible { box-shadow: inset 0 0 0 2px var(--ink); border-radius: 6px; }
  .chip { width: 34px; height: 34px; flex: none; border-radius: 6px; border: 1.5px solid var(--c); background: color-mix(in srgb, var(--c) 34%, transparent); color: var(--c); display: flex; align-items: center; justify-content: center; font-size: 18px; font-weight: 600; }
  .app .txt .nm { font-size: 12px; }
  .sub { font: 400 11px/1.25 var(--mono); color: var(--muted); }
  .edit { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 8px; }
  .edit :global(input:not([type='checkbox'])) { width: 100%; box-sizing: border-box; }
  .kinds { display: flex; gap: 2px; padding: 3px; align-self: flex-start; border-radius: var(--r-pill); background: var(--hair); }
  .kinds button { height: 24px; padding: 0 12px; border: 0; border-radius: var(--r-pill); background: transparent; color: var(--muted); font-size: 11px; font-weight: 600; }
  .kinds button.on { background: var(--surface); color: var(--ink); box-shadow: var(--shadow-chosen); }
  .another { display: flex; flex-direction: row !important; align-items: center; gap: 8px !important; font-size: 12px; color: var(--muted); cursor: pointer; }
  .another input { margin: 0; width: 14px; height: 14px; accent-color: var(--ink); }
  .del { position: absolute; right: 2px; top: 14px; width: 24px; height: 24px; border-radius: 50%; border: 0; background: transparent; color: var(--muted); display: flex; align-items: center; justify-content: center; padding: 0; }
  .del:hover { color: var(--bad); }
  .del svg { width: 12px; height: 12px; fill: none; stroke: currentColor; stroke-width: 1.8; stroke-linecap: round; }

  .form label { display: flex; flex-direction: column; gap: 6px; }
  .form input:not([type='checkbox']) { border-radius: var(--r-pill); height: 38px; padding: 0 16px; }
  .grow { flex: 1; }
  .btns { display: flex; gap: 10px; }
  .btns button { flex: 1; }
  .save { height: 38px; padding: 0 16px; border-radius: var(--r-pill); border: 0; background: var(--signal); color: #fff; font: 500 13px/1 var(--mono); }
  .problem { font-size: 12px; color: var(--bad); }
  .note { font-size: 12px; color: var(--ink); }

  .chead h3 { flex: none; }
  .acts { flex-wrap: wrap; }
  @container ql (max-width: 820px) {
    .ql { grid-template-columns: 1fr; grid-template-rows: auto; min-height: 0; }
    .rows { max-height: 220px; }
    .strip { flex: none; height: 250px; }
  }
  @container cfg (max-width: 600px) {
    .bottom { grid-template-columns: 1fr; height: auto; }
    .alist { max-height: 220px; }
    .panel, .form { overflow: visible; }
    .chead { flex-direction: column; align-items: flex-start; }
    .board { padding: 12px; }
    .cap { font-size: 10px; }
  }
</style>
