<script>
  import { status } from '../status.svelte.js';
  import Switch from './Switch.svelte';

  /** `active`: this section is the one showing, so the list is read again each time it opens. */
  let { active = false } = $props();

  const { invoke } = window.__TAURI__?.core ?? { invoke: async () => { throw new Error('Not running inside OASIS.'); } };

  /** @typedef {{ id: string, time: string, style: string, raw: string, text: string, audio_ms: number, has_audio: boolean }} Entry */
  let entries = $state(/** @type {Entry[]} */ ([]));
  let keep = $state(false);
  let loaded = $state(false);
  let problem = $state('');
  const ready = $derived(!!status.engine);
  // Below 1400 px the streak card is a third of the row: show the day count only, no dot grid.
  let narrow = $state(false);
  $effect(() => {
    const q = matchMedia("(max-width: 1399px)");
    const set = () => (narrow = q.matches);
    set();
    q.addEventListener("change", set);
    return () => q.removeEventListener("change", set);
  });

  async function load() {
    try {
      const reply = await invoke('settings_fields');
      const fields = reply.sections.find((/** @type {{ id: string }} */ s) => s.id === 'voice')?.fields ?? [];
      keep = !!fields.find((/** @type {{ id: string }} */ f) => f.id === 'voice.keep_history')?.value;
      entries = await invoke('history_list');
      problem = '';
      loaded = true;
    } catch (e) { problem = String(e); }
  }
  $effect(() => { if (ready && active) load(); });
  // A dictation made while this section is open shows up at once (it is saved just before the text is typed).
  $effect(() => {
    const off = window.__TAURI__?.event.listen('engine-event', (/** @type {{ payload: { type: string } }} */ e) => {
      if (e.payload.type === 'transcript' && active && keep) setTimeout(load, 300);
    });
    return () => { off?.then((/** @type {() => void} */ stop) => stop()); };
  });

  async function setKeep(/** @type {boolean} */ on) {
    try { await invoke('set_setting', { id: 'voice.keep_history', value: on }); keep = on; problem = ''; } catch (e) { problem = String(e); }
  }
  const openFolder = () => invoke('open_history_folder').catch((/** @type {unknown} */ e) => (problem = String(e)));

  // ---- the list, a heading per day
  const pad = (/** @type {number} */ n) => String(n).padStart(2, '0');
  const ymd = (/** @type {Date} */ d) => `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
  function dayLabel(/** @type {string} */ day) {
    const t = new Date();
    if (day === ymd(t)) return 'Today';
    t.setDate(t.getDate() - 1);
    if (day === ymd(t)) return 'Yesterday';
    return new Date(`${day}T00:00:00`).toLocaleDateString(undefined, { weekday: 'long', month: 'short', day: 'numeric' });
  }
  const days = $derived.by(() => {
    /** @type {{ day: string, items: Entry[] }[]} */
    const out = [];
    for (const e of entries) {
      const day = e.time.slice(0, 10);
      if (out.at(-1)?.day === day) out.at(-1)?.items.push(e); else out.push({ day, items: [e] });
    }
    return out;
  });

  // ---- play and copy
  /** @type {HTMLAudioElement | null} */
  let player = null;
  let playing = $state('');
  let copied = $state('');
  function stop() { player?.pause(); if (player) URL.revokeObjectURL(player.src); player = null; playing = ''; }
  $effect(() => stop); // leaving the screen stops the sound
  async function play(/** @type {Entry} */ e) {
    const same = playing === e.id;
    stop();
    if (same) return;
    try {
      const bytes = Uint8Array.from(atob(await invoke('history_audio', { id: e.id })), (c) => c.charCodeAt(0));
      player = new Audio(URL.createObjectURL(new Blob([bytes], { type: 'audio/wav' })));
      player.onended = stop;
      playing = e.id;
      await player.play();
    } catch (x) { stop(); problem = String(x); }
  }
  // ---- the ⋮ menu: undo/redo the clean-up, delete, extract the audio
  let menu = $state(''); // id of the row whose menu is open
  let raws = $state(/** @type {Record<string, boolean>} */ ({})); // rows showing what the speech model heard
  let sure = $state(''); // id awaiting the second click on Delete
  const shown = (/** @type {Entry} */ e) => (raws[e.id] ? e.raw : e.text);
  // Word diff raw → edited (LCS): 'del' = the AI removed it, 'add' = the AI added it. Dictation-sized text, so O(n·m) is fine.
  function diff(/** @type {string} */ a, /** @type {string} */ b) {
    const x = a.split(/(\s+)/).filter(Boolean), y = b.split(/(\s+)/).filter(Boolean);
    const L = Array.from({ length: x.length + 1 }, () => new Array(y.length + 1).fill(0));
    for (let i = x.length - 1; i >= 0; i--) for (let j = y.length - 1; j >= 0; j--)
      L[i][j] = x[i] === y[j] ? L[i + 1][j + 1] + 1 : Math.max(L[i + 1][j], L[i][j + 1]);
    const out = [];
    let i = 0, j = 0;
    while (i < x.length || j < y.length) {
      if (i < x.length && j < y.length && x[i] === y[j]) { out.push({ t: x[i++], k: '' }); j++; }
      else if (j < y.length && (i >= x.length || L[i][j + 1] >= L[i + 1][j])) out.push({ t: y[j++], k: 'add' });
      else out.push({ t: x[i++], k: 'del' });
    }
    return out;
  }
  // ---- stats: fixes = runs of changed words between what was heard and what was typed
  const stats = $derived.by(() => {
    let words = 0, fixes = 0;
    /** @type {Record<string, number>} */
    const perDay = {};
    for (const e of entries) {
      const n = e.text.split(/\s+/).filter(Boolean).length;
      words += n;
      perDay[e.time.slice(0, 10)] = (perDay[e.time.slice(0, 10)] ?? 0) + n;
      if (e.raw === e.text) continue;
      let inRun = false;
      for (const s of diff(e.raw, e.text)) {
        const changed = s.k && s.t.trim();
        if (changed && !inRun) fixes++;
        if (changed) inRun = true; else if (!s.k && s.t.trim()) inRun = false;
      }
    }
    const set = new Set(days.map((d) => d.day)); // days, newest first
    let longest = 0, run = 0;
    const sorted = [...set].sort();
    for (let i = 0; i < sorted.length; i++) {
      const gap = i && (new Date(sorted[i]).getTime() - new Date(sorted[i - 1]).getTime()) / 864e5;
      run = i && gap === 1 ? run + 1 : 1;
      longest = Math.max(longest, run);
    }
    // current streak: back from today (or yesterday, if nothing is said yet today)
    const t = new Date();
    if (!set.has(ymd(t))) t.setDate(t.getDate() - 1);
    let current = 0;
    const inStreak = new Set();
    for (; set.has(ymd(t)); t.setDate(t.getDate() - 1)) { current++; inStreak.add(ymd(t)); }
    // the last 12 weeks, one dot a day, Sunday on the top row; a dot's shade follows that day's words
    const peak = Math.max(1, ...Object.values(perDay));
    // 13 week columns ending with this week, Sunday on the top row; days still to come stay empty
    const today = ymd(new Date());
    const first = new Date(); first.setDate(first.getDate() - first.getDay() - 77);
    const cells = [];
    for (let i = 0; i < 91; i++) {
      const d = new Date(first); d.setDate(first.getDate() + i);
      const day = ymd(d), n = perDay[day] ?? 0;
      cells.push(day > today ? null : { day, n, lvl: n ? Math.ceil(4 * n / peak) : 0, streak: inStreak.has(day) }); // 4 shades, by share of the busiest day
    }
    return { words, fixes, current, longest, cells };
  });
  $effect(() => {
    if (!menu) return;
    const close = () => { menu = ''; sure = ''; };
    const onKey = (/** @type {KeyboardEvent} */ k) => { if (k.key === 'Escape') close(); };
    window.addEventListener('click', close);
    window.addEventListener('keydown', onKey);
    return () => { window.removeEventListener('click', close); window.removeEventListener('keydown', onKey); };
  });
  async function remove(/** @type {Entry} */ e) {
    if (sure !== e.id) { sure = e.id; return; } // the menu stays open for the confirming click
    try { await invoke('history_delete', { id: e.id }); entries = entries.filter((x) => x.id !== e.id); } catch (x) { problem = String(x); }
    menu = ''; sure = '';
  }
  async function extract(/** @type {Entry} */ e) {
    try {
      const bytes = Uint8Array.from(atob(await invoke('history_audio', { id: e.id })), (c) => c.charCodeAt(0));
      const a = document.createElement('a');
      a.href = URL.createObjectURL(new Blob([bytes], { type: 'audio/wav' }));
      a.download = `${e.id}.wav`;
      a.click();
      setTimeout(() => URL.revokeObjectURL(a.href), 4000);
    } catch (x) { problem = String(x); }
  }
  async function exportSet() {
    try { const n = await invoke('history_export_dataset'); problem = ''; note = `Training set with ${n} clip${n === 1 ? '' : 's'} ready: the folder is open.`; } catch (x) { problem = String(x); }
  }
  let note = $state('');

  async function copy(/** @type {Entry} */ e) {
    try {
      await navigator.clipboard.writeText(shown(e));
      copied = e.id;
      setTimeout(() => { if (copied === e.id) copied = ''; }, 1400);
    } catch { problem = 'The text could not be copied.'; }
  }
</script>

<div class="wrap">
  {#if !ready}
    <p class="hint">The history appears here once OASIS is running.</p>
  {:else if loaded}
    <section class="card head" aria-label="History settings">
      <div class="rtext">
        <div class="rname">Keep my dictation history</div>
        <div class="hint">Each dictation is saved on this PC as a recording and its text. Nothing is sent anywhere. To help improve the speech model, share the folder yourself, when you choose to.</div>
      </div>
      <button type="button" class="quiet" onclick={exportSet} title="Copies every clip with its text into a 'dataset' folder (metadata.jsonl), ready to send or train on">Export training set</button>
      <button type="button" class="quiet" onclick={openFolder}>Open folder</button>      <Switch checked={keep} label="Keep my dictation history" onchange={setKeep} />
    </section>

    {#if problem}<span class="problem" role="alert">{problem}</span>{:else if note}<span class="hint" role="status">{note}</span>{/if}

    <div class="body">
    <div class="stats" aria-label="Your numbers">
      <section class="card stat"><div class="big">{stats.fixes.toLocaleString()}</div><div class="lab">Fixes made</div></section>
      <section class="card stat"><div class="big">{stats.words.toLocaleString()}</div><div class="lab">Total words dictated</div></section>
      <section class="card stat">
        <div class="big">{stats.current} day{stats.current === 1 ? '' : 's'}</div>
        <div class="lab">Streak · longest {stats.longest}</div>
        {#if !narrow}
        <div class="heat" role="img" aria-label="Days you dictated in the last 12 weeks">
          {#each stats.cells as c}
            {#if c}<i class="l{c.lvl}" class:streak={c.streak} title="{dayLabel(c.day)}: {c.n} word{c.n === 1 ? '' : 's'}"></i>{:else}<i class="pad"></i>{/if}
          {/each}
        </div>
        <div class="key">
          <span>Less</span>{#each [1, 2, 3, 4] as l}<i class="l{l}"></i>{/each}<span>More</span>
          <i class="streak"></i><span>Current streak</span>
        </div>
        {/if}
      </section>
    </div>

    <div class="list" aria-label="Dictation history">
      {#each days as d (d.day)}
        <h2 class="day lab">{dayLabel(d.day)}</h2>
        <ul>
          {#each d.items as e (e.id)}
            <li class="row">
              <time class="when mono" datetime={e.time.replace(' ', 'T')} title={e.time}>{e.time.slice(11, 16)}</time>
              <p class="text">{#if raws[e.id]}{#each diff(e.raw, e.text) as s}<span class={s.k}>{s.t}</span>{/each}{:else}{e.text}{/if}</p>
              <span class="acts" class:show={playing === e.id || copied === e.id || menu === e.id}>
                {#if e.has_audio}
                  <button type="button" class="quiet" onclick={() => play(e)}>{playing === e.id ? 'Stop audio' : 'Play audio'}</button>
                {/if}
                <button type="button" class="quiet" onclick={() => copy(e)}>{copied === e.id ? 'Copied' : 'Copy text'}</button>
                <span class="mwrap">
                  <button type="button" class="quiet dots" aria-label="More" aria-expanded={menu === e.id}
                    onclick={(ev) => { ev.stopPropagation(); menu = menu === e.id ? '' : e.id; sure = ''; }}>⋮</button>
                  {#if menu === e.id}
                    <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
                    <div class="menu" onclick={(ev) => ev.stopPropagation()}>
                      {#if e.raw !== e.text}
                        <button type="button" onclick={() => { raws[e.id] = !raws[e.id]; menu = ''; }}>{raws[e.id] ? 'Redo AI edit' : 'Undo AI edit'}</button>
                      {/if}
                      {#if e.has_audio}
                        <button type="button" onclick={() => { extract(e); menu = ''; }}>Extract audio</button>
                      {/if}
                      <button type="button" class="bad" onclick={() => remove(e)}>{sure === e.id ? 'Click again to delete' : 'Delete transcript'}</button>
                    </div>
                  {/if}
                </span>
              </span>
            </li>
          {/each}
        </ul>
      {:else}
        <p class="hint empty">{keep ? 'Nothing yet. Hold your talk button and dictate: it shows up here.' : 'The history is off. Turn it on above, and each dictation from then on is kept here.'}</p>
      {/each}
    </div>
    </div>
  {:else}
    <p class="hint" role="status">{problem || 'Loading the history…'}</p>
  {/if}
</div>

<style>
  .wrap { flex: 1; min-height: 0; display: flex; flex-direction: column; gap: 14px; min-width: 0; max-width: 980px; }
  .card { border: 1px solid var(--line); background: var(--surface); border-radius: var(--r-card); padding: 14px 18px; }
  .head { display: flex; align-items: center; gap: 16px; flex-wrap: wrap; }
  .rtext { flex: 1 1 320px; min-width: 0; }
  .rname { font-size: 14px; font-weight: 600; color: var(--ink); }
  .hint { margin: 2px 0 0; font-size: 12px; line-height: 1.45; color: var(--muted); max-width: 72ch; }
  .body { flex: 1; min-height: 0; display: grid; grid-template-columns: minmax(0, 1fr); grid-template-rows: auto minmax(0, 1fr); gap: 14px; }
  .stats { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: 12px; align-items: start; }
  .heat { margin-top: 12px; display: grid; grid-template-columns: repeat(13, 1fr); grid-template-rows: repeat(7, auto); grid-auto-flow: column; gap: 4px; }
  .heat i { aspect-ratio: 1; border-radius: 50%; background: var(--hair); }
  .heat i.l1, .key i.l1 { background: color-mix(in srgb, var(--ok) 30%, var(--hair)); }
  .heat i.l2, .key i.l2 { background: color-mix(in srgb, var(--ok) 55%, var(--hair)); }
  .heat i.l3, .key i.l3 { background: color-mix(in srgb, var(--ok) 80%, var(--hair)); }
  .heat i.l4, .key i.l4 { background: var(--ok); }
  .heat i.streak, .key i.streak { box-shadow: 0 0 0 1.5px var(--ok); }
  .key { margin-top: 12px; display: flex; align-items: center; flex-wrap: wrap; gap: 5px; font: 500 11px/1 var(--mono); color: var(--muted); }
  .key i { width: 12px; height: 12px; border-radius: 50%; background: var(--hair); }
  .key i.streak { background: none; margin-left: 10px; }
  .heat i.pad { background: none; }
  /* wide window: the numbers sit in a column at the right of the list */
  @media (min-width: 1400px) {
    .wrap { max-width: 1320px; }
    .body { grid-template-columns: minmax(0, 1fr) 320px; grid-template-rows: minmax(0, 1fr); align-items: start; }
    .stats { grid-column: 2; grid-row: 1; grid-template-columns: minmax(0, 1fr); }
    .list { grid-column: 1; grid-row: 1; height: 100%; }
  }
  .stat .big { font-size: 28px; font-weight: 600; color: var(--ink); line-height: 1.1; }
  .stat .lab { margin-top: 4px; }
  .empty { padding: 18px 4px; }
  .problem { font-size: 12px; color: var(--bad); }
  .lab { font: 500 11px/1.3 var(--mono); letter-spacing: .1em; text-transform: uppercase; color: var(--muted); }
  .mono { font-family: var(--mono); }

  .list { flex: 1; min-height: 0; overflow-y: auto; padding-right: 4px; }
  .day { margin: 18px 0 6px; font-weight: 500; }
  .day:first-child { margin-top: 0; }
  ul { margin: 0; padding: 0; list-style: none; border-top: 1px solid var(--line); }
  /* time on the left, the text beside it, and the two actions at the right edge once the row is hovered or focused */
  .row { display: grid; grid-template-columns: 56px minmax(0, 1fr) auto; align-items: start; gap: 14px; padding: 12px 6px; border-bottom: 1px solid var(--hair); }
  .row:hover, .row:focus-within { background: var(--sunk); }
  .when { padding-top: 2px; font-size: 12px; color: var(--muted); }
  .text { margin: 0; font-size: 14px; line-height: 1.5; color: var(--ink); overflow-wrap: anywhere; white-space: pre-wrap; }
  .del { color: var(--bad); background: color-mix(in srgb, var(--bad) 14%, transparent); text-decoration: line-through; }
  .add { color: var(--ok); background: color-mix(in srgb, var(--ok) 14%, transparent); }
  .acts { display: flex; gap: 6px; opacity: 0; transition: opacity .15s var(--ease); }
  .row:hover .acts, .row:focus-within .acts, .acts.show { opacity: 1; }
  .quiet { height: 30px; padding: 0 12px; border-radius: var(--r-pill); border: 1px solid var(--line); background: var(--surface); color: var(--ink); font-size: 12px; font-weight: 600; white-space: nowrap; }
  .mwrap { position: relative; }
  .dots { width: 30px; padding: 0; font-size: 16px; line-height: 1; }
  .menu { position: absolute; right: 0; top: 34px; z-index: 5; min-width: 190px; display: flex; flex-direction: column; padding: 6px; border: 1px solid var(--line); border-radius: var(--r-card); background: var(--surface); box-shadow: 0 8px 24px rgba(0, 0, 0, .35); }
  .menu button { text-align: left; padding: 9px 12px; border: 0; border-radius: 8px; background: none; color: var(--ink); font-size: 13px; }
  .menu button:hover, .menu button:focus-visible { background: var(--sunk); }
  .menu .bad { color: var(--bad); }
  @media (hover: none) { .acts { opacity: 1; } }
  @media (max-width: 700px) { .row { grid-template-columns: 48px minmax(0, 1fr); } .acts { grid-column: 2; } }
</style>
