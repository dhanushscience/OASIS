<script>
  import { trackUnsaved } from '../busy.svelte.js';
  import { onMount } from 'svelte';
  import { data, looksLikeSite } from '../profiles.svelte.js';
  import { quick, loadQuick } from '../quicksetups.svelte.js';
  import ActionField from './ActionField.svelte';
  import Select from './Select.svelte';
  import ScriptEditor from './ScriptEditor.svelte';

  /**
   * The automation builder: blocks on a canvas, joined by wires like n8n. Each block has two exits,
   * green "when it works" and red "when it fails"; "Button pressed" is joined to the first block.
   * Or one AutoHotkey script for the whole button (the Script side of the switch).
   */
  /** @typedef {{ kind: string, value: string, next?: number | null, on_fail?: number | null, x?: number | null, y?: number | null }} Step */
  /** @typedef {{ id: number, kind: string, value: string, x: number, y: number, ok: number | null, fail: number | null }} Block */
  /** @type {{ steps: Step[], button: number, onsave: (steps: Step[]) => void, onclose: () => void }} */
  let { steps, button, onsave, onclose } = $props();

  const W = 216, H = 80; // a block's size on the canvas
  const TRIGGER = { x: 40, y: 150, w: 150, h: 56 };
  const ICON = /** @type {Record<string, string>} */ ({
    QuickLaunch: '<rect x="3" y="3" width="8" height="8" rx="1.5"/><rect x="13" y="3" width="8" height="8" rx="1.5"/><rect x="3" y="13" width="8" height="8" rx="1.5"/><path d="M17 13v8M13 17h8"/>',
    LaunchApp: '<rect x="3" y="4" width="18" height="16" rx="2.5"/><path d="M3 9h18M6.5 6.5h.01M9.5 6.5h.01"/>',
    OpenUrl: '<circle cx="12" cy="12" r="9"/><path d="M3 12h18M12 3c2.5 2.6 3.8 5.6 3.8 9s-1.3 6.4-3.8 9c-2.5-2.6-3.8-5.6-3.8-9S9.5 5.6 12 3z"/>',
    Shortcut: '<rect x="2.5" y="6" width="19" height="12" rx="2.5"/><path d="M6 10h.01M10 10h.01M14 10h.01M18 10h.01M7.5 14h9"/>',
    AutoHotkey: '<path d="m8 8-4 4 4 4M16 8l4 4-4 4M13.5 5l-3 14"/>',
    PlaySound: '<path d="M4 9.5v5h3.5L12 18.5v-13L7.5 9.5z"/><path d="M15.5 9a4 4 0 0 1 0 6M18 6.5a7.5 7.5 0 0 1 0 11"/>',
  });
  const OTHER_ICON = '<circle cx="12" cy="12" r="8"/><path d="M12 8v4l2.5 2.5"/>';
  // What the palette offers, most used first.
  const BLOCKS = [
    { kind: 'QuickLaunch', name: 'Quick Launch', hint: 'Open a saved setup' },
    { kind: 'LaunchApp', name: 'Open app', hint: 'Start or bring forward' },
    { kind: 'OpenUrl', name: 'Open website', hint: 'In your browser' },
    { kind: 'Shortcut', name: 'Press keys', hint: 'Like Ctrl+T' },
    { kind: 'AutoHotkey', name: 'AutoHotkey script', hint: 'Anything a script can do' },
    { kind: 'PlaySound', name: 'Play sound', hint: 'Know by ear it is done' },
  ].filter((b) => data.writableStepTypes.includes(b.kind));
  const NAME = /** @type {Record<string, string>} */ ({ ...Object.fromEntries(BLOCKS.map((b) => [b.kind, b.name])), PlaySound: 'Play sound', Delay: 'Wait (old)', MouseClick: 'Mouse click', RunScript: 'PowerShell', PasteText: 'Paste text', JoinMeeting: 'Join meeting' });
  const OPENS = ['QuickLaunch', 'LaunchApp', 'OpenUrl'];

  // ---- the blocks: links by block id while editing, by place in the list once saved
  let uid = 0;
  /** @param {Step[]} list */
  function toBlocks(list) {
    const ids = list.map(() => ++uid);
    // A step saved as a plain list goes to the one below it; the builder shows that as a wire.
    const at = (/** @type {number | null | undefined} */ code, /** @type {number | null} */ auto) => {
      const i = code == null ? auto : code;
      return i != null && i >= 0 && i < ids.length ? ids[i] : null;
    };
    return list.map((s, i) => ({
      id: ids[i], kind: s.kind, value: s.value,
      x: s.x ?? 260 + (i % 3) * 270, y: s.y ?? 110 + Math.floor(i / 3) * 150,
      ok: at(s.next, i + 1), fail: at(s.on_fail, null),
    }));
  }
  // svelte-ignore state_referenced_locally
  const scriptOnly = steps.length === 1 && steps[0].kind === 'AutoHotkey' && steps[0].x == null;
  let mode = $state(scriptOnly ? 'script' : 'flow');
  // svelte-ignore state_referenced_locally
  let script = $state(scriptOnly ? steps[0].value : '');
  /** @type {Block[]} */
  // svelte-ignore state_referenced_locally
  let blocks = $state(scriptOnly ? [] : toBlocks(steps));
  /** the block "Button pressed" is joined to */
  // svelte-ignore state_referenced_locally
  let first = $state(/** @type {number | null} */ (blocks[0]?.id ?? null));
  // svelte-ignore state_referenced_locally
  let selected = $state(/** @type {number | null} */ (blocks[0]?.id ?? null));
  /** a wire picked to remove: its block and which exit ("start" is the Button pressed wire) */
  let wire = $state(/** @type {{ from: number | 'start', exit: 'ok' | 'fail' } | null} */ (null));
  let scriptFor = $state(/** @type {number | null} */ (null));
  let problem = $state('');
  let tried = $state(false);
  let askClose = $state(false);
  const byId = (/** @type {number | null} */ id) => blocks.find((b) => b.id === id);
  const sel = $derived(byId(selected));

  const { invoke } = window.__TAURI__?.core ?? { invoke: async () => [] };
  /** the sounds a block can play: [id, name, is a "worked" sound] */
  let sounds = $state(/** @type {[string, string, boolean][]} */ ([]));
  const play = (/** @type {string} */ id) => invoke('play_sound', { id }).catch(() => {});
  onMount(() => { loadQuick(); invoke('sound_list').then((/** @type {[string, string, boolean][]} */ l) => (sounds = l)).catch(() => {}); });

  // ---- saving: "Button pressed" leads to the first step in the list, the rest keep their order
  function toSteps() {
    if (mode === 'script') return [{ kind: 'AutoHotkey', value: script }];
    const start = byId(first);
    const order = start ? [start, ...blocks.filter((b) => b !== start)] : blocks;
    const at = new Map(order.map((b, i) => [b.id, i]));
    return order.map((b) => ({
      kind: b.kind, value: b.value,
      next: b.ok != null ? at.get(b.ok) ?? -1 : -1, on_fail: b.fail != null ? at.get(b.fail) ?? null : null,
      x: Math.round(b.x), y: Math.round(b.y),
    }));
  }
  // svelte-ignore state_referenced_locally
  const startedAs = JSON.stringify(toSteps());
  const dirty = $derived(JSON.stringify(toSteps()) !== startedAs);
  trackUnsaved('flow', () => dirty);

  /** Blocks "Button pressed" reaches; the others are drawn faded and never run. */
  const reached = $derived.by(() => {
    const seen = new Set();
    const go = [first];
    while (go.length) {
      const id = go.pop();
      if (id == null || seen.has(id)) continue;
      seen.add(id);
      const b = byId(id);
      if (b) go.push(b.ok, b.fail);
    }
    return seen;
  });
  const empty = (/** @type {Block} */ b) => data.writableStepTypes.includes(b.kind) && b.kind !== 'JoinMeeting' && !b.value.trim();

  function save() {
    tried = true;
    if (mode === 'script') {
      if (!script.trim()) return (problem = 'Write or paste the script first.');
    } else {
      if (!blocks.length) return (problem = 'Add a block from the left first.');
      if (first == null) return (problem = 'Join "Button pressed" to the block that should run first.');
      const missing = blocks.find((b) => reached.has(b.id) && empty(b));
      if (missing) { selected = missing.id; return (problem = `Fill in "${NAME[missing.kind] ?? missing.kind}" first.`); }
    }
    onsave(toSteps());
  }
  const close = () => (dirty ? (askClose = true) : onclose());

  // ---- adding and removing blocks
  /** @type {HTMLElement | undefined} */
  let canvasEl = $state();
  let pan = $state({ x: 0, y: 0 });
  let zoom = $state(1);
  const ZOOMS = [0.4, 0.5, 0.65, 0.8, 1, 1.25, 1.5, 2];
  /** Zoom about a point of the canvas (its middle by default), so what is under it stays put. */
  function zoomTo(/** @type {number} */ next, /** @type {number} */ cx = NaN, /** @type {number} */ cy = NaN) {
    const r = canvasEl?.getBoundingClientRect();
    const px = Number.isNaN(cx) ? (r ? r.width / 2 : 0) : cx, py = Number.isNaN(cy) ? (r ? r.height / 2 : 0) : cy;
    const z = Math.min(2, Math.max(0.4, next));
    pan = { x: px - ((px - pan.x) / zoom) * z, y: py - ((py - pan.y) / zoom) * z };
    zoom = z;
  }
  const zoomStep = (/** @type {1 | -1} */ dir) => zoomTo(dir > 0 ? ZOOMS.find((z) => z > zoom + 0.01) ?? 2 : [...ZOOMS].reverse().find((z) => z < zoom - 0.01) ?? 0.4);
  /** Fit every block (and the Button pressed card) in view. */
  function fit() {
    const r = canvasEl?.getBoundingClientRect();
    if (!r) return;
    const xs = [TRIGGER.x, TRIGGER.x + TRIGGER.w, ...blocks.flatMap((b) => [b.x, b.x + W])];
    const ys = [TRIGGER.y, TRIGGER.y + TRIGGER.h, ...blocks.flatMap((b) => [b.y, b.y + H])];
    const [x0, x1, y0, y1] = [Math.min(...xs), Math.max(...xs), Math.min(...ys), Math.max(...ys)];
    const z = Math.min(1.25, Math.max(0.4, Math.min((r.width - 100) / (x1 - x0), (r.height - 100) / (y1 - y0))));
    zoom = z;
    pan = { x: (r.width - (x1 - x0) * z) / 2 - x0 * z, y: (r.height - (y1 - y0) * z) / 2 - y0 * z };
  }
  function wheel(/** @type {WheelEvent} */ e) {
    e.preventDefault();
    const r = /** @type {HTMLElement} */ (canvasEl).getBoundingClientRect();
    if (e.ctrlKey || e.metaKey) zoomTo(zoom * Math.exp(-e.deltaY * 0.01), e.clientX - r.left, e.clientY - r.top);
    else pan = { x: pan.x - e.deltaX, y: pan.y - e.deltaY }; // a plain scroll moves about
  }
  $effect(() => {
    // a non-passive listener, or the page would zoom/scroll as well
    canvasEl?.addEventListener('wheel', wheel, { passive: false });
    return () => canvasEl?.removeEventListener('wheel', wheel);
  });
  /** @param {string} kind  @param {{ x: number, y: number }} [at] where the block's middle goes, on the canvas */
  function add(kind, at) {
    const prev = at ? undefined : byId(selected);
    const r = canvasEl?.getBoundingClientRect();
    // where it was dropped; else beside the selected block; else the middle of what is in view
    const x = at ? at.x - W / 2 : prev ? prev.x + W + 60 : ((r ? r.width / 2 : 300) - pan.x) / zoom - W / 2;
    const y = at ? at.y - H / 2 : prev ? prev.y : ((r ? r.height / 2 : 160) - pan.y) / zoom - H / 2;
    const b = { id: ++uid, kind, value: '', x: Math.round(x / 10) * 10, y: Math.round(y / 10) * 10, ok: null, fail: null };
    blocks.push(b);
    // A click builds a chain: the new block follows the selected one. A dropped block stays where it is put.
    if (prev && prev.ok == null) prev.ok = b.id;
    if (first == null) first = b.id;
    selected = b.id;
    problem = '';
    if (kind === 'AutoHotkey') scriptFor = b.id;
  }
  /** A sound after this block's exit: a chime for "when it works", a buzz for "when it fails". */
  function addSoundAfter(/** @type {Block} */ from, /** @type {'ok' | 'fail'} */ exit) {
    const id = exit === 'ok' ? 'chime' : 'buzz';
    const b = { id: ++uid, kind: 'PlaySound', value: id, x: from.x + W + 60, y: from.y + (exit === 'ok' ? -30 : 110), ok: null, fail: null };
    blocks.push(b);
    from[exit] = b.id;
    selected = b.id;
    play(id);
  }
  function remove(/** @type {number} */ id) {
    blocks = blocks.filter((b) => b.id !== id);
    for (const b of blocks) {
      if (b.ok === id) b.ok = null;
      if (b.fail === id) b.fail = null;
    }
    if (first === id) first = null;
    if (selected === id) selected = null;
  }
  function cut(/** @type {{ from: number | 'start', exit: 'ok' | 'fail' }} */ w) {
    if (w.from === 'start') first = null;
    else { const b = byId(w.from); if (b) b[w.exit] = null; }
    wire = null;
  }

  // ---- dragging: the canvas pans, a block moves, an exit dot draws a wire
  /** @type {{ type: 'pan' | 'block' | 'wire', id?: number | 'start', exit?: 'ok' | 'fail', sx: number, sy: number, ox: number, oy: number, moved: boolean } | null} */
  let drag = null;
  let ghost = $state(/** @type {{ x1: number, y1: number, x2: number, y2: number, exit: string } | null} */ (null));
  const world = (/** @type {PointerEvent} */ e) => {
    const r = canvasEl?.getBoundingClientRect() ?? { left: 0, top: 0 };
    return { x: (e.clientX - r.left - pan.x) / zoom, y: (e.clientY - r.top - pan.y) / zoom };
  };
  function downCanvas(/** @type {PointerEvent} */ e) {
    if (e.target !== canvasEl && !(e.target instanceof SVGSVGElement)) return;
    drag = { type: 'pan', sx: e.clientX, sy: e.clientY, ox: pan.x, oy: pan.y, moved: false };
    selected = null;
    wire = null;
  }
  function downBlock(/** @type {PointerEvent} */ e, /** @type {Block} */ b) {
    if (e.button !== 0 || /** @type {HTMLElement} */ (e.target).closest('.port')) return;
    drag = { type: 'block', id: b.id, sx: e.clientX, sy: e.clientY, ox: b.x, oy: b.y, moved: false };
    selected = b.id;
    wire = null;
  }
  function downPort(/** @type {PointerEvent} */ e, /** @type {number | 'start'} */ id, /** @type {'ok' | 'fail'} */ exit) {
    e.stopPropagation();
    const p = id === 'start' ? startOut() : out(/** @type {Block} */ (byId(id)), exit);
    drag = { type: 'wire', id, exit, sx: e.clientX, sy: e.clientY, ox: 0, oy: 0, moved: false };
    ghost = { x1: p.x, y1: p.y, x2: p.x, y2: p.y, exit };
  }
  /** a block being dragged in from the list: what kind, and where the pointer is (screen) */
  let carry = $state(/** @type {{ kind: string, x: number, y: number, moved: boolean, sx: number, sy: number } | null} */ (null));
  function downPick(/** @type {PointerEvent} */ e, /** @type {string} */ kind) {
    if (e.button !== 0) return;
    carry = { kind, x: e.clientX, y: e.clientY, sx: e.clientX, sy: e.clientY, moved: false };
  }
  function move(/** @type {PointerEvent} */ e) {
    if (carry) { carry.x = e.clientX; carry.y = e.clientY; if (Math.abs(e.clientX - carry.sx) + Math.abs(e.clientY - carry.sy) > 5) carry.moved = true; }
    if (!drag) return;
    const dx = e.clientX - drag.sx, dy = e.clientY - drag.sy;
    if (Math.abs(dx) + Math.abs(dy) > 3) drag.moved = true;
    if (drag.type === 'pan') pan = { x: drag.ox + dx, y: drag.oy + dy };
    else if (drag.type === 'block') { const b = byId(/** @type {number} */ (drag.id)); if (b) { b.x = drag.ox + dx / zoom; b.y = drag.oy + dy / zoom; } }
    else if (ghost) { const w = world(e); ghost.x2 = w.x; ghost.y2 = w.y; }
  }
  /** set when a drag from the list has just ended, so the click that follows it does not add a second block */
  let justDropped = false;
  function up(/** @type {PointerEvent} */ e) {
    if (carry) {
      const c = carry;
      carry = null;
      justDropped = c.moved;
      setTimeout(() => (justDropped = false), 0);
      const r = canvasEl?.getBoundingClientRect();
      const inside = r && e.clientX >= r.left && e.clientX <= r.right && e.clientY >= r.top && e.clientY <= r.bottom;
      // dropped on the canvas: it lands there; a plain click is handled by the button itself
      if (c.moved && inside) add(c.kind, { x: (e.clientX - r.left - pan.x) / zoom, y: (e.clientY - r.top - pan.y) / zoom });
      return;
    }
    if (drag?.type === 'block') { const b = byId(/** @type {number} */ (drag.id)); if (b) { b.x = Math.round(b.x / 10) * 10; b.y = Math.round(b.y / 10) * 10; } }
    if (drag?.type === 'wire') {
      const el = /** @type {HTMLElement | null} */ (document.elementFromPoint(e.clientX, e.clientY)?.closest('[data-block]') ?? null);
      const to = el ? Number(el.dataset.block) : null;
      if (to != null && to !== drag.id) {
        if (drag.id === 'start') first = to;
        else { const b = byId(drag.id ?? null); if (b && drag.exit) b[drag.exit] = to; }
        problem = '';
      }
      ghost = null;
    }
    drag = null;
  }

  // ---- where the dots are, and the wires between them
  const inPort = (/** @type {Block} */ b) => ({ x: b.x, y: b.y + H / 2 });
  const out = (/** @type {Block} */ b, /** @type {'ok' | 'fail'} */ exit) => ({ x: b.x + W, y: b.y + (exit === 'ok' ? 28 : 56) });
  const startOut = () => ({ x: TRIGGER.x + TRIGGER.w, y: TRIGGER.y + TRIGGER.h / 2 });
  const path = (/** @type {{x:number,y:number}} */ a, /** @type {{x:number,y:number}} */ b) => {
    const c = Math.max(50, Math.abs(b.x - a.x) / 2);
    return `M${a.x},${a.y} C${a.x + c},${a.y} ${b.x - c},${b.y} ${b.x},${b.y}`;
  };
  const wires = $derived.by(() => {
    /** @type {{ from: number | 'start', exit: 'ok' | 'fail', a: {x:number,y:number}, b: {x:number,y:number} }[]} */
    const list = [];
    const s = byId(first);
    if (s) list.push({ from: 'start', exit: 'ok', a: startOut(), b: inPort(s) });
    for (const b of blocks) {
      for (const exit of /** @type {const} */ (['ok', 'fail'])) {
        const t = byId(b[exit]);
        if (t) list.push({ from: b.id, exit, a: out(b, exit), b: inPort(t) });
      }
    }
    return list;
  });

  // ---- what a block shows
  function summary(/** @type {Block} */ b) {
    const v = b.value.trim();
    if (b.kind === 'QuickLaunch') return quick.list.find((/** @type {any} */ q) => q.id === v)?.name ?? (v ? '(removed setup)' : 'Pick a setup');
    if (b.kind === 'AutoHotkey') return v.split('\n').find((l) => l.trim())?.trim() ?? 'Write the script';
    if (b.kind === 'OpenUrl') return v.replace(/^https?:\/\//i, '').replace(/\/$/, '') || 'Type a website';
    if (b.kind === 'LaunchApp') return v.split(/[\\/]/).pop()?.replace(/\.exe$/i, '') || 'Pick an app';
    if (b.kind === 'Shortcut') return v || 'Record the keys';
    if (b.kind === 'PlaySound') return sounds.find((x) => x[0] === v)?.[1] ?? 'Pick a sound';
    if (b.kind === 'Delay') return `${(Number(v) || 0) / 1000} s`;
    return v || '—';
  }
  const label = (/** @type {Block} */ b) => `${NAME[b.kind] ?? b.kind}: ${summary(b)}`;

  function key(/** @type {KeyboardEvent} */ e) {
    if (scriptFor != null) return; // the script editor has the keys
    const typing = /** @type {HTMLElement} */ (document.activeElement)?.matches?.('input, textarea, select');
    if (e.key === 'Escape') { if (typing) /** @type {HTMLElement} */ (document.activeElement).blur(); else if (askClose) askClose = false; else close(); return; }
    if ((e.key === 'Delete' || e.key === 'Backspace') && !typing) {
      if (wire) { cut(wire); e.preventDefault(); } else if (selected != null && mode === 'flow') { remove(selected); e.preventDefault(); }
    }
    if (e.key === 's' && e.ctrlKey) { e.preventDefault(); save(); }
  }
  const go = (/** @type {Block} */ b, /** @type {'ok' | 'fail'} */ exit, /** @type {string} */ v) => { b[exit] = v === '' ? null : Number(v); };
</script>

<svelte:window onkeydown={key} onpointermove={move} onpointerup={up} />

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="scrim" onclick={close}></div>
<div class="sheet" role="dialog" tabindex="-1" aria-modal="true" aria-label="Steps for button {button + 1}">
  <header>
    <div class="title"><span class="lab">Button {button + 1} · Steps</span><h3>What happens when it is pressed</h3></div>
    <div class="switch" role="radiogroup" aria-label="Build with">
      <button role="radio" aria-checked={mode === 'flow'} class:on={mode === 'flow'} onclick={() => (mode = 'flow')}>Blocks</button>
      <button role="radio" aria-checked={mode === 'script'} class:on={mode === 'script'} onclick={() => (mode = 'script')}>Script</button>
    </div>
    <button class="x" aria-label="Close" onclick={close}><svg viewBox="0 0 24 24"><path d="M6 6l12 12M18 6 6 18"/></svg></button>
  </header>

  {#if mode === 'flow'}
    <div class="body">
      <nav class="palette" aria-label="Blocks">
        <span class="lab">Add a block</span>
        {#each BLOCKS as b}
          <button class="pick" title="Click to add, or drag it onto the canvas" onpointerdown={(e) => downPick(e, b.kind)} onclick={() => { if (!justDropped) add(b.kind); }}>
            <span class="ico"><svg viewBox="0 0 24 24">{@html ICON[b.kind]}</svg></span>
            <span class="pt"><b>{b.name}</b><small>{b.hint}</small></span>
          </button>
        {/each}
        <p class="tip">Click a block to add it after the selected one, or drag it onto the canvas. Drag from a dot to join blocks.</p>
      </nav>

      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div class="canvas" bind:this={canvasEl} onpointerdown={downCanvas} style="background-position: {pan.x}px {pan.y}px; background-size: {20 * zoom}px {20 * zoom}px">
        <div class="world" style="transform: translate({pan.x}px, {pan.y}px) scale({zoom})">
          <svg class="wires" aria-hidden="true">
            {#each wires as w (`${w.from}-${w.exit}`)}
              {@const on = wire?.from === w.from && wire?.exit === w.exit}
              <path class="hit" d={path(w.a, w.b)} onpointerdown={(e) => { e.stopPropagation(); wire = { from: w.from, exit: w.exit }; selected = null; }} />
              <path class="wire {w.exit}" class:on d={path(w.a, w.b)} />
            {/each}
            {#if ghost}<path class="wire {ghost.exit} ghost" d={path({ x: ghost.x1, y: ghost.y1 }, { x: ghost.x2, y: ghost.y2 })} />{/if}
          </svg>
          {#if wire}
            {@const w = wires.find((x) => x.from === wire?.from && x.exit === wire?.exit)}
            {#if w}<button class="cut" style="left: {(w.a.x + w.b.x) / 2}px; top: {(w.a.y + w.b.y) / 2}px" aria-label="Remove this link" onclick={() => wire && cut(wire)}>×</button>{/if}
          {/if}

          <div class="trigger" style="left: {TRIGGER.x}px; top: {TRIGGER.y}px; width: {TRIGGER.w}px; height: {TRIGGER.h}px">
            <span class="bolt"><svg viewBox="0 0 24 24"><path d="M13 3 5 13h6l-1 8 8-10h-6z"/></svg></span>
            <span><b>Button {button + 1}</b><small>pressed</small></span>
            <!-- svelte-ignore a11y_no_static_element_interactions -->
            <span class="port ok out" title="Drag to the first block" onpointerdown={(e) => downPort(e, 'start', 'ok')}></span>
          </div>

          {#each blocks as b (b.id)}
            <!-- svelte-ignore a11y_no_static_element_interactions -->
            <div class="block" data-block={b.id} class:sel={selected === b.id} class:off={!reached.has(b.id)} class:miss={tried && empty(b) && reached.has(b.id)}
              style="left: {b.x}px; top: {b.y}px; width: {W}px; height: {H}px" onpointerdown={(e) => downBlock(e, b)}>
              <span class="port in" aria-hidden="true"></span>
              <span class="ico"><svg viewBox="0 0 24 24">{@html ICON[b.kind] ?? OTHER_ICON}</svg></span>
              <span class="bt"><small>{NAME[b.kind] ?? b.kind}</small><b>{summary(b)}</b></span>
              {#if !reached.has(b.id)}<em class="tag">not joined</em>{/if}
              <!-- svelte-ignore a11y_no_static_element_interactions -->
              <span class="port ok out" title="When it works: drag to the next block" onpointerdown={(e) => downPort(e, b.id, 'ok')}></span>
              <!-- svelte-ignore a11y_no_static_element_interactions -->
              <span class="port fail out" title="When it fails: drag to another block" onpointerdown={(e) => downPort(e, b.id, 'fail')}></span>
            </div>
          {/each}
        </div>
        {#if !blocks.length}
          <div class="hint"><b>Start with a block on the left.</b><span>Quick Launch, an app, a website, keys or a script. Each one waits until the one before it is ready.</span></div>
        {/if}
        <div class="legend" aria-hidden="true"><span><i class="dot ok"></i>when it works</span><span><i class="dot fail"></i>when it fails</span></div>
        <div class="zoom" role="group" aria-label="Zoom">
          <button aria-label="Zoom out" title="Zoom out" disabled={zoom <= 0.4} onclick={() => zoomStep(-1)}>−</button>
          <button class="pct mono" title="Fit everything in view" aria-label="Fit everything in view" onclick={fit}>{Math.round(zoom * 100)}%</button>
          <button aria-label="Zoom in" title="Zoom in" disabled={zoom >= 2} onclick={() => zoomStep(1)}>+</button>
        </div>
      </div>

      <aside class="inspector" aria-label="Selected block">
        {#if sel}
          <div class="ihead">
            <span class="ico"><svg viewBox="0 0 24 24">{@html ICON[sel.kind] ?? OTHER_ICON}</svg></span>
            <b>{NAME[sel.kind] ?? sel.kind}</b>
            <button class="del" onclick={() => sel && remove(sel.id)}>Remove</button>
          </div>
          {#if sel.kind === 'QuickLaunch'}
            <label class="f"><span class="lab">Setup</span>
              <Select value={sel.value} label="Setup" onchange={(/** @type {string} */ v) => sel && (sel.value = v)}
                options={[
                  { value: '', label: 'Pick a setup' },
                  ...quick.list.map((/** @type {any} */ q) => ({ value: q.id, label: q.name })),
                  ...(sel.value && !quick.list.some((/** @type {any} */ q) => q.id === sel?.value) ? [{ value: sel.value, label: '(removed setup)' }] : []),
                ]} />
            </label>
          {:else if sel.kind === 'LaunchApp' || sel.kind === 'OpenUrl'}
            {#key sel.id}
              <ActionField value={sel.value} seg={1} kind={sel.kind} readonly={false} label={sel.kind === 'OpenUrl' ? 'Website' : 'App'} placeholder="Type an app or a site" help="Pick from the list, or type a name, a path or an address."
                onedit={(p) => { if (!sel) return; if (p.value != null) sel.value = p.value; sel.kind = p.kind || (looksLikeSite(sel.value) ? 'OpenUrl' : 'LaunchApp'); }} />
            {/key}
          {:else if sel.kind === 'Shortcut'}
            {#key sel.id}
              <ActionField value={sel.value} seg={0} kind="Shortcut" readonly={false} label="Keys" placeholder="Ctrl+T" help="Click the box, then press the keys."
                onedit={(p) => { if (sel && p.value != null) sel.value = p.value; }} />
            {/key}
          {:else if sel.kind === 'AutoHotkey'}
            <div class="f"><span class="lab">Script</span>
              <button class="scriptbtn mono" onclick={() => (scriptFor = sel?.id ?? null)}>
                <span>{summary(sel)}</span><svg class="pen" viewBox="0 0 24 24" aria-hidden="true"><path d="M4 20h4L19 9l-4-4L4 16zM13.5 6.5l4 4"/></svg>
              </button>
            </div>
          {:else if sel.kind === 'PlaySound'}
            <div class="f sounds">
              {#each [[true, 'When it worked'], [false, 'When it went wrong']] as [good, title]}
                <span class="lab"><i class="dot {good ? 'ok' : 'fail'}"></i>{title}</span>
                {#each sounds.filter((x) => x[2] === good) as [id, name]}
                  <div class="snd" class:on={sel.value === id}>
                    <button class="choose" role="radio" aria-checked={sel.value === id} onclick={() => { if (sel) sel.value = id; play(id); }}>
                      <span class="tick" aria-hidden="true"></span>{name}</button>
                    <button class="listen" aria-label="Listen to {name}" title="Listen" onclick={() => play(id)}><svg viewBox="0 0 24 24" aria-hidden="true">{@html ICON.PlaySound}</svg></button>
                  </div>
                {/each}
              {/each}
            </div>
          {:else if data.writableStepTypes.includes(sel.kind)}
            <label class="f"><span class="lab">Value</span><input class="mono" value={sel.value} oninput={(e) => sel && (sel.value = e.currentTarget.value)} /></label>
          {:else}
            <p class="note">Made by an AI plan; it runs as it is. You can move it and join it.</p>
          {/if}

          <label class="f"><span class="lab"><i class="dot ok"></i>When it works</span>
            <Select value={sel.ok ?? ''} label="When it works" onchange={(/** @type {string} */ v) => sel && go(sel, 'ok', v)}
              options={[{ value: '', label: 'Finish here' }, ...blocks.filter((b) => b.id !== sel?.id).map((b) => ({ value: b.id, label: label(b) }))]} />
            {#if sel.ok == null && sel.kind !== 'PlaySound'}<button class="addsnd" onclick={() => sel && addSoundAfter(sel, 'ok')}>+ Play a "done" sound</button>{/if}
          </label>
          <label class="f"><span class="lab"><i class="dot fail"></i>When it fails</span>
            <Select value={sel.fail ?? ''} label="When it fails" onchange={(/** @type {string} */ v) => sel && go(sel, 'fail', v)}
              options={[{ value: '', label: 'Stop and tell me' }, ...blocks.map((b) => ({ value: b.id, label: b.id === sel?.id ? 'Try this block again' : label(b) }))]} />
            {#if sel.fail == null && sel.kind !== 'PlaySound'}<button class="addsnd" onclick={() => sel && addSoundAfter(sel, 'fail')}>+ Play an "error" sound</button>{/if}
          </label>
          {#if OPENS.includes(sel.kind)}<p class="note">The next block waits until this has opened, up to 15 seconds. If it does not open, the "when it fails" path is taken.</p>{/if}
        {:else}
          <div class="how">
            <span class="lab">How it works</span>
            <ol>
              <li>Click a block on the left. Each new one joins after the selected block.</li>
              <li>Drag from a dot to join blocks: <i class="dot ok"></i> when it works, <i class="dot fail"></i> when it fails.</li>
              <li>No waiting to set: each block waits until the one before it is ready.</li>
            </ol>
            <span class="keys mono">Ctrl+scroll zooms · Delete removes · Ctrl+S saves</span>
          </div>
        {/if}
      </aside>
    </div>
  {:else}
    <div class="scriptmode">
      <textarea class="code" bind:value={script} spellcheck="false" autocomplete="off" wrap="off"
        placeholder={'; One AutoHotkey v2 script for the whole button. For example:\nRun "chrome.exe"\nWinWaitActive "ahk_exe chrome.exe"\nSend "^t"'}></textarea>
      <ul class="rules">
        <li>Written for <b>AutoHotkey v2</b>, installed for all users (the normal installer).</li>
        <li>Runs only when <b>Allow scripts</b> is on in Settings. Never from voice or the browser.</li>
        <li>Stops after one minute, so a script left running does not stay hidden.</li>
      </ul>
    </div>
  {/if}

  <footer>
    {#if askClose}
      <span class="ask">Close without saving these changes?</span>
      <button class="ghost" onclick={() => (askClose = false)}>Keep editing</button>
      <button class="danger" onclick={onclose}>Discard</button>
    {:else}
      <span class="problem" role="alert">{problem}</span>
      <button class="ghost" onclick={close}>Cancel</button>
      <button class="save" onclick={save}>Save steps</button>
    {/if}
  </footer>
</div>

{#if carry?.moved}
  <div class="carried" style="left: {carry.x}px; top: {carry.y}px"><span class="ico"><svg viewBox="0 0 24 24">{@html ICON[carry.kind]}</svg></span><b>{NAME[carry.kind]}</b></div>
{/if}

{#if scriptFor != null && byId(scriptFor)}
  {@const b = /** @type {Block} */ (byId(scriptFor))}
  <ScriptEditor value={b.value} step={blocks.indexOf(b) + 1} onsave={(t) => { b.value = t; scriptFor = null; }} onclose={() => (scriptFor = null)} />
{/if}

<style>
  .scrim { position: fixed; inset: 0; z-index: 40; background: rgba(0, 0, 0, .3); animation: fade .2s var(--ease); }
  .sheet { position: fixed; z-index: 41; inset: 20px; display: flex; flex-direction: column; border-radius: var(--r-bar); background: var(--surface); box-shadow: 0 16px 48px var(--shadow); overflow: hidden; outline: none; animation: rise .26s var(--ease); }
  header { display: flex; align-items: center; gap: 16px; padding: 16px 20px; border-bottom: 1px solid var(--line); }
  .title { flex: 1; min-width: 0; }
  .lab { font: 500 11px/1.3 var(--mono); letter-spacing: .12em; text-transform: uppercase; color: var(--muted); display: inline-flex; align-items: center; gap: 6px; }
  h3 { margin: 3px 0 0; font-size: 18px; font-weight: 700; letter-spacing: -.01em; }
  .switch { display: flex; gap: 4px; padding: 4px; border-radius: var(--r-pill); background: var(--hair); }
  .switch button { height: 34px; padding: 0 18px; border: 0; border-radius: var(--r-pill); background: transparent; color: var(--muted); font-size: 13px; font-weight: 600; }
  .switch button.on { background: var(--surface); color: var(--ink); box-shadow: var(--shadow-chosen); }
  .x { flex: none; width: 34px; height: 34px; display: grid; place-items: center; border: 0; border-radius: 50%; background: var(--hair); color: var(--ink); }
  .x svg { width: 16px; height: 16px; fill: none; stroke: currentColor; stroke-width: 2; stroke-linecap: round; }

  .body { flex: 1; min-height: 0; display: grid; grid-template-columns: 200px minmax(0, 1fr) 290px; }
  .palette { display: flex; flex-direction: column; gap: 6px; padding: 16px 12px; border-right: 1px solid var(--line); overflow: auto; }
  .palette .lab { margin: 0 6px 4px; }
  .pick { display: flex; align-items: center; gap: 10px; padding: 8px; border: 1px solid transparent; border-radius: var(--r-row); background: transparent; color: var(--ink); text-align: left; transition: background-color .2s var(--ease), border-color .2s var(--ease); }
  .pick:hover { background: var(--sunk, var(--hair)); border-color: var(--line); }
  .pt { display: flex; flex-direction: column; min-width: 0; }
  .pt b { font-size: 13px; font-weight: 600; }
  .pt small { font-size: 11px; color: var(--muted); }
  .ico { flex: none; width: 32px; height: 32px; display: grid; place-items: center; border-radius: 9px; background: var(--hair); color: var(--ink); }
  .ico svg { width: 17px; height: 17px; fill: none; stroke: currentColor; stroke-width: 1.8; stroke-linecap: round; stroke-linejoin: round; }
  .tip { margin: auto 6px 0; font-size: 11.5px; line-height: 1.45; color: var(--muted); }

  .canvas { position: relative; overflow: hidden; cursor: grab; touch-action: none; background-color: var(--sunk, var(--hair)); background-image: radial-gradient(circle, var(--faint) 1px, transparent 1.2px); background-size: 20px 20px; }
  .canvas:active { cursor: grabbing; }
  .world { position: absolute; left: 0; top: 0; width: 0; height: 0; }
  .wires { position: absolute; left: 0; top: 0; width: 1px; height: 1px; overflow: visible; }
  .wire { fill: none; stroke-width: 2; pointer-events: none; transition: stroke-width .15s var(--ease); }
  .wire.ok { stroke: var(--ink); opacity: .55; }
  .wire.fail { stroke: var(--bad); stroke-dasharray: 6 5; opacity: .8; }
  .wire.on { stroke-width: 3.5; opacity: 1; }
  .wire.ghost { opacity: .9; }
  .hit { fill: none; stroke: transparent; stroke-width: 14; cursor: pointer; pointer-events: stroke; }
  .cut { position: absolute; z-index: 3; width: 24px; height: 24px; margin: -12px 0 0 -12px; padding: 0; border: 0; border-radius: 50%; background: var(--ink); color: var(--inv); font-size: 15px; line-height: 1; }

  .trigger, .block { position: absolute; display: flex; align-items: center; gap: 10px; padding: 0 16px 0 12px; border-radius: 14px; background: var(--surface); box-shadow: 0 1px 2px var(--shadow), 0 6px 18px color-mix(in srgb, var(--shadow) 60%, transparent); }
  .trigger { border: 1.5px solid var(--ink); font-size: 13px; }
  .trigger span:nth-child(2) { display: flex; flex-direction: column; line-height: 1.2; }
  .trigger small { color: var(--muted); }
  .bolt { width: 30px; height: 30px; display: grid; place-items: center; border-radius: 50%; background: var(--ink); color: var(--inv); }
  .bolt svg { width: 15px; height: 15px; fill: currentColor; }
  .block { border: 1.5px solid var(--line); cursor: grab; user-select: none; transition: box-shadow .2s var(--ease), border-color .2s var(--ease), opacity .2s var(--ease); }
  .block:hover { border-color: var(--faint); }
  .block.sel { border-color: var(--ink); box-shadow: 0 0 0 3px color-mix(in srgb, var(--ink) 14%, transparent), 0 8px 22px var(--shadow); }
  .block.off { opacity: .55; }
  .block.miss { border-color: var(--bad); }
  .bt { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 2px; }
  .bt small { font: 500 10px/1.2 var(--mono); letter-spacing: .1em; text-transform: uppercase; color: var(--muted); }
  .bt b { font-size: 13px; font-weight: 600; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .tag { position: absolute; top: -9px; left: 12px; padding: 1px 8px; border-radius: var(--r-pill); background: var(--hair); color: var(--muted); font: 600 10px/1.4 var(--sans); font-style: normal; }
  .port { position: absolute; width: 14px; height: 14px; border-radius: 50%; border: 2px solid var(--surface); box-shadow: 0 0 0 1px var(--line); }
  .port.in { left: -8px; top: calc(50% - 7px); background: var(--faint); }
  .port.out { right: -8px; cursor: crosshair; transition: transform .15s var(--ease); }
  .port.out:hover { transform: scale(1.35); }
  .port.ok { background: var(--ok); }
  .port.fail { background: var(--bad); }
  .trigger .port.out { top: calc(50% - 7px); }
  .block .port.ok { top: 21px; }
  .block .port.fail { top: 49px; }
  .hint { position: absolute; left: 50%; top: 50%; transform: translate(-50%, -50%); max-width: 320px; display: flex; flex-direction: column; gap: 4px; text-align: center; font-size: 13px; color: var(--muted); pointer-events: none; }
  .hint b { color: var(--ink); font-size: 15px; }
  .legend { position: absolute; left: 14px; bottom: 12px; display: flex; gap: 14px; padding: 6px 12px; border-radius: var(--r-pill); background: var(--surface); font-size: 11.5px; color: var(--muted); box-shadow: 0 1px 2px var(--shadow); }
  .zoom { position: absolute; right: 14px; bottom: 12px; display: flex; align-items: center; padding: 3px; border-radius: var(--r-pill); background: var(--surface); box-shadow: 0 1px 2px var(--shadow); }
  .zoom button { min-width: 32px; height: 30px; padding: 0 8px; border: 0; border-radius: var(--r-pill); background: transparent; color: var(--ink); font-size: 16px; font-weight: 600; }
  .zoom button:hover:not(:disabled) { background: var(--hair); }
  .zoom .pct { min-width: 54px; font-size: 12px; color: var(--muted); }
  .carried { position: fixed; z-index: 60; transform: translate(-50%, -50%); display: flex; align-items: center; gap: 10px; padding: 8px 14px 8px 8px; border-radius: 12px; background: var(--surface); border: 1.5px solid var(--ink); box-shadow: 0 10px 28px var(--shadow); pointer-events: none; font-size: 13px; }
  .pick { touch-action: none; }
  .sounds { gap: 4px; }
  .sounds .lab { margin: 8px 0 2px; }
  .snd { display: flex; align-items: center; gap: 6px; border-radius: var(--r-row); border: 1.5px solid transparent; transition: background-color .2s var(--ease), border-color .2s var(--ease); }
  .snd:hover { background: var(--hair); }
  .snd.on { border-color: var(--ink); background: var(--sunk, var(--hair)); }
  .choose { flex: 1; min-width: 0; display: flex; align-items: center; gap: 10px; height: 36px; padding: 0 10px; border: 0; background: transparent; color: var(--ink); font-size: 13px; font-weight: 600; text-align: left; }
  .tick { width: 14px; height: 14px; flex: none; border-radius: 50%; border: 1.5px solid var(--faint); }
  .snd.on .tick { border: 4px solid var(--ink); }
  .listen { width: 32px; height: 32px; flex: none; display: grid; place-items: center; border: 0; border-radius: 50%; background: transparent; color: var(--muted); }
  .listen:hover { background: var(--surface); color: var(--ink); }
  .listen svg { width: 16px; height: 16px; fill: none; stroke: currentColor; stroke-width: 1.8; stroke-linecap: round; stroke-linejoin: round; }
  .addsnd { align-self: flex-start; height: 28px; padding: 0 12px; border-radius: var(--r-pill); border: 1px dashed var(--faint); background: transparent; color: var(--muted); font-size: 12px; font-weight: 600; }
  .addsnd:hover { color: var(--ink); border-color: var(--ink); }
  .legend span { display: inline-flex; align-items: center; gap: 6px; }
  .dot { display: inline-block; width: 9px; height: 9px; border-radius: 50%; vertical-align: middle; }
  .dot.ok { background: var(--ok); }
  .dot.fail { background: var(--bad); }

  .inspector { display: flex; flex-direction: column; gap: 14px; padding: 16px; border-left: 1px solid var(--line); overflow: auto; }
  .ihead { display: flex; align-items: center; gap: 10px; }
  .ihead b { flex: 1; font-size: 15px; }
  .del { height: 28px; padding: 0 12px; border-radius: var(--r-pill); border: 1px solid var(--line); background: transparent; color: var(--bad); font-size: 12px; font-weight: 600; }
  .f { display: flex; flex-direction: column; gap: 6px; }
  input { height: 40px; border-radius: var(--r-row); border: 1.5px solid var(--line); background: var(--surface); color: var(--ink); padding: 0 12px; font-size: 13px; outline: none; min-width: 0; }
  input:focus-visible { border-color: var(--ink); }
  .note { margin: 0; font-size: 12px; line-height: 1.45; color: var(--muted); }
  .how ol { margin: 8px 0 12px; padding-left: 18px; display: flex; flex-direction: column; gap: 8px; font-size: 12.5px; line-height: 1.45; color: var(--muted); }
  .keys { font-size: 11px; color: var(--muted); }
  .mono { font-family: var(--mono); }
  .scriptbtn { height: 40px; display: flex; align-items: center; gap: 8px; padding: 0 4px; border: 0; border-bottom: 2px dotted var(--faint); border-radius: 0; background: transparent; color: var(--ink); font-size: 12.5px; text-align: left; }
  .scriptbtn span { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .scriptbtn:hover { border-bottom-color: var(--ink); }
  .pen { width: 16px; height: 16px; fill: none; stroke: currentColor; stroke-width: 1.8; stroke-linecap: round; stroke-linejoin: round; }

  .scriptmode { flex: 1; min-height: 0; display: flex; flex-direction: column; gap: 12px; padding: 18px 20px; }
  .code { flex: 1; resize: none; padding: 14px 16px; border-radius: var(--r-row); border: 1.5px solid var(--line); background: var(--sunk, var(--surface)); color: var(--ink); font: 13px/1.6 var(--mono); outline: none; }
  .code:focus-visible { border-color: var(--ink); }
  .rules { margin: 0; padding: 12px 14px 12px 30px; border-radius: var(--r-row); background: var(--hair); display: flex; flex-direction: column; gap: 4px; font-size: 12px; line-height: 1.45; color: var(--muted); }
  .rules b { color: var(--ink); font-weight: 600; }

  footer { display: flex; align-items: center; gap: 10px; padding: 14px 20px; border-top: 1px solid var(--line); }
  .problem, .ask { flex: 1; min-width: 0; font-size: 12.5px; color: var(--bad); }
  .ask { color: var(--ink); font-weight: 600; }
  footer button { height: 42px; padding: 0 22px; border-radius: var(--r-bar); font-size: 14px; font-weight: 600; }
  .ghost { border: 1.5px solid var(--line); background: transparent; color: var(--muted); }
  .save { border: 0; background: var(--ink); color: var(--inv); }
  .danger { border: 0; background: var(--bad); color: var(--surface); }
  @keyframes fade { from { opacity: 0; } }
  @keyframes rise { from { opacity: 0; transform: translateY(12px) scale(.99); } }
  @media (prefers-reduced-motion: reduce) { .sheet, .scrim { animation: none; } }
</style>
