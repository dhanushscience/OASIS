<script>
  import { tick } from 'svelte';
  import {
    data, OUT_MS, current, currentConfig, isEverywhere, saveProfile, saveLook, deleteProfile,
    setMulti, addSet, updateSet, deleteSet, askIfExists, setIconImage, appIcon, editExisting,
  } from '../profiles.svelte.js';
  import { overlaps } from '../matching.js';
  import { sure } from '../confirm.svelte.js';
  import OsLogo from './OsLogo.svelte';
  import Select from './Select.svelte';
  import { status } from '../status.svelte.js';
  import { view as padView } from '../pad.svelte.js';
  // Two drawings to choose between only when the pad is on and linked and keyboard keys are on too.
  const both = $derived(data.input.hardware && data.input.keyboard && status.engine?.pad === 'Connected');
  const { invoke } = window.__TAURI__?.core ?? { invoke: async () => { throw new Error('Not running inside OASIS.'); } };

  /** @type {{ children?: import('svelte').Snippet }} */
  let { children } = $props();

  const p = $derived(current());
  const everywhere = $derived(isEverywhere(p));
  const locked = $derived(!!status.engine && !status.engine.integrity_ok); // DESIGN.md 6.7: read-only

  let problem = $state('');
  /** Runs a change and shows its refusal, if any, under the header. */
  async function attempt(/** @type {() => Promise<any>} */ run) {
    problem = '';
    try { await run(); return true; } catch (e) { problem = String(e); return false; }
  }

  // ---- the name, changed in the settings panel
  let name = $state('');

  // ---- Applies to
  let match = $state('');
  let kind = $state('app');
  const openMatch = () => { match = p.context_match; kind = p.kind === 'website' ? 'website' : 'app'; };

  // ---- the settings: everything is a draft until Save
  let emoji = $state('');
  /** asking "turn on several sets?" from the bar under the pad (the settings panel does not ask) */
  let confirmMulti = $state(false);
  $effect(() => { void data.shown; confirmMulti = false; });
  let multi = $state(false);
  /** show the labels first: a tap shows what each button does, the next one runs it (off: a tap runs at once) */
  let labels = $state(true);
  /** 'emoji', 'app' (the program's own icon) or 'image' (a picture from the computer) */
  let iconMode = $state('emoji');
  /** the picture chosen in this panel, as PNG bytes; null = nothing new */
  let iconBytes = $state(/** @type {number[] | null} */ (null));
  let iconPreview = $state('');
  let iconNote = $state('');
  let iconBusy = $state(false);
  /** another profile that already applies to what is typed */
  const twin = $derived(match.trim() && match.trim() !== '*' ? data.profiles.find((o) => o.index !== p.index && overlaps(match, o.context_match)) : undefined);

  function showBytes(/** @type {number[]} */ bytes) {
    iconBytes = bytes;
    iconPreview = URL.createObjectURL(new Blob([new Uint8Array(bytes)], { type: 'image/png' }));
  }
  async function useAppIcon() {
    iconNote = '';
    iconBusy = true;
    try { showBytes(await appIcon(match)); iconMode = 'app'; }
    catch (e) { iconNote = String(e); }
    iconBusy = false;
  }
  /** A picture from the computer, squared to 64 pixels so every icon is small and the same size. */
  async function pickImage(/** @type {Event & { currentTarget: HTMLInputElement }} */ e) {
    const file = e.currentTarget.files?.[0];
    e.currentTarget.value = '';
    if (!file) return;
    iconNote = '';
    try {
      const img = await createImageBitmap(file);
      const canvas = document.createElement('canvas');
      canvas.width = canvas.height = 64;
      const k = Math.min(64 / img.width, 64 / img.height);
      canvas.getContext('2d')?.drawImage(img, (64 - img.width * k) / 2, (64 - img.height * k) / 2, img.width * k, img.height * k);
      const blob = await new Promise((done) => canvas.toBlob(done, 'image/png'));
      if (!blob) throw new Error('empty');
      showBytes(Array.from(new Uint8Array(await /** @type {Blob} */ (blob).arrayBuffer())));
      iconMode = 'image';
    } catch { iconNote = 'That file could not be read as a picture.'; }
  }

  /** One Save for the whole panel. */
  async function save() {
    problem = '';
    if (!name.trim()) { problem = 'A profile needs a name.'; return; }
    const m = match.trim();
    if (m && m !== '*' && askIfExists(m, p.index)) return; // "already exists - modify it?"
    const was = { emoji: p.icon_emoji, image: !!data.icons[p.index], multi: p.multi, labels: p.label_mode !== 'hide' };
    const mode = labels === was.labels ? p.label_mode : labels ? 'show' : 'hide';
    let looked = false;
    const ok = await attempt(async () => {
      await saveProfile({ name, context_match: match, kind });
      if (iconMode === 'emoji') {
        if (was.image) await setIconImage(null);
        if (emoji.trim() !== was.emoji || mode !== p.label_mode) { await saveLook(mode, emoji.trim()); looked = true; }
      } else if (iconBytes) {
        await setIconImage(iconBytes);
        if (was.emoji || mode !== p.label_mode) { await saveLook(mode, ''); looked = true; }
      }
      if (!looked && mode !== p.label_mode) await saveLook(mode, p.icon_emoji);
      if (multi !== was.multi) await setMulti(multi);
    });
    if (ok) closeSettings();
  }

  // ---- sets of buttons: picked in the list; edited in the menu; added with "+"
  let mode = $state('edit'); // 'edit' the chosen set, or 'new'
  let setName = $state('');
  let setMatch = $state('');
  let setPanel = $state('');
  let setWhen = $state('');
  let setLabel = $state('');
  const labelModes = [{ value: '', label: 'Follow the profile' }, { value: 'show', label: 'Show labels' }, { value: 'hide', label: 'Hide labels (run at once)' }];
  /** why the pad view jumped to this set (a picked section already had one) */
  let setNote = $state('');
  let view = $state('settings'); // which layer is open over the pad: 'settings' or 'new'
  /** The screen only shows this header when the profile has a set of buttons. */
  const cfg = $derived(/** @type {NonNullable<ReturnType<typeof currentConfig>>} */ (currentConfig()));
  // The quick switch under the name. It changes the set's own choice when that set has one, else the profile's.
  // With several sets, the switch is per set (each can differ); with one, it is the profile's.
  const ownLabels = $derived(p.multi || cfg.label_mode !== '');
  const labelsOn = $derived((cfg.label_mode !== "" ? cfg.label_mode : p.label_mode) !== 'hide');
  const toggleLabels = () => attempt(() => {
    const next = labelsOn ? 'hide' : 'show';
    return ownLabels
      ? updateSet({ name: cfg.name, context_match: cfg.is_primary ? '' : cfg.context_match, when: cfg.when, panel_id: cfg.panel_id, label_mode: next })
      : saveLook(next, p.icon_emoji);
  });
  function fillSet() {
    picked = ''; pickNote = ''; setNote = '';
    if (mode === 'new') { setName = `CP${p.configs.length + 1}`; setMatch = ''; setPanel = ''; setWhen = ''; setLabel = ''; return; }
    setName = cfg.name; setMatch = cfg.is_primary ? '' : cfg.context_match; setPanel = cfg.panel_id; setWhen = cfg.when; setLabel = cfg.label_mode;
  }
  // "Pick the section": a few seconds to click in the other program, then OASIS reads what has focus.
  let countdown = $state(0);
  let picked = $state('');
  let pickNote = $state('');
  async function pickSection() {
    pickNote = ''; picked = '';
    countdown = 1; // waiting: OASIS is minimised and the overlay asks for the click
    try {
      const got = await invoke('capture_focus');
      countdown = 0;
      const known = data.panelPresets.some((o) => o.value === got.panel_id);
      setPanel = known ? got.panel_id : '';
      setMatch = known ? '' : got.context;
      picked = known ? got.panel_label || got.panel_id : got.context;
      if (picked && got.process) picked = `${picked} · ${got.process}`;
      if (!picked) { pickNote = 'Nothing was recognised there. Try again.'; return; }
      // Already have a set for this: go to it instead of making a second one.
      const same = (/** @type {string} */ a, /** @type {string} */ b) => a.trim().toLowerCase() === b.trim().toLowerCase();
      const old = p.configs.find((c) => !c.is_primary && same(c.context_match, setMatch) && same(c.panel_id, setPanel) && !c.when);
      if (old) { chosen(old.index); setNote = `“${old.name}” already uses ${picked}. Nothing new was made; edit it here.`; view = 'set'; }
    } catch (e) { pickNote = String(e); }
    countdown = 0;
  }
  // "When" is about YouTube (playing, ended, Shorts), so it only shows where YouTube can be: a browser profile,
  // a set for a YouTube page, or a set that already has one (so it can be cleared).
  const BROWSERS = ['chrome', 'msedge', 'edge', 'firefox', 'brave', 'opera', 'vivaldi', 'arc'];
  const showWhen = $derived(
    !!setWhen || /youtu/i.test(setMatch) || /youtu/i.test(p.context_match) ||
      p.context_match.toLowerCase().split(/[,\s]+/).some((w) => BROWSERS.includes(w.replace(/\.exe$/, ''))),
  );
  const setEdit = () => ({ name: setName, context_match: setMatch, when: setWhen, panel_id: setPanel, label_mode: setLabel });
  function openSettings() {
    name = p.name; openMatch(); emoji = p.icon_emoji; problem = ''; multi = p.multi; labels = p.label_mode !== 'hide';
    iconBytes = null; iconNote = ''; iconPreview = data.icons[p.index] ?? '';
    iconMode = iconPreview ? 'image' : 'emoji';
    mode = 'edit'; fillSet(); view = 'settings'; data.settings = true;
  }
  // "Modify it" in the already-exists question opens this profile's settings (after it has been shown).
  let seenRequest = 0;
  $effect(() => { const n = data.editRequest; if (n !== seenRequest) { seenRequest = n; tick().then(openSettings); } });
  // The main set is the profile itself (it has no condition of its own), so it opens the profile's settings.
  function openSet() {
    if (cfg.is_primary) return openSettings();
    problem = '';
    mode = 'edit'; fillSet(); view = 'set'; data.settings = true;
  }
  function closeSettings() {
    problem = ''; data.settings = false; }
  $effect(() => { void data.shown; data.settings = false; });
  const chosen = (/** @type {number} */ i) => { data.config = i; mode = 'edit'; fillSet(); };
</script>

<svelte:window onkeydown={(e) => data.settings && e.key === 'Escape' && closeSettings()} />

<header>
  <div class="id" style="opacity: {data.out ? 0 : 1}; transform: {data.out ? `translateY(${data.dir * 6}px)` : 'none'}; transition-duration: {data.out ? OUT_MS : 320}ms">
    <h2 class="name">{p.name}</h2>
      <button class="more" class:on={data.settings && view === 'settings'} aria-label="Settings for this profile" aria-expanded={data.settings && view === 'settings'} title="Settings for this profile"
      onclick={() => (data.settings && view === 'settings' ? closeSettings() : openSettings())}><svg viewBox="0 0 24 24" aria-hidden="true"><circle cx="12" cy="12" r="3" /><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1 0 2.83 2 2 0 0 1-2.83 0l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-2 2 2 2 0 0 1-2-2v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83 0 2 2 0 0 1 0-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1-2-2 2 2 0 0 1 2-2h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 0-2.83 2 2 0 0 1 2.83 0l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 2-2 2 2 0 0 1 2 2v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 0 2 2 0 0 1 0 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 2 2 2 2 0 0 1-2 2h-.09a1.65 1.65 0 0 0-1.51 1z" /></svg></button>
    {#if both}
      <span class="grow"></span>
      <div class="seg dev" role="group" aria-label="Show the buttons for">
        <button class:on={!padView.keys} aria-pressed={!padView.keys} onclick={() => (padView.keys = false)}>OASIS interface</button>
        <button class:on={padView.keys} aria-pressed={padView.keys} onclick={() => (padView.keys = true)}>Keyboard</button>
      </div>
    {/if}
  </div>

  <button type="button" class="quick" aria-pressed={!labelsOn} disabled={locked} onclick={toggleLabels}
    title={labelsOn ? 'Showing labels: a tap shows what each button does, the next one runs it. Click to run on the first tap instead.' : 'Labels are hidden: a tap runs the button at once. Click to show labels again.'}>
    <svg viewBox="0 0 24 24" aria-hidden="true">
      <path d="M2 12s3.6-7 10-7 10 7 10 7-3.6 7-10 7S2 12 2 12z" /><circle cx="12" cy="12" r="3" />{#if !labelsOn}<path d="M4 4l16 16" />{/if}
    </svg>
    {labelsOn ? 'Show labels' : 'Labels hidden'}
  </button>

  {@render children?.()}

  <div class="sets">
    <!-- greyed out and untouchable until several sets are enabled -->
    <div class="setlist" class:off={!p.multi} inert={!p.multi} role="radiogroup" aria-label="Set of buttons">
      <span class="lab">Config profile</span>
      {#each p.multi ? p.configs : [cfg] as c}
        <button class="set" role="radio" aria-checked={cfg.index === c.index} class:on={cfg.index === c.index} onclick={() => chosen(c.index)}
          ondblclick={async () => { chosen(c.index); await tick(); openSet(); }}>{c.name}</button>
      {/each}
      <button class="set add" aria-label="Add a set of buttons" disabled={locked || p.configs.length >= data.maxSets}
        title={p.configs.length >= data.maxSets ? `Up to ${data.maxSets} sets` : 'Add a set of buttons for another window or address'}
        onclick={() => { mode = 'new'; fillSet(); view = 'new'; data.settings = true; }}>+</button>
    </div>
    {#if p.multi}
      <button class="set edit" aria-label="Edit this set" title={cfg.is_primary ? 'Settings of this profile' : 'Name and conditions of this set'} onclick={openSet}>Edit set</button>
    {:else}
      {#if confirmMulti}
        <span class="ask">Enable several sets of buttons?</span>
        <button class="yes" onclick={async () => { await attempt(() => setMulti(true)); confirmMulti = false; }}>Yes, enable</button>
        <button class="set edit" onclick={() => (confirmMulti = false)}>Cancel</button>
      {:else}
        <button class="set edit" disabled={locked} title="Give this profile other sets of buttons, for a window title or address" onclick={() => (confirmMulti = true)}>Enable multi config</button>
      {/if}
    {/if}
  </div>

  <section class="panel compact top" class:open={data.settings && view === 'settings'} inert={!(data.settings && view === 'settings')} aria-label="Profile settings">
    <div class="phead">
      <span class="ptitle"><span class="lab">Application profile</span>
        <label class="etitle" title="Rename"><input id="pf-name" bind:value={name} readonly={locked} autocomplete="off" maxlength="40" aria-label="Profile name" /><svg viewBox="0 0 24 24" aria-hidden="true"><path d="M4 20h4L19 9a2.8 2.8 0 0 0-4-4L4 16v4z" /><path d="m13.5 6.5 4 4" /></svg></label></span>
      <button class="x" aria-label="Close without saving" title="Close without saving" onclick={closeSettings}><svg viewBox="0 0 24 24" aria-hidden="true"><path d="M6 6l12 12M18 6 6 18" /></svg></button>
    </div>
    <!-- the same rows as a set of buttons: a quiet icon and its name on the left, the control on the right -->
    <div class="rows">
      <div class="row top"><span class="rl"><svg viewBox="0 0 24 24" aria-hidden="true"><rect x="3" y="4" width="18" height="16" rx="2" /><path d="M3 9h18" /></svg>Applies to</span>
        <div class="stack">
          <div class="seg" role="radiogroup" aria-label="It is">
            {#each [['app', 'An app'], ['website', 'A website']] as [v, t]}
              <button type="button" role="radio" aria-checked={kind === v} class:on={kind === v} disabled={locked || !match.trim() || match.trim() === '*'} onclick={() => (kind = v)}>{t}</button>
            {/each}
          </div>
          <input id="pf-match" bind:value={match} readonly={locked} autocomplete="off" maxlength="200" placeholder="chrome, code, netflix.com, *" aria-label="Applies to" />
          {#if twin}
            <span class="hint warn">“{twin.name}” already applies to this. <button type="button" class="link" onclick={() => editExisting(twin.index)}>Modify it instead</button></span>
          {:else}
            <span class="hint">An app name, a program path or a web address. * means everything else.</span>
          {/if}
        </div>
      </div>

      <div class="row top"><span class="rl"><svg viewBox="0 0 24 24" aria-hidden="true"><rect x="3" y="3" width="18" height="18" rx="2" /><circle cx="9" cy="9" r="2" /><path d="m21 15-5-5L5 21" /></svg>Icon</span>
        <div class="stack">
          <div class="iconrow">
            <span class="pv" aria-hidden="true">
              {#if iconMode !== 'emoji' && iconPreview}<img src={iconPreview} alt="" />{:else if iconMode === 'emoji' && emoji.trim()}{emoji}{:else if everywhere}<OsLogo size={22} />{:else}{name.charAt(0).toUpperCase()}{/if}
            </span>
            <div class="seg" role="radiogroup" aria-label="Icon from">
              {#each [['emoji', 'Emoji'], ['app', 'App icon'], ['image', 'Image']] as [v, t]}
                <button type="button" role="radio" aria-checked={iconMode === v} class:on={iconMode === v} disabled={locked}
                  onclick={() => { iconMode = v; if (v === 'app' && !iconBytes) useAppIcon(); }}>{t}</button>
              {/each}
            </div>
          </div>
          {#if iconMode === 'emoji'}
            <input bind:value={emoji} maxlength="16" placeholder="Paste an emoji (Win + . opens the picker)" aria-label="Emoji icon" readonly={locked} />
          {:else if iconMode === 'app'}
            <button type="button" class="quiet" disabled={locked || iconBusy || !match.trim() || match.trim() === '*'} onclick={useAppIcon}>{iconBusy ? 'Reading…' : 'Read the app’s icon again'}</button>
          {:else}
            <label class="file quiet">Choose a picture…<input type="file" accept="image/*" hidden onchange={pickImage} disabled={locked} /></label>
          {/if}
          {#if iconNote}<span class="hint warn">{iconNote}</span>{/if}
        </div>
      </div>

      <div class="row"><span class="rl"><svg viewBox="0 0 24 24" aria-hidden="true"><path d="M2 12s3.5-7 10-7 10 7 10 7-3.5 7-10 7S2 12 2 12z" /><circle cx="12" cy="12" r="3" /></svg>Show labels</span>
        <span class="swline">
          <button type="button" class="switch" role="switch" aria-checked={labels} class:on={labels} disabled={locked} onclick={() => (labels = !labels)} aria-label="Show labels">
            <span class="knob"></span><span class="swt">{labels ? 'On' : 'Off'}</span>
          </button>
          <span class="hint">{labels ? 'A tap shows what each button does; the next tap runs it.' : 'A tap runs the button at once.'}</span>
        </span>
      </div>

      <div class="row"><span class="rl"><svg viewBox="0 0 24 24" aria-hidden="true"><path d="m12 3 9 5-9 5-9-5 9-5z" /><path d="m3 13 9 5 9-5" /></svg>Several sets</span>
        <span class="swline">
          <button type="button" class="switch" role="switch" aria-checked={multi} class:on={multi} disabled={locked} onclick={() => (multi = !multi)} aria-label="Several sets of buttons">
            <span class="knob"></span><span class="swt">{multi ? 'On' : 'Off'}</span>
          </button>
          <span class="hint">Other buttons for a window title or address, picked in the Config profile bar.</span>
        </span>
      </div>
    </div>

    {#if problem}<div class="problem" role="alert">{problem}</div>{/if}
    <div class="foot">
      <button class="quiet bad" disabled={locked || data.profiles.length <= 1} title={data.profiles.length <= 1 ? 'At least one profile must remain' : ''}
        onclick={async () => { if (await sure({ title: `Delete “${p.name}”?`, body: 'This profile and all its buttons will be removed. This cannot be undone.', yes: 'Delete profile' }) && await attempt(deleteProfile)) closeSettings(); }}>Delete profile</button>
      <span class="grow"></span>
      <button class="primary" disabled={locked} onclick={save}>Save</button>
    </div>
  </section>

  <section class="panel compact" class:open={data.settings && view === 'set'} inert={!(data.settings && view === 'set')} aria-label="This set of buttons">
    <div class="phead">
      <span class="ptitle"><span class="lab">Set of buttons · {p.name}</span>
        <label class="etitle" title="Rename"><input bind:value={setName} readonly={locked} autocomplete="off" maxlength="24" aria-label="Set name" /><svg viewBox="0 0 24 24" aria-hidden="true"><path d="M4 20h4L19 9a2.8 2.8 0 0 0-4-4L4 16v4z" /><path d="m13.5 6.5 4 4" /></svg></label></span>
      <button class="x" aria-label="Close" title="Close" onclick={closeSettings}><svg viewBox="0 0 24 24" aria-hidden="true"><path d="M6 6l12 12M18 6 6 18" /></svg></button>
    </div>
    <!-- one row per setting: a quiet icon and its name on the left, the control on the right -->
    {#if setNote}<span class="hint note" role="status">{setNote}</span>{/if}
    <div class="rows">
      {#if !cfg.is_primary}
        <label class="row"><span class="rl"><svg viewBox="0 0 24 24" aria-hidden="true"><rect x="3" y="4" width="18" height="16" rx="2" /><path d="M3 9h18" /></svg>Title or address</span>
          <input bind:value={setMatch} readonly={locked} autocomplete="off" maxlength="200" placeholder="contains, e.g. WhatsApp" /></label>
        <!-- the panel a set is tied to (set by Pick the section) is kept as it is, but not shown: it is for OASIS, not for reading -->
        {#if showWhen}
        <div class="row"><span class="rl"><svg viewBox="0 0 24 24" aria-hidden="true"><circle cx="12" cy="12" r="9" /><path d="M12 7v5l3 2" /></svg>When</span>
          <Select bind:value={setWhen} disabled={locked} label="When" options={data.whenOptions} /></div>{/if}
      {:else}
        <span class="hint main">The main set applies when nothing more specific does, so it has no condition and cannot be deleted.</span>
      {/if}
      <div class="row"><span class="rl"><svg viewBox="0 0 24 24" aria-hidden="true"><path d="M2 12s3.5-7 10-7 10 7 10 7-3.5 7-10 7S2 12 2 12z" /><circle cx="12" cy="12" r="3" /></svg>Show labels</span>
        <Select bind:value={setLabel} disabled={locked} label="Show labels" options={labelModes} /></div>
    </div>
    {#if problem}<div class="problem" role="alert">{problem}</div>{/if}
    <div class="actions">
      {#if cfg.can_delete}
        <button class="quiet bad" onclick={async () => { if (await sure({ title: `Delete “${cfg.name}”?`, body: 'This set of buttons will be removed. This cannot be undone.', yes: 'Delete set' }) && await attempt(deleteSet)) closeSettings(); }}>Delete set</button>
      {/if}
      <span class="grow"></span>
      <button class="primary" disabled={locked} onclick={async () => { if (await attempt(() => updateSet(setEdit()))) closeSettings(); }}>Save set</button>
    </div>
  </section>

  <section class="panel compact" class:open={data.settings && view === 'new'} inert={!(data.settings && view === 'new')} aria-label="New set of buttons">
    <div class="phead">
      <span class="ptitle"><span class="lab">New set of buttons · {p.name}</span>
        {#if p.multi}<label class="etitle" title="Rename"><input bind:value={setName} readonly={locked} autocomplete="off" maxlength="24" aria-label="Set name" /><svg viewBox="0 0 24 24" aria-hidden="true"><path d="M4 20h4L19 9a2.8 2.8 0 0 0-4-4L4 16v4z" /><path d="m13.5 6.5 4 4" /></svg></label>{:else}<strong>Several sets</strong>{/if}</span>
      <button class="x" aria-label="Close" title="Close" onclick={closeSettings}><svg viewBox="0 0 24 24" aria-hidden="true"><path d="M6 6l12 12M18 6 6 18" /></svg></button>
    </div>
    {#if !p.multi}
      <p class="hint">One set of buttons now. Turn this on to give the same profile other buttons for a window title or address.
        {#if p.configs.length > 1}{p.configs.length - 1} more set{p.configs.length === 2 ? ' is' : 's are'} kept.{/if}</p>
    {:else}
      <div class="rows">
        <div class="row top"><span class="rl"><svg viewBox="0 0 24 24" aria-hidden="true"><path d="M12 3v4M12 17v4M3 12h4M17 12h4" /><circle cx="12" cy="12" r="3" /></svg>Applies to</span>
          <div class="stack">
            <button type="button" class="pickbtn" class:got={picked && countdown === 0} disabled={locked || countdown > 0} onclick={pickSection}>
              {#if picked && countdown === 0}<svg viewBox="0 0 24 24" aria-hidden="true"><path d="m5 12.5 4.5 4.5L19 7.5" /></svg><span class="pt">{picked}</span><span class="again">Pick again</span>
              {:else}<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M12 3v4M12 17v4M3 12h4M17 12h4" /><circle cx="12" cy="12" r="3" /></svg><span class="pt">{countdown > 0 ? 'Waiting for your click…' : 'Pick the section'}</span>{/if}
            </button>
            <span class="hint" class:warn={pickNote}>{pickNote || (countdown > 0 ? 'Click inside the program, in the part you want (a terminal, the editor, a page).' : (picked ? 'This set runs when that section is in front.' : 'OASIS steps aside, then click the section in the other program.'))}</span>
          </div>
        </div>
        <div class="row"><span class="rl"><svg viewBox="0 0 24 24" aria-hidden="true"><path d="M2 12s3.5-7 10-7 10 7 10 7-3.5 7-10 7S2 12 2 12z" /><circle cx="12" cy="12" r="3" /></svg>Show labels</span>
          <Select bind:value={setLabel} disabled={locked} label="Show labels" options={labelModes} /></div>
      </div>
    {/if}
    {#if problem}<div class="problem" role="alert">{problem}</div>{/if}
    <div class="actions">
      {#if p.multi && !picked && countdown === 0}<span class="hint">Pick the section to continue.</span>{/if}
      <span class="grow"></span>
      {#if p.multi}
        <button class="primary" disabled={locked || !picked || countdown > 0}
          onclick={async () => { if (await attempt(() => addSet(setEdit()))) { mode = 'edit'; fillSet(); closeSettings(); } }}>Create set</button>
      {:else}
        <button class="primary" disabled={locked} onclick={() => attempt(() => setMulti(true))}>Turn on</button>
      {/if}
    </div>
  </section>

  {#if problem && !data.settings}<div class="problem" role="alert">{problem}</div>{/if}
</header>

<style>
  /* above the pad, so its popovers open over it; `transform: none` at rest keeps this from trapping them */
  header { position: relative; flex: 1; min-height: 0; padding-top: 28px; display: flex; flex-direction: column; gap: 14px; }
  .id { transition-property: opacity, transform; transition-timing-function: var(--ease); flex: none; display: flex; flex-wrap: wrap; align-items: center; gap: 8px 12px; } /* narrow: the device switch goes under the name rather than cutting it short */
  .grow { flex: 1; }
  .name { margin: 0; font-size: 26px; font-weight: 600; letter-spacing: -.02em; line-height: 1.1; color: var(--signal-ink); min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .more { flex: none; width: 30px; height: 30px; border-radius: 50%; border: 1px solid var(--line); background: var(--surface); color: var(--ink); display: flex; align-items: center; justify-content: center; padding: 0; }
  .more svg { fill: none; stroke: currentColor; stroke-width: 1.7; stroke-linecap: round; stroke-linejoin: round; }
  .more svg { width: 17px; height: 17px; }
  .lab { font: 500 11px/1.3 var(--mono); letter-spacing: .12em; text-transform: uppercase; color: var(--muted); }

  /* always one line: the sets scroll sideways in a narrow window instead of wrapping under the bar */
  .sets { container-type: inline-size; container-name: sets; position: relative; z-index: 2; flex: none; height: 44px; border-top: 1px solid var(--hair); padding-top: 6px; display: flex; align-items: center; gap: 8px; }
  .setlist { flex: 1; min-width: 0; display: flex; flex-wrap: nowrap; align-items: center; gap: 8px; overflow-x: auto; scrollbar-width: none; padding: 2px 0; transition: opacity .24s var(--ease); }
  .setlist.off { opacity: .4; filter: grayscale(1); }
  .sets .lab { margin-right: 6px; font: 600 13px/1.3 var(--sans); letter-spacing: .08em; }
  .set { height: 32px; max-width: 170px; padding: 0 14px; border-radius: var(--r-pill); border: 1px solid var(--line); background: var(--surface); color: var(--ink); font-size: 13px; font-weight: 600; text-align: left; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; transition: background-color .24s var(--ease), color .24s var(--ease); }
  .set.on { border: 1.5px solid var(--ink); background: var(--sunk); }
  .sets .set, .sets .lab { flex: none; }
  @container sets (max-width: 460px) { .sets .lab { display: none; } }
  .set.add { width: 32px; padding: 0; text-align: center; border-style: dashed; border-color: var(--faint); background: transparent; font-size: 18px; font-weight: 400; }
  .quick { align-self: flex-start; margin-top: -6px; height: 26px; display: inline-flex; align-items: center; gap: 6px; padding: 0 11px 0 9px; border-radius: var(--r-pill); border: 1px solid var(--hair); background: transparent; color: var(--muted); font-size: 12px; font-weight: 500; transition: color .2s var(--ease), border-color .2s var(--ease), background-color .2s var(--ease); }
  .quick:hover { color: var(--ink); border-color: var(--line); background: var(--surface); }
  .quick svg { width: 14px; height: 14px; fill: none; stroke: currentColor; stroke-width: 1.7; stroke-linecap: round; stroke-linejoin: round; }
  .more.on { background: var(--ink); color: var(--inv); border-color: var(--ink); }
  /* a layer over the pad's area (the pad stays mounted underneath); header is static so this is placed against the screen body */
  .panel { position: absolute; left: 0; right: 0; top: 74px; bottom: 56px; z-index: 8; background: var(--surface); border: 1px solid var(--line); border-radius: var(--r-card); box-shadow: 0 8px 28px var(--shadow); padding: 18px; display: flex; flex-direction: column; gap: 14px; overflow: hidden;
    opacity: 0; transform: translateY(8px); visibility: hidden; transition: opacity .24s var(--ease), transform .24s var(--ease), visibility 0s linear .24s; }
  .panel.open { opacity: 1; transform: none; visibility: visible; transition-delay: 0s; }
  /* the set's own settings: a card just big enough, sitting over the Config profile row it belongs to */
  .panel.compact { top: auto; right: auto; width: min(460px, 100%); max-height: calc(100% - 130px); gap: 12px; padding: 16px 18px; overflow: hidden; }
  /* the rows scroll if they must; the title and the Save row always show */
  .panel.compact .rows { flex: 1 1 auto; min-height: 0; overflow-y: auto; padding-right: 4px; }
  .panel.compact .phead, .panel.compact .actions, .panel.compact .foot, .panel.compact .problem { flex: none; }
  @media (max-width: 1100px) { .panel.compact.top { max-height: calc(100% - 78px); } .rows .row { grid-template-columns: minmax(0, 1fr); gap: 4px; } .rows .row.top .rl { padding-top: 0; } .swline { flex-wrap: wrap; } .panel.compact .rows { overflow-x: hidden; } }
  .rows { display: flex; flex-direction: column; gap: 8px; }
  .etitle { display: inline-flex; flex-direction: row; align-items: center; gap: 6px; max-width: 100%; cursor: text; color: var(--muted); }
  .etitle input { field-sizing: content; min-width: 4ch; max-width: 100%; height: 30px; padding: 0 2px; border: 0; border-bottom: 1.5px dashed var(--line); border-radius: 0; background: transparent;
    color: var(--ink); font-size: 20px; font-weight: 700; letter-spacing: -.02em; }
  .etitle:hover input { border-bottom-color: var(--muted); }
  .etitle input:focus-visible { border-bottom: 1.5px solid var(--ink); }
  .etitle svg { width: 14px; height: 14px; flex: none; fill: none; stroke: currentColor; stroke-width: 1.8; stroke-linecap: round; stroke-linejoin: round; }
  .etitle:hover svg, .etitle:focus-within svg { color: var(--ink); }
  .row { display: grid; grid-template-columns: 136px minmax(0, 1fr); align-items: center; gap: 12px; }
  .rl { display: flex; align-items: center; gap: 8px; font-size: 12px; font-weight: 600; color: var(--muted); }
  .rl svg { width: 15px; height: 15px; flex: none; fill: none; stroke: currentColor; stroke-width: 1.7; stroke-linecap: round; stroke-linejoin: round; opacity: .8; }
  .row input { width: 100%; box-sizing: border-box; }
  .hint.main { line-height: 1.4; }
  /* the profile's own settings: the same card, opened from the gear, so it sits under the name */
  .panel.compact.top { top: 74px; bottom: auto; width: min(620px, 100%); max-height: calc(100% - 74px - 64px); }
  /* a short window: while open, it may cover the Config profile bar too, so more than a row or two shows */
  @media (max-height: 760px) { .panel.compact.top { bottom: 0; max-height: none; } }
  .row.top { align-items: start; }
  .row.top .rl { padding-top: 9px; }
  .stack { display: flex; flex-direction: column; gap: 6px; min-width: 0; }
  .stack .iconrow .pv { width: 36px; height: 36px; border-radius: 10px; font-size: 18px; }
  .stack .iconrow .pv img { width: 24px; height: 24px; }
  .swline { display: flex; align-items: center; gap: 12px; min-width: 0; }
  .swline .switch { flex: none; } /* the hint beside it must not squeeze it, or the knob runs past the edge */
  .swline .hint { line-height: 1.35; }
  .phead { display: flex; align-items: center; justify-content: space-between; gap: 12px; }
  .ptitle { display: flex; flex-direction: column; gap: 2px; }
  .ptitle strong { font-size: 20px; letter-spacing: -.02em; }
  .x { width: 30px; height: 30px; border-radius: 50%; border: 1px solid var(--line); background: var(--surface); color: var(--ink); display: flex; align-items: center; justify-content: center; padding: 0; }
  .x svg { width: 14px; height: 14px; fill: none; stroke: currentColor; stroke-width: 1.8; stroke-linecap: round; }
  /* Pick the section: a wide button that becomes a green "caught" chip once something is picked */
  .pickbtn { display: flex; align-items: center; gap: 10px; min-height: 40px; padding: 6px 14px; border-radius: var(--r-field); border: 1.5px dashed var(--faint); background: transparent; color: var(--ink); font-size: 13px; font-weight: 600; text-align: left; transition: border-color .2s var(--ease), background-color .2s var(--ease); }
  .pickbtn:hover:not(:disabled) { border-color: var(--ink); }
  .pickbtn.got { border-style: solid; border-color: var(--ok); background: color-mix(in srgb, var(--ok) 8%, transparent); }
  .pickbtn svg { width: 17px; height: 17px; flex: none; fill: none; stroke: currentColor; stroke-width: 1.8; stroke-linecap: round; stroke-linejoin: round; }
  .pickbtn.got svg { color: var(--ok); }
  .pickbtn .pt { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .pickbtn .again { flex: none; font-size: 11px; font-weight: 600; color: var(--muted); }

  label { display: flex; flex-direction: column; gap: 4px; }
  input { height: 36px; border-radius: var(--r-field); border: 1.5px solid var(--line); background: var(--surface); color: var(--ink); padding: 0 12px; font-size: 13px; outline: none; }
  input:focus-visible { border-color: var(--ink); }
  .primary { height: 34px; padding: 0 16px; border-radius: var(--r-pill); border: 0; background: var(--ink); color: var(--inv); font-weight: 600; font-size: 13px; }
  .quiet { height: 30px; padding: 0 12px; border-radius: var(--r-pill); border: 1px solid var(--line); background: transparent; font-size: 12px; font-weight: 600; }
  .bad { color: var(--bad); }
  .warn { color: var(--bad); }
  .link { border: 0; background: transparent; padding: 0; font: inherit; font-weight: 600; color: var(--ink); text-decoration: underline; }
  .lab { display: block; }
  .iconrow { display: flex; align-items: center; gap: 10px; }
  .pv { width: 44px; height: 44px; flex: none; border-radius: 12px; border: 1.5px solid var(--line); background: var(--surface); display: flex; align-items: center; justify-content: center; font-size: 22px; font-weight: 600; color: var(--ink); }
  .pv img { width: 30px; height: 30px; object-fit: contain; }
  .iconrow .seg { flex: 1; }
  .file { display: inline-flex; flex-direction: row; align-items: center; justify-content: center; cursor: pointer; align-self: flex-start; }
  /* the knob slides to the right when on, and the word moves to the free side */
  .switch { position: relative; align-self: flex-start; width: 78px; height: 32px; padding: 0 11px; display: flex; align-items: center; justify-content: flex-end; border-radius: var(--r-pill); border: 1.5px solid var(--line); background: var(--hair); font-size: 12px; font-weight: 600; color: var(--muted); transition: background-color .2s var(--ease), border-color .2s var(--ease), color .2s var(--ease); }
  .knob { position: absolute; top: 3px; left: 3px; width: 24px; height: 24px; border-radius: 50%; background: var(--surface); box-shadow: var(--shadow-chosen); transition: transform .22s var(--ease), background-color .2s var(--ease); }
  .switch.on { justify-content: flex-start; background: var(--ink); border-color: var(--ink); color: var(--inv); }
  .switch.on .knob { transform: translateX(46px); background: var(--signal); }
  .foot { flex: none; display: flex; align-items: center; gap: 8px; padding-top: 12px; border-top: 1px solid var(--hair); }
  .set.edit { margin-left: auto; font-size: 12px; }
  .ask { margin-left: auto; font-size: 12px; color: var(--muted); }
  .yes { height: 32px; padding: 0 14px; border-radius: var(--r-pill); border: 0; background: var(--ink); color: var(--inv); font-size: 12px; font-weight: 600; }
  .ask + .yes + .set.edit { margin-left: 0; }
  button:disabled { opacity: .4; cursor: default; }
  .seg { display: flex; gap: 2px; padding: 3px; border-radius: var(--r-pill); background: var(--hair); }
  .seg button { flex: 1; height: 28px; padding: 0 10px; border: 0; border-radius: var(--r-pill); background: transparent; color: var(--muted); font-size: 12px; font-weight: 600; white-space: nowrap; }
  .seg button.on { background: var(--surface); color: var(--ink); box-shadow: var(--shadow-chosen); }
  .seg.dev { flex: none; }
  .seg.dev button { flex: none; height: 30px; padding: 0 16px; }
  .actions { display: flex; align-items: center; gap: 8px; }
  .hint { margin: 0; font-size: 12px; color: var(--muted); }
  .hint.note { padding: 8px 10px; border-radius: var(--r-field); background: var(--sunk); color: var(--ink); }
  .problem { font-size: 12px; color: var(--bad); }
</style>
