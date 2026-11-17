<script>
  import { trackUnsaved } from '../busy.svelte.js';
  const { invoke } = window.__TAURI__?.core ?? { invoke: async () => { throw new Error('Not running inside OASIS.'); } };
  import { sure } from '../confirm.svelte.js';
  import KeyMapDialog from './KeyMapDialog.svelte';
  import FlowBuilder from './FlowBuilder.svelte';
  import { data, current, currentConfig, saveButton, SEGMENTS, segmentOf, looksLikeSite, kindWord } from '../profiles.svelte.js';
  import { status } from '../status.svelte.js';
  import ActionField from './ActionField.svelte';
  import { paint, HUES } from '../hues.js';
  import { splitBrowser } from '../actionKinds.js';
  import Select from './Select.svelte';

  // "Open with": the browsers found on this PC, the same list the Preferred browser setting offers.
  let browsers = $state(/** @type {{ value: string, label: string }[]} */ ([]));
  invoke('settings_fields').then((/** @type {any} */ r) => {
    browsers = r.sections.flatMap((/** @type {any} */ s) => s.fields).find((/** @type {any} */ f) => f.id === 'preferences.browser')?.options.filter((/** @type {any} */ o) => o.value) ?? [];
  }).catch(() => {});

  const p = $derived(current());
  const base = $derived(currentConfig()?.buttons[data.button]);
  const locked = $derived(!!status.engine && !status.engine.integrity_ok);

  /** @typedef {{ kind: string, value: string, next?: number | null, on_fail?: number | null, x?: number | null, y?: number | null }} Step */
  /** @typedef {{ seg: number, kind: string, label: string, value: string, browser: string, steps: Step[] }} Draft */
  const toDraft = (/** @type {any} */ b) => {
    // a site with its own browser is saved as "browser|address" on an app action; here it is a site again
    const site = b?.kind === 'LaunchApp' ? splitBrowser(b.value ?? '') : null;
    return /** @type {Draft} */ ({
      seg: segmentOf(b?.kind ?? ''), kind: site ? 'OpenUrl' : b?.kind ?? '', label: b?.label ?? '', value: site ? site.url : b?.value ?? '', browser: site?.browser ?? '',
      // the links and the builder's places come along, so saving keeps them
      steps: (b?.steps ?? []).map((/** @type {any} */ s) => ({ kind: s.kind, value: s.value, next: s.next ?? null, on_fail: s.on_fail ?? null, x: s.x ?? null, y: s.y ?? null })),
    });
  };

  /** @type {Draft | null} */
  let draft = $state(null);
  const d = $derived(draft ?? toDraft(base));
  // keyboard · an app window / the web · a list of steps; plain outlines, drawn in the text colour
  const ICONS = [
    '<rect x="2.5" y="6" width="19" height="12" rx="2.5"/><path d="M6 10h.01M10 10h.01M14 10h.01M18 10h.01M7.5 14h9"/>',
    '<rect x="3" y="4" width="18" height="16" rx="2.5"/><path d="M3 9h18M6.5 6.5h.01M9.5 6.5h.01"/><path d="m10 13-2 2 2 2M14 13l2 2-2 2"/>',
    '',
    '<path d="M9 6h11M9 12h11M9 18h11"/><path d="m3.5 5.5 1.2 1.2 2-2.4M3.5 11.5l1.2 1.2 2-2.4M3.5 17.5l1.2 1.2 2-2.4"/>',
  ];
  const dirty = $derived(draft != null && JSON.stringify(draft) !== JSON.stringify(toDraft(base)));
  trackUnsaved('configure', () => dirty);
  const edit = (/** @type {Partial<Draft>} */ patch) => { draft = { ...d, ...patch }; saved = false; problem = ''; };

  // A different button, profile or set starts from what is saved.
  let key = '';
  $effect(() => {
    const k = `${data.shown}/${currentConfig()?.index}/${data.button}`;
    if (k !== key) { key = k; tried = false; draft = null; saved = false; problem = ''; building = false; cancelTry(); tryNote = ''; }
  });

  let editingKey = $state(false);
  /** the automation builder is open */
  let building = $state(false);
  /** What the builder made goes on the button and is saved, so one press keeps it. */
  function saveFlow(/** @type {Step[]} */ steps) {
    edit({ seg: 3, steps });
    building = false;
    save();
  }
  const STEP_NAME = /** @type {Record<string, string>} */ ({
    QuickLaunch: 'Quick Launch', LaunchApp: 'Open app', OpenUrl: 'Open website', Shortcut: 'Press keys', AutoHotkey: 'Script', PlaySound: 'Sound', Delay: 'Wait',
  });

  let problem = $state('');
  let saved = $state(false);
  /** counts saves, so the box can pulse once for each */
  let pulse = $state(0);
  let busy = $state(false);
  /** asking "clear this button?" in place, before anything is removed */
  const hasAction = $derived(!!(base?.label || base?.value || base?.steps?.length));

  // The one field, and what it is called, per kind (DESIGN.md: "the smart action field").
  const FIELD = [
    ['Keys to press', 'Ctrl+Shift+P', 'Click the box, then press the keys you want, like Ctrl+Shift+P. Esc leaves, Backspace clears it.'],
    ['App or website', 'Type an app or a site', 'An app name, a program path or a web address.'],
    ['Text to paste', 'Type the text', 'Pasted where the cursor is.'],
    ['Automation', '', ''],
  ];
  const OTHER = /** @type {Record<string, string[]>} */ ({
    MouseClick: ['Screen position', '120,340', 'Where to click, from the top left of the screen: x,y, or x,y,right for a right click.'],
    RunScript: ['PowerShell script', 'Get-Process', 'Runs hidden. Needs "Allow RunScript actions" in Settings.'],
  });
  const field = $derived(
    d.seg >= 0 ? FIELD[d.seg] : (OTHER[d.kind] ?? [`${kindWord({ kind: d.kind, value: '', label: '', steps: [] })} value`, '', 'Set in the older editor; saved as it is.']),
  );
  const noValue = $derived(d.seg === -1 && d.kind === 'JoinMeeting');

  /** App or website: what was chosen on the chip, else what the text looks like. */
  const appOrSite = () => (d.kind === 'OpenUrl' || d.kind === 'LaunchApp' ? d.kind : looksLikeSite(d.value) ? 'OpenUrl' : 'LaunchApp');

  /** What the action box changed. A suggested name fills the name on the overlay only while it is empty. */
  function onedit(/** @type {{ value?: string, seg?: number, kind?: string, name?: string }} */ patch) {
    const { name, ...rest } = patch;
    edit({ ...rest, ...(name && !d.label.trim() ? { label: name } : {}) });
  }

  /** Set once Save is pressed with something missing, so the empty box is marked. */
  let tried = $state(false);
  const noName = $derived(!d.label.trim());
  // the builder checks each block before it hands the steps back
  const noAction = $derived(!noValue && (d.seg === 3 ? !d.steps.length : !d.value.trim()));

  async function save() {
    // A button with no name or no action does nothing when pressed, so both are asked for.
    tried = true;
    if (noName || noAction) {
      problem = noName && noAction ? 'Give the button a name and choose what it does.' : noName ? 'Give the button a name: it is shown on the overlay.' : d.seg === 3 ? 'Add at least one block in the builder.' : 'Choose what this button does.';
      return;
    }
    tried = false;
    busy = true;
    problem = '';
    let kind = d.seg === -1 ? d.kind : ['Shortcut', appOrSite(), 'PasteText', 'Automation'][d.seg];
    let value = noValue || d.seg === 3 ? '' : d.value;
    if (kind === 'OpenUrl' && value.trim() && !/^https?:\/\//i.test(value.trim())) value = `https://${value.trim()}`;
    // "Open with": the engine's app action already opens "browser|address" in that browser
    if (kind === 'OpenUrl' && d.browser) { kind = 'LaunchApp'; value = `${d.browser}|${value.trim()}`; }
    try {
      await saveButton(data.button, { kind, value, label: d.label, steps: d.seg === 3 ? d.steps : [] });
      draft = null;
      saved = true;
      pulse += 1;
    } catch (e) {
      problem = String(e);
    }
    busy = false;
  }

  /** Takes the name (and with it the label beside the pad) and the action off this button. */
  async function clear() {
    busy = true;
    problem = '';
    try {
      await saveButton(data.button, { kind: 'Shortcut', value: '', label: '', steps: [] }) // a blank value: nothing is bound;
      draft = null;
      saved = true;
    } catch (e) {
      problem = String(e);
    }
    busy = false;
  }

  const status_ = $derived(
    locked ? ['Not saved: file changed outside OASIS', 'var(--bad)']
      : busy ? ['Saving…', 'var(--signal)']
      : dirty ? ['Unsaved changes', 'var(--signal)']
      : ['Saved', 'var(--ok)'],
  );
  const ABOUT = [
    ['Presses a keyboard shortcut for you.', 'Example: Ctrl+C copies, Ctrl+Shift+M mutes your microphone.'],
    ['Opens an app, a folder or a website.', 'Example: Spotify, or youtube.com opens in your browser.'],
    null,
    ['Runs several actions in order, one after another.', 'Example: open Chrome, wait one second, then press Ctrl+T.'],
  ];

  // "Try it": runs the saved button as a press would. A key or paste goes to whatever window is in
  // front, so there is a few seconds to click the app first; opening an app or a site runs at once.
  let trying = $state(0);
  let tryNote = $state('');
  const stateText = $derived(tryNote || (trying ? `Click the app to try it in: ${trying}` : saved && !dirty && !locked ? 'Saved' : status_[0]));
  /** @type {ReturnType<typeof setInterval> | undefined} */
  let tryTimer;
  const cancelTry = () => { clearInterval(tryTimer); trying = 0; };
  async function runNow() {
    try {
      await invoke('try_button', { profile: current().index, config: currentConfig()?.index, button: data.button });
      tryNote = 'Sent. It runs now.';
    } catch (e) {
      problem = String(e);
    }
    setTimeout(() => (tryNote = ''), 4000);
  }
  function tryIt() {
    problem = '';
    tryNote = '';
    if (['LaunchApp', 'OpenUrl', 'QuickLaunch', 'JoinMeeting'].includes(base?.kind ?? '')) return runNow();
    trying = 3;
    tryTimer = setInterval(() => {
      trying -= 1;
      if (trying <= 0) { cancelTry(); runNow(); }
    }, 1000);
  }
  $effect(() => () => clearInterval(tryTimer));
</script>

<aside aria-label="Configure button">
  <div class="head">
    <div class="toprow">
      <span class="top"><b>Configure</b><span class="prof">{p.name}</span></span>
      <div class="big">Button <span class="n" style="color: {HUES[data.button]?.c}">0{data.button + 1}</span></div>
    </div>
    {#if data.input.keyboard}
      {@const parts = (data.input.keys[data.button] || '').split('+').map((s) => s.trim()).filter(Boolean)}
      <span class="lab mapped">Key Mapping</span>
      {#if editingKey}<KeyMapDialog onclose={() => (editingKey = false)} />{/if}
      {#if true}
        <button class="chord edit" disabled={locked} onclick={() => (editingKey = true)}
          aria-label={parts.length ? `Keyboard shortcut ${parts.join(' plus ')}. Click to change` : 'No keyboard shortcut set. Click to set one'}>
          {#each parts as part, j}
            {#if j}<span class="plus" aria-hidden="true">+</span>{/if}
            <span class="cap mono" class:main={j === parts.length - 1} style={paint(data.button, j === parts.length - 1)}>{part}</span>
          {:else}
            <span class="none">No key set. Click to choose one.</span>
          {/each}
          <svg class="pen" viewBox="0 0 24 24" aria-hidden="true"><path d="M4 20h4L19 9l-4-4L4 16zM13.5 6.5l4 4"/></svg>
        </button>
      {/if}
    {/if}
  </div>

  <div class="group">
    <span class="lab">Action type</span>
    <div class="seg" role="radiogroup" aria-label="Action type">
      {#each SEGMENTS as s, i}
        {#if i !== 2}<!-- "Text" is not offered; the index stays so older Paste-text buttons still load -->
        <button role="radio" aria-checked={d.seg === i} class:on={d.seg === i} disabled={locked}
          onclick={() => edit({ seg: i, kind: '' })}>
          <span class="ico" aria-hidden="true"><svg viewBox="0 0 24 24">{@html ICONS[i]}</svg></span><span class="t">{s}</span></button>{/if}
      {/each}
    </div>
    {#if ABOUT[d.seg]}{@const a = ABOUT[d.seg] ?? []}<div class="about"><b>{a[0]}</b><span>{a[1]}</span></div>{/if}
  </div>

  <label class="group">
    <span class="lab req-row">Name on the overlay <span class="req">required</span></span>
    <input class="name" class:miss={tried && noName} aria-invalid={tried && noName} value={d.label} readonly={locked} maxlength="14" oninput={(e) => edit({ label: e.currentTarget.value })} />
  </label>

  {#if d.seg === 3}
    {@const shown = d.steps.slice(0, 4)}
    <div class="group">
      <span class="lab">Steps</span>
      <button class="flowcard" class:miss={tried && !d.steps.length} disabled={locked} onclick={() => (building = true)}
        aria-label={d.steps.length ? `${d.steps.length} blocks. Open the builder to change them` : 'No blocks yet. Open the builder'}>
        {#if d.steps.length === 1 && d.steps[0].kind === 'AutoHotkey' && d.steps[0].x == null}
          <span class="chip mono">AutoHotkey script · {d.steps[0].value.split('\n').length} lines</span>
        {:else}
          {#each shown as s, i}{#if i}<span class="arrow" aria-hidden="true">→</span>{/if}<span class="chip">{STEP_NAME[s.kind] ?? s.kind}</span>{/each}
          {#if d.steps.length > shown.length}<span class="more">+{d.steps.length - shown.length}</span>{/if}
          {#if !d.steps.length}<span class="empty">No blocks yet</span>{/if}
        {/if}
        <svg class="pen" viewBox="0 0 24 24" aria-hidden="true"><path d="M4 20h4L19 9l-4-4L4 16zM13.5 6.5l4 4"/></svg>
      </button>
    </div>
  {:else if !noValue}
    <ActionField value={d.value} seg={d.seg} kind={d.kind} readonly={locked} label={field[0]} placeholder={field[1]} help={field[2]} {pulse} {onedit} />
    {#if d.seg === 1 && d.value.trim() && appOrSite() === 'OpenUrl' && (browsers.length || d.browser)}
      <!-- only for a website; a browser that is no longer installed stays listed so it is not lost on Save -->
      <div class="with">
        <span class="lab">Open with</span>
        <Select value={d.browser} label="Open with" disabled={locked} onchange={(x) => edit({ browser: x })}
          options={[{ value: '', label: 'Default browser' }, ...browsers, ...(d.browser && !browsers.some((b) => b.value === d.browser) ? [{ value: d.browser, label: d.browser }] : [])]} />
      </div>
    {/if}
  {:else}
    <div class="group"><span class="help">Nothing to enter for this action.</span></div>
  {/if}

  {#if building}
    <FlowBuilder steps={d.steps} button={data.button} onsave={saveFlow} onclose={() => (building = false)} />
  {/if}
  {#if d.seg === 3}<button class="add" disabled={locked} onclick={() => (building = true)}>{d.steps.length ? 'Open the builder' : '+ Add new'}</button>{/if}

  {#if problem}<div class="problem" role="alert">{problem}</div>{/if}
  <div class="line">
    <span class="acts">
    <button class="try" class:counting={trying > 0} disabled={trying === 0 && (locked || dirty || busy || !hasAction)}
      title={hasAction ? 'Run this button now to see it work' : 'Set what the button does first'} onclick={trying ? cancelTry : tryIt}>{trying ? `Cancel (${trying})` : 'Try it'}</button>
    {#if hasAction}
      <button class="clear" disabled={locked || busy} onclick={async () => { if (await sure({ title: 'Clear this button?', body: 'Its name and action are removed. This cannot be undone.', yes: 'Clear button' })) clear(); }}>
        <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M4 7h16M10 11v6M14 11v6M6 7l1 12a2 2 0 0 0 2 2h6a2 2 0 0 0 2-2l1-12M9 7V4h6v3" /></svg>Clear</button>
    {/if}
    </span>
    <span class="state mono" role="status"><span class="dot" style="background: {status_[1]}"></span>{#key stateText}<span class="fade">{stateText}</span>{/key}</span>
  </div>
  <div class="buttons">
    <button class="discard" disabled={!dirty} onclick={() => { draft = null; problem = ''; }}>Discard</button>
    <button class="save" disabled={!dirty || locked || busy} onclick={save}>Save</button>
  </div>
</aside>

<style>
  aside { width: clamp(420px, 30vw, 440px); flex: none; background: var(--sunk); border-radius: var(--r-panel); padding: 22px; display: flex; flex-direction: column; gap: 18px; min-height: 0; overflow: auto; }
  .head { display: flex; flex-direction: column; gap: 6px; padding-bottom: 26px; margin-bottom: 10px; border-bottom: 1px solid var(--line); }
  .toprow { display: flex; align-items: baseline; justify-content: space-between; gap: 10px; }
  .toprow .top { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .big { flex: none; font-size: 22px; font-weight: 700; letter-spacing: -.01em; line-height: 1.2; white-space: nowrap; }
  .n { color: var(--signal-ink); }
  /* Clear is a quiet text button in the footer row: there when wanted, not competing with Save */
  .acts { display: flex; align-items: center; gap: 8px; flex: none; }
  .clear { flex: none; display: flex; align-items: center; gap: 5px; height: 30px; padding: 0 12px 0 10px; border-radius: var(--r-pill); border: 1px solid var(--bad); background: transparent; color: var(--bad); font-size: 12px; font-weight: 600; }
  .clear svg { width: 13px; height: 13px; fill: none; stroke: currentColor; stroke-width: 1.8; stroke-linecap: round; stroke-linejoin: round; }
  .clear:hover:not(:disabled) { background: color-mix(in srgb, var(--bad) 10%, transparent); }
  .clear:disabled { opacity: .4; }
  .try.counting { background: var(--ink); color: var(--inv); border-color: var(--ink); }
  .try:disabled { opacity: .45; }
  /* the shortcut as keycaps in the button's own colour, the same as on the keyboard drawing */
  .chord { display: flex; flex-wrap: wrap; align-items: center; gap: 8px 10px; margin-top: 2px; }
  .mapped { margin-top: 20px; }
  button.chord.edit { align-self: flex-start; background: none; border: 0; padding: 0 0 6px; cursor: pointer; color: var(--muted); border-bottom: 2px dashed var(--faint); }
  button.chord.edit:hover:not(:disabled) { color: var(--ink); border-bottom-color: var(--ink); }
  /* the automation: its first blocks as chips, a pencil and the same dotted line as the key mapping, so it reads as "click to edit" */
  .flowcard { min-height: 44px; display: flex; flex-wrap: wrap; align-items: center; gap: 6px; padding: 6px 2px 8px; border: 0; border-bottom: 2px dotted var(--faint); border-radius: 0; background: transparent; color: var(--ink); text-align: left; cursor: pointer; }
  .flowcard:hover:not(:disabled) { border-bottom-color: var(--ink); }
  .flowcard.miss { border-bottom-color: var(--bad); }
  .flowcard .chip { padding: 4px 10px; border-radius: var(--r-pill); background: var(--hair); font-size: 12px; font-weight: 600; white-space: nowrap; }
  .flowcard .arrow, .flowcard .more, .flowcard .empty { font-size: 12px; color: var(--muted); }
  .flowcard .pen { margin-left: auto; color: var(--muted); }
  .pen { width: 16px; height: 16px; fill: none; stroke: currentColor; stroke-width: 1.8; stroke-linecap: round; stroke-linejoin: round; }
  .cap { min-width: 48px; height: 44px; padding: 0 16px; display: inline-flex; align-items: center; justify-content: center; border-radius: 9px; font-size: 17px; font-weight: 600; color: #fff; background: var(--c); box-shadow: 0 4px 0 var(--e); margin-bottom: 4px; }
  .cap:not(.main) { background: var(--c); }
  .plus { color: var(--muted); font-size: 15px; }
  .none { font-size: 12px; color: var(--muted); }
  .mono { font-family: var(--mono); }
  .top { display: flex; align-items: center; gap: 8px; font-size: 17px; font-weight: 700; letter-spacing: -.01em; color: var(--ink); }
  .top b { font-weight: 700; }
  .prof { min-width: 0; overflow: hidden; text-overflow: ellipsis; padding: 2px 10px; border-radius: var(--r-pill); background: color-mix(in srgb, var(--signal) 14%, transparent); color: var(--signal-ink); font-size: 12px; font-weight: 600; letter-spacing: .02em; }
  .lab { font: 500 11px/1.3 var(--mono); letter-spacing: .12em; text-transform: uppercase; color: var(--muted); }
  .mono { font-family: var(--mono); }
  .group { display: flex; flex-direction: column; gap: 8px; }
  .req-row { display: flex; justify-content: space-between; align-items: baseline; }
  .with { display: flex; align-items: center; justify-content: space-between; gap: 12px; margin-top: -6px; }
  .with :global(button) { min-width: 180px; }
  .req { text-transform: none; letter-spacing: 0; font-weight: 600; color: var(--bad); }
  .name.miss { border-color: var(--bad); }
  .seg { display: flex; gap: 4px; padding: 4px; border-radius: var(--r-pill); background: var(--hair); }
  .seg button { flex: 1; min-width: 0; height: 44px; display: flex; align-items: center; justify-content: center; gap: 8px; padding: 0 8px; border: 0; border-radius: var(--r-pill); background: transparent; color: var(--muted); font-size: 12px; font-weight: 600; white-space: nowrap; transition: background-color .24s var(--ease), color .24s var(--ease); }
  .seg .ico { flex: none; width: 28px; height: 28px; border-radius: 50%; display: grid; place-items: center; background: var(--surface); color: var(--ink); transition: background-color .24s var(--ease), color .24s var(--ease); }
  .seg .ico svg { width: 17px; height: 17px; fill: none; stroke: currentColor; stroke-width: 1.8; stroke-linecap: round; stroke-linejoin: round; }
  .seg .t { overflow: hidden; text-overflow: ellipsis; }
  .seg button.on .ico { background: var(--ink); color: var(--inv); }
  .seg button.on { background: var(--surface); color: var(--ink); box-shadow: var(--shadow-chosen); }
  .about { display: flex; flex-direction: column; gap: 2px; margin-top: 6px; padding: 0 6px; font-size: 12.5px; line-height: 1.45; color: var(--muted); }
  .about b { color: var(--ink); font-weight: 600; opacity: .8; }
  .group:has(.seg) { margin-bottom: 10px; }
  .seg button:disabled { cursor: default; }
  input { height: 42px; border-radius: var(--r-row); border: 1.5px solid var(--line); background: var(--surface); color: var(--ink); padding: 0 14px; font-size: 13px; outline: none; min-width: 0; }
  input:focus-visible { border-color: var(--ink); }
  .name { font-size: 15px; font-weight: 600; }
  .help { font-size: 12px; color: var(--muted); }
  .add { height: 34px; border-radius: var(--r-pill); border: 1px dashed var(--faint); background: transparent; font-size: 12px; font-weight: 600; align-self: flex-start; padding: 0 14px; margin-bottom: auto; }
  .line { margin-top: auto; display: flex; align-items: center; justify-content: space-between; gap: 10px; }
  .try { flex: none; white-space: nowrap; height: 30px; padding: 0 12px; border-radius: var(--r-pill); border: 1px solid var(--line); background: transparent; font-size: 12px; font-weight: 600; }
  .state { font-size: 11px; color: var(--muted); display: flex; align-items: center; justify-content: flex-end; gap: 6px; min-width: 0; text-align: right; }
  .dot { width: 6px; height: 6px; border-radius: 50%; }
  .fade { animation: fade .24s var(--ease); }
  @keyframes fade { from { opacity: 0; } }
  .buttons { display: flex; gap: 10px; }
  .buttons button { flex: 1; height: 48px; border-radius: var(--r-bar); font-size: 15px; font-weight: 600; }
  .discard { border: 1.5px solid var(--line); background: transparent; color: var(--muted); }
  .save { border: 0; background: var(--ink); color: var(--inv); }
  /* Dark: a light button at 40% turned muddy grey; a quiet graphite one with faint text reads as "not yet". */
  :global(:root[data-theme='dark']) .save:disabled { opacity: 1; background: var(--side); color: var(--faint); }
  button:disabled { opacity: .4; cursor: default; }
  .problem { font-size: 12px; color: var(--bad); }
  /* a short window: the same panel, tighter, so it fits without a scroll bar */
  @media (max-height: 800px) {
    aside { gap: 12px; padding: 18px 22px; }
    .head { padding-bottom: 14px; margin-bottom: 0; }
    .group:has(.seg) { margin-bottom: 2px; }
    .group { gap: 6px; }
    .mapped { margin-top: 6px; }
    .cap { height: 34px; min-width: 40px; padding: 0 12px; font-size: 15px; }
  }
  /* a very short window (the smallest the app allows): the mapped key moves up beside Button 0N, examples go */
  @media (max-height: 700px) {
    .head { display: grid; grid-template-columns: minmax(0, 1fr) auto auto; align-items: center; column-gap: 12px; padding-bottom: 12px; }
    .toprow { display: contents; }
    .toprow .top { order: 0; }
    .head .chord { order: 1; margin: 0; flex-wrap: nowrap; gap: 4px; }
    .toprow .big { order: 2; }
    .mapped { display: none; }
    .head .none { display: none; }
    .cap { height: 30px; min-width: 34px; padding: 0 9px; font-size: 13px; border-radius: 7px; }
    .about span { display: none; }
    aside { gap: 10px; padding: 16px 20px; }
  }
</style>
