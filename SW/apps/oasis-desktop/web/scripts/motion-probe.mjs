// Motion probe: loads the built page in headless Chrome with a stub Tauri API and prints the real
// animations (property, duration, delay), so the timings in DESIGN.md section 0 are measured, not
// read off the CSS. Run `node scripts/motion-probe.mjs` (or `... reduced` for prefers-reduced-motion).
//
// Needs, started by you first (its own Chrome profile folder, so no other Chrome is touched):
//   npx vite preview --port 4173 --strictPort
//   chrome --headless=new --disable-gpu --remote-debugging-port=9333 --user-data-dir=<a scratch folder> about:blank
// then stop those two processes by their IDs afterwards.
// Loads the built page in headless Chrome with a stub Tauri API and reports the real animations.
const PORT = 9333;
const base = `http://127.0.0.1:${PORT}`;
const tab = await (await fetch(`${base}/json/new?about:blank`, { method: 'PUT' })).json();
const ws = new WebSocket(tab.webSocketDebuggerUrl);
await new Promise((r) => (ws.onopen = r));
let id = 0;
const pending = new Map();
ws.onmessage = (m) => { const d = JSON.parse(m.data); if (d.id && pending.has(d.id)) { pending.get(d.id)(d); pending.delete(d.id); } };
const send = (method, params = {}) => new Promise((res) => { const i = ++id; pending.set(i, res); ws.send(JSON.stringify({ id: i, method, params })); });
const evaluate = async (expression) => {
  const r = await send('Runtime.evaluate', { expression, awaitPromise: true, returnByValue: true });
  if (r.result.exceptionDetails) throw new Error(JSON.stringify(r.result.exceptionDetails.exception?.description ?? r.result.exceptionDetails));
  return r.result.result.value;
};

const reduced = process.argv[2] === 'reduced';
await send('Page.enable');
await send('Emulation.setDeviceMetricsOverride', { width: 1180, height: 760, deviceScaleFactor: 1, mobile: false });
if (reduced) await send('Emulation.setEmulatedMedia', { features: [{ name: 'prefers-reduced-motion', value: 'reduce' }] });

const stub = `
  const button = (n) => ({ kind: 'Shortcut', value: 'Ctrl+' + n, label: 'B' + n, steps: [] });
  const profile = (i, name, match) => ({ index: i, name, context_match: match, kind: 'app', primary_config: 0, multi: false, label_mode: '', icon_emoji: '',
    configs: [{ index: 0, name, context_match: '*', is_primary: true, can_delete: false, when: '', panel_id: '', panel_label: '', buttons: [button(1), button(2), button(3)] }] });
  let profiles = [...Array.from({ length: 13 }, (_, i) => profile(i, 'P' + i, 'p' + i)), profile(13, 'Everywhere', '*')];
  const engine = { running: true, pad: 'Connected', pad_name: 'OASIS', battery_percent: 67, charging: false, latency_ms: 8, profiles_loaded: 4, live: false, integrity_ok: true,
    last_press: null, last_error: null, bridge_status: 'OFF', bridge_clients: 0, youtube: '', youtube_title: '', voice: 'off', front_app: null, front_profile: null };
  window.__engine = engine;
  window.__setProfiles = (v) => { profiles = v; };
  window.__TAURI__ = {
    core: { invoke: async (cmd, args) => {
      if (cmd === 'status') return { engine: window.__noEngine ? null : engine, error: window.__error ?? null };
      if (cmd === 'settings_view') return { profiles, action_types: ['Shortcut','LaunchApp','OpenUrl','MouseClick','PasteText','RunScript','JoinMeeting','Automation'], step_types: ['Delay'], writable_step_types: ['Delay'], panel_presets: [], when_options: [], max_sets: 8 };
      if (cmd === 'save_button') { const b = profiles[args.profile].configs[args.config].buttons[args.button]; Object.assign(b, args.edit); return null; }
      if (cmd === 'profile_use_counts') return {};
      if (cmd === 'installed_apps') return [];
      return {};
    } },
    event: { listen: async (name, cb) => { (window.__listeners ||= []).push(cb); return () => {}; } },
    window: { getCurrentWindow: () => ({ minimize() {}, toggleMaximize() {}, close() {} }) },
    app: { getVersion: async () => '0.1.0' },
  };`;
await send('Page.addScriptToEvaluateOnNewDocument', { source: stub });
await send('Page.navigate', { url: 'http://localhost:4173/app.html' });
await new Promise((r) => setTimeout(r, 2500));

await evaluate(`window.__sleep = (ms) => new Promise((r) => setTimeout(r, ms));
  window.__raf = () => new Promise((r) => requestAnimationFrame(r));
  window.__anims = async () => { await __raf(); await __raf(); return document.getAnimations().map((a) => { const t = a.effect.getComputedTiming(); const el = a.effect.target;
    return [(el.className && el.className.baseVal !== undefined ? el.className.baseVal : String(el.className)).split(' ')[0] || el.tagName, a.transitionProperty ?? a.animationName, Math.round(t.duration), Math.round(t.delay)]; }); };`);

const show = (title, rows) => console.log(`\n${title}\n` + rows.map((r) => '  ' + r.join(' | ')).join('\n'));

// `node scripts/motion-probe.mjs states <folder>`: the states of DESIGN.md 6.7, one by one, with a
// screenshot of each in <folder>. The stub engine is changed and the window must show it.
if (process.argv[2] === 'states') {
  const { writeFileSync } = await import('node:fs');
  const folder = process.argv[3] ?? '.';
  const shot = async (name) => {
    const r = await send('Page.captureScreenshot', { format: 'png' });
    writeFileSync(`${folder}/state-${name}.png`, Buffer.from(r.result.data, 'base64'));
  };
  await evaluate(`window.__emit = (payload) => (window.__listeners || []).forEach((cb) => cb({ payload }));`);
  let failed = 0;
  const check = async (what, expr, want) => {
    const got = await evaluate(expr);
    const ok = JSON.stringify(got) === JSON.stringify(want);
    if (!ok) failed++;
    console.log(`${ok ? 'PASS' : 'FAIL'}  ${what}: ${JSON.stringify(got)}${ok ? '' : ` (wanted ${JSON.stringify(want)})`}`);
  };
  const settle = (ms = 1400) => evaluate(`__sleep(${ms})`);

  await check('dry run is the start: the switch says DRY RUN', `document.querySelector('.live').textContent.trim()`, 'DRY RUN');
  await evaluate(`__engine.live = true`); await settle();
  await check('live: the switch says LIVE', `document.querySelector('.live').textContent.trim()`, 'LIVE');
  await evaluate(`__engine.live = false`);

  await evaluate(`__engine.pad = 'Scanning'`); await settle();
  await check('pad lost: the card says Pad lost', `document.querySelector('#padText').textContent`, 'Pad lost');
  await check('pad lost: the pad drawing is dimmed', `document.querySelectorAll('.pad.waiting').length`, 1);
  await check('pad lost: and says it is waiting', `document.querySelector('.waiting-note')?.textContent`, 'Waiting for the pad');
  await check('pad lost: editing still works (the name box is not read-only)', `document.querySelector('header input').readOnly`, false);
  await shot('pad-lost');
  await evaluate(`__engine.pad = 'Connected'`);

  await evaluate(`__engine.pad = 'Unavailable'; __engine.last_error = 'Turn Bluetooth on.'`); await settle();
  await check('Bluetooth off: a banner on the screen', `document.querySelector('#padProblem')?.textContent.includes('Bluetooth is not available')`, true);
  await evaluate(`__engine.pad = 'Connected'; __engine.last_error = null`);

  await evaluate(`__engine.integrity_ok = false`); await settle();
  await check('file changed outside: a banner on the screen', `!!document.querySelector('#integrity')`, true);
  await check('file changed outside: the name box is read-only', `document.querySelector('header input').readOnly`, true);
  await check('file changed outside: Live cannot be switched on', `document.querySelector('#live').disabled`, true);
  await check('file changed outside: the panel says nothing is saved', `document.querySelector('aside .state').textContent.trim()`, 'Not saved: file changed outside OASIS');
  await check('file changed outside: Save is off', `[...document.querySelectorAll('aside button')].find((b) => b.textContent.trim() === 'Save').disabled`, true);
  await shot('file-changed-outside');
  await evaluate(`__engine.integrity_ok = true`);

  await evaluate(`window.__error = 'no Bluetooth adapter'`); await settle();
  await check('error: a banner says OASIS could not start', `document.querySelector('#problem')?.textContent.includes('could not start')`, true);
  await shot('error');
  await evaluate(`window.__error = null`);

  await evaluate(`__setProfiles([]); __emit({ type: 'settings_changed' })`); await settle(700);
  await check('no profiles yet: says so, with a way to make one', `document.querySelector('.empty')?.textContent.includes('No profiles yet')`, true);
  await check('no profiles yet: no pad drawing, no Configure panel', `document.querySelectorAll('.pad, aside[aria-label="Configure button"]').length`, 0);
  await shot('no-profiles');

  console.log(failed ? `\n${failed} FAILED` : '\nall passed');
  ws.close();
  setTimeout(() => process.exit(failed ? 1 : 0), 100);
  await new Promise(() => {});
}

// `node scripts/motion-probe.mjs pad`: the pad runs the window. Fires the events the engine sends
// (window_pad on release, pad_button down and up) and checks what the window does.
if (process.argv[2] === 'pad') {
  await evaluate(`window.__emit = (payload) => (window.__listeners || []).forEach((cb) => cb({ payload }));
    window.__name = () => document.querySelector('header input').value;
    window.__button = () => document.querySelector('aside .big .n').textContent;
    window.__section = () => document.querySelector('nav [aria-current="page"]').dataset.section;
    window.__press = async (button, hold = false) => { __emit({ type: 'window_pad', button, hold }); await __sleep(450); };`);
  let failed = 0;
  const check = async (what, expr, want) => {
    const got = await evaluate(expr);
    const ok = JSON.stringify(got) === JSON.stringify(want);
    if (!ok) failed++;
    console.log(`${ok ? 'PASS' : 'FAIL'}  ${what}: ${JSON.stringify(got)}${ok ? '' : ` (wanted ${JSON.stringify(want)})`}`);
  };
  await check('opens on the first profile', `__name()`, 'P0');
  await evaluate(`__press(3)`); await check('03 tap: next profile', `__name()`, 'P1');
  await evaluate(`__press(3)`); await check('03 tap again', `__name()`, 'P2');
  await evaluate(`__press(1)`); await check('01 tap: previous profile', `__name()`, 'P1');
  await check('button 01 is selected', `__button()`, '01');
  await evaluate(`__press(2)`); await check('02 tap: next button', `__button()`, '02');
  await evaluate(`__press(2)`); await check('02 tap again', `__button()`, '03');
  await evaluate(`__press(2)`); await check('02 tap wraps round', `__button()`, '01');

  // the key flash: a real press lights the legend cap and the key on the drawing
  await evaluate(`__emit({ type: 'pad_button', button: 1, down: true }); __sleep(30)`);
  await check('flash while 01 is down: legend caps lit', `document.querySelectorAll('.cap.lit').length`, 1);
  await check('flash while 01 is down: pad keys lit', `document.querySelectorAll('.key.flash').length`, 1);
  await evaluate(`__emit({ type: 'pad_button', button: 1, down: false }); __sleep(600)`);
  await check('flash gone after release', `document.querySelectorAll('.cap.lit, .key.flash').length`, 0);

  // holds change section, round the list
  await evaluate(`__press(3, true)`); await check('hold 03: next section', `__section()`, 'quick-launch');
  await evaluate(`__press(3, true)`); await check('hold 03 again', `__section()`, 'voice');
  await evaluate(`__press(3)`); await evaluate(`__press(1, true)`); await check('hold 01: previous section (a tap there does nothing)', `__section()`, 'quick-launch');
  await evaluate(`__press(1, true)`); await check('hold 01 again', `__section()`, 'profiles');
  await check('the tap on another section changed no profile', `__name()`, 'P1');
  await evaluate(`__press(1, true)`); await check('hold 01 from the first: round to the last', `__section()`, 'settings');
  await evaluate(`__press(3, true)`); await check('hold 03 from the last: round to the first', `__section()`, 'profiles');

  // hold 02 is back
  await evaluate(`[...document.querySelectorAll('header .chip')].find((b) => b.textContent.includes('Labels')).click(); __sleep(100)`);
  await check('a popover is open', `!!document.querySelector('[role="dialog"]')`, true);
  await evaluate(`__press(2, true)`); await check('hold 02: the popover closes, the section stays', `[!!document.querySelector('[role="dialog"]'), __section()]`, [false, 'profiles']);
  await evaluate(`document.querySelector('#action-box').focus(); __sleep(50)`);
  await evaluate(`__press(2, true)`); await check('hold 02: leaves the box you are typing in', `document.activeElement.id === 'action-box'`, false);
  await evaluate(`__press(3, true); __sleep(50)`); await evaluate(`__press(2, true)`); await check('hold 02 on another section: back to Profiles', `__section()`, 'profiles');

  console.log(failed ? `\n${failed} FAILED` : '\nall passed');
  ws.close();
  setTimeout(() => process.exit(failed ? 1 : 0), 100);
  await new Promise(() => {});
}

// 1. change profile: click "next" once
await evaluate(`document.querySelector('[aria-label="Previous profile"]').click(); undefined`); // settle: first profile may already be first
await evaluate(`__sleep(900)`);
await evaluate(`document.querySelector('[aria-label="Next profile"]').click(); undefined`);
const out = await evaluate(`__anims()`);
show('CHANGE PROFILE, first frames (out): class | property | duration ms | delay ms', out.filter((r) => ['track', 'row', 'svelte-'].some((k) => String(r[0]).includes(k))));
await evaluate(`__sleep(230)`);
const inn = await evaluate(`__anims()`);
show('CHANGE PROFILE, after the swap (in): class | property | duration ms | delay ms', inn.filter((r) => String(r[0]) === 'row' || String(r[0]).startsWith('row') || String(r[0]).includes('row')));

// 2. holding next: three quick presses, each interrupting the last, must end on the third profile
await evaluate(`__sleep(900)`);
await evaluate(`(async () => { const b = document.querySelector('[aria-label="Next profile"]'); b.click(); await __sleep(40); b.click(); await __sleep(40); b.click(); })()`);
await evaluate(`__sleep(900)`);
console.log('\nTHREE QUICK "next" PRESSES: header shows',JSON.stringify(await evaluate(`document.querySelector('header input').value`)), '(expected "P4")');
console.log('STRIP GLIDE: .track transition =', await evaluate(`(() => { const t = document.querySelector('[role="listbox"] > div'); const c = getComputedStyle(t); return c.transitionProperty + ' ' + c.transitionDuration + ' ' + c.transitionTimingFunction + ' | now translateX ' + c.transform; })()`));
// 3. section change
await evaluate(`__sleep(600)`);
await evaluate(`document.querySelector('[data-section="voice"]').click(); undefined`);
show('SECTION CHANGE: class | property | duration ms | delay ms', (await evaluate(`__anims()`)).filter((r) => String(r[0]).includes('panel') || String(r[0]).includes('tick') || String(r[0]).includes('cur')));
await evaluate(`__sleep(400)`);
const transforms = await evaluate(`[...document.querySelectorAll('main > section')].map((s) => [s.getAttribute('aria-label'), getComputedStyle(s).opacity, getComputedStyle(s).transform])`);
show('PANEL STATE after the change (label | opacity | transform)', transforms);

// 4. Save pulse
await evaluate(`document.querySelector('[data-section="profiles"]').click(); undefined`);
await evaluate(`__sleep(500)`);
await evaluate(`(() => { const i = document.querySelector('#action-box'); i.focus(); i.value = 'Ctrl+Alt+Q'; i.dispatchEvent(new Event('input', { bubbles: true })); })()`);
await evaluate(`__sleep(200)`);
await evaluate(`[...document.querySelectorAll('button')].find((b) => b.textContent.trim() === 'Save').click(); undefined`);
show('SAVE: class | property | duration ms | delay ms', (await evaluate(`__anims()`)).filter((r) => String(r[0]).includes('ring') || String(r[0]).includes('fade')));

console.log(reduced ? '\n(reduced motion emulated)' : '');
if (reduced) {
  const all = await evaluate(`(async () => { document.querySelector('[aria-label="Next profile"]').click(); await __raf(); await __raf(); return document.getAnimations().map((a) => Math.round(a.effect.getComputedTiming().duration)); })()`);
  console.log('REDUCED MOTION: animation durations right after a profile change:', JSON.stringify(all));
  const swapped = await evaluate(`document.querySelector('header input').value`);
  console.log('header shows (no fade, swapped at once):', swapped);
}
ws.close();
await fetch(`${base}/json/close/${tab.id}`);
setTimeout(() => process.exit(0), 100);
