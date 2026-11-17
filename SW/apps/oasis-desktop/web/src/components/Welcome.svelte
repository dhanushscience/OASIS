<script>
  // The first-run welcome window: the small window that opens in place of the application.
  // It shows what OASIS does before it says it, then asks the two or three things the guide needs.
  // Which screen is up comes from the guide's one state (guide/onboarding.svelte.js), never from here.
  import { onMount } from 'svelte';
  import MrScience from './MrScience.svelte';
  import TryCommands from './TryCommands.svelte';
  import { svg } from '../guide/mrscience.js';
  import { HUES } from '../hues.js';
  import { guide, startGuide, send } from '../guide/onboarding.svelte.js';
  import { LINES, BLOCKED, DOWNLOAD, PITCH, PITCH_PARTS, PITCH_PARTS_OPEN } from '../guide/lines.js';

  const tauri = /** @type {any} */ (window).__TAURI__;
  const invoke = (/** @type {string} */ c, /** @type {object} */ a = {}) => tauri.core.invoke(c, a);
  const theme = document.documentElement.dataset.theme === 'dark' ? 'dark' : 'light';
  const closeWindow = () => tauri?.window.getCurrentWindow().close();

  /** The opening scene has been seen; the device question is up. */
  let asked = $state(false);
  /** Which key Mr. Science is on in the opening: 0 dictates, 1 opens an app. */
  let scene = $state(0);
  const parts = $derived(scene === 0 ? PITCH_PARTS : PITCH_PARTS_OPEN);
  const screen = $derived(!guide.loaded || (guide.step === 'welcome' && !asked) ? 'intro' : guide.step === 'welcome' ? 'device' : guide.step);

  // The model download, said in one plain line on every screen. Read only: the guide never starts or stops it.
  let models = $state(/** @type {any} */ (null));
  const download = $derived.by(() => {
    if (!models) return '';
    const text = /** @type {Record<string, string>} */ (DOWNLOAD)[models.phase] ?? '';
    return models.phase === 'downloading' && models.total_mb ? `${text} ${Math.round((models.done_mb / models.total_mb) * 100)}%` : text;
  });

  // Browser: the installed ones, with the saved choice marked.
  let browsers = $state(/** @type {{ value: string, label: string }[]} */ ([]));
  let browser = $state('');
  let savedBrowser = '';

  onMount(async () => {
    startGuide();
    if (!tauri) return;
    tauri.event.listen('models-status', (/** @type {{ payload: any }} */ e) => (models = e.payload));
    try { models = await invoke('models_status'); } catch { /* the line stays empty */ }
    try {
      const reply = await invoke('settings_fields');
      const field = reply.sections.flatMap((/** @type {any} */ s) => s.fields).find((/** @type {any} */ f) => f.id === 'preferences.browser');
      browsers = field?.options.filter((/** @type {any} */ o) => o.value) ?? [];
      browser = savedBrowser = field?.value || browsers[0]?.value || '';
      const micField = reply.sections.flatMap((/** @type {any} */ s) => s.fields).find((/** @type {any} */ f) => f.id === 'voice.microphone');
      mics = micField?.options ?? [];
      mic = savedMic = micField?.value ?? '';
    } catch { /* no list: the step is passed over by the guide when there are fewer than two */ }
  });

  // Microphone: asked before the voice part. The PC's default (the empty choice) is picked first.
  let mics = $state(/** @type {{ value: string, label: string }[]} */ ([]));
  let mic = $state('');
  let savedMic = '';
  let micFound = $state(true);
  $effect(() => {
    if (screen !== 'microphone') return;
    invoke('onboarding_check_mic').then((/** @type {{ names: string[] }} */ r) => (micFound = r.names.length > 0)).catch(() => {});
  });
  async function micChosen() {
    if (mic !== savedMic) await invoke('set_setting', { id: 'voice.microphone', value: mic }).catch(() => {});
    send('next');
  }

  // Vertical or horizontal pop-up: one click sets the overlay, the voice pop-up and the keys, and moves on.
  const LAYOUTS = [
    { vertical: false, name: 'Horizontal', where: 'Along the bottom', keys: ['F1', 'F2', 'F3'] },
    { vertical: true, name: 'Vertical', where: 'At the left edge', keys: ['`', 'Tab', 'Caps'] },
  ];
  let layoutNote = $state('');
  async function layoutChosen(/** @type {boolean} */ vertical) {
    layoutNote = '';
    try {
      await invoke('onboarding_layout', { vertical });
      send('next');
    } catch (e) {
      layoutNote = String(e);
    }
  }

  async function browserChosen() {
    if (browser && browser !== savedBrowser) await invoke('set_setting', { id: 'preferences.browser', value: browser }).catch(() => {});
    send('browser_chosen');
  }

  // Groq, only on a PC too light for a local model. Pressing Test is the consent: it is the first thing that sends anything.
  let key = $state('');
  let testing = $state(false);
  let passed = $state(false);
  let groqNote = $state('');
  async function testKey() {
    testing = true;
    groqNote = '';
    try {
      await invoke('set_groq_key', { key });
      const result = await invoke('onboarding_test_groq');
      passed = result.ok;
      groqNote = result.text;
      if (!passed) await invoke('set_groq_key', { key: '' }); // a key that failed is not kept
    } catch (e) {
      passed = false;
      groqNote = String(e);
    }
    testing = false;
  }
  async function useGroq() {
    await invoke('set_setting', { id: 'voice.stt_provider', value: 1 }).catch(() => {});
    send('groq_done');
  }

  // The exit: he hops off the window, then it closes and the application takes its place.
  let leaving = $state(false);
  function leave() {
    leaving = true;
    setTimeout(closeWindow, matchMedia('(prefers-reduced-motion: reduce)').matches ? 0 : 620);
  }
</script>

<svelte:window onkeydown={(e) => e.key === 'Escape' && send('skip_all')} />

<main>
  <header data-tauri-drag-region>
    <img class="logo" src={theme === 'dark' ? './oasis-logo-paper.png' : './oasis-logo-ink.png'} alt="OASIS" data-tauri-drag-region />
    <button class="x" aria-label="Close" onclick={closeWindow}>
      <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M6 6l12 12M18 6L6 18" /></svg>
    </button>
  </header>

  {#if screen === 'intro'}
    <div class="words">
      <h1>{PITCH[0]}<br />{PITCH[1]}</h1>
      <!-- the three parts light up as the scene below acts each one out -->
      <p><span class="b1">{parts[0]}</span> <span class="b2">{parts[1]}</span> <span class="b3">{parts[2]}</span></p>
    </div>

    <div class="stage" aria-hidden="true">
      <div class="pad">
        {#each HUES as h, i}
          <div class="col">
            {#if i === scene}
              <div class="sci" class:second={scene === 1}>
                {@html svg(theme, 5)}
                <!-- his callout: he says who he is, and while the key is held it shows his voice -->
                <span class="callout">
                  <span class="hello">{LINES.welcome[0]}</span>
                  <span class="voice">{#each [0, 1, 2, 3, 4] as b}<i style="animation-delay: {b * -0.17}s"></i>{/each}</span>
                </span>
              </div>
            {/if}
            <div class="key" class:held={i === scene} style="--c:{h.c};--e:{h.e}"><span>0{i + 1}</span></div>
          </div>
        {/each}
      </div>
      {#if scene === 0}
        <div class="note">
          <div class="bar"><span>Notepad</span></div>
          <!-- he says his name; the first typing is a slip, then OASIS cleans it up on its own -->
          <div class="text">
            <div class="ln"><span class="typed">my name is danush</span><span class="fixed">My name is Dhanush.</span><span class="caret"></span></div>
            <span class="tag">cleaned up</span>
          </div>
        </div>
      {:else}
        <!-- key 2: he says "open Notes", the little app opens -->
        <div class="launch">
          <span class="said">"open Notes"</span>
          <div class="note app">
            <div class="bar"><span>Notes</span></div>
            <div class="body"><i></i><i></i><i></i></div>
          </div>
        </div>
      {/if}
    </div>
  {:else if screen === 'try_commands'}
    <!-- after the voice part: he says it in one line on top, the practice fills the stage under it -->
    <div class="talk practice">
      <div class="line"><MrScience scale={2} /><div class="bubble">{LINES.try_commands[0]}</div></div>
      <div class="tryc"><TryCommands compact /></div>
    </div>
  {:else}
    <div class="talk">
      <div class="who" class:leaving><MrScience scale={7} mood={screen === 'exit' ? 'success' : screen === 'groq' ? 'thinking' : 'idle'} /></div>
      <div class="say">
        {#if screen === 'device'}
          <div class="bubble">{LINES.welcome[1]}</div>
          <div class="choices">
            <button class="choice" onclick={() => send('device_answer', true)}>Yes, I have one</button>
            <button class="choice" onclick={() => send('device_answer', false)}>Not yet, I'll use my keyboard</button>
          </div>
        {:else if screen === 'layout'}
          <div class="bubble">{LINES.layout[0]}</div>
          <div class="choices row">
            {#each LAYOUTS as l}
              <button class="lay" onclick={() => layoutChosen(l.vertical)}>
                <!-- a small screen with the three bars where they will sit -->
                <span class="mini" class:vert={l.vertical} aria-hidden="true">
                  <span class="bars">{#each HUES as h}<i style="--c:{h.c}"></i>{/each}</span>
                </span>
                <span class="lname">{l.name}</span>
                <span class="lwhere">{l.where}</span>
                <span class="caps">{#each l.keys as k}<kbd>{k}</kbd>{/each}</span>
              </button>
            {/each}
          </div>
          <p class="fine">The keys are buttons 1, 2 and 3. You can change both later in Settings.</p>
          {#if layoutNote}<div class="note2" role="status">{layoutNote}</div>{/if}
        {:else if screen === 'browser'}
          <div class="bubble">{LINES.browser[0]}</div>
          <div class="choices row" role="radiogroup" aria-label="Browser">
            {#each browsers as b}
              <button class="pick" role="radio" aria-checked={browser === b.value} class:on={browser === b.value} onclick={() => (browser = b.value)}>{b.label}</button>
            {/each}
          </div>
          <div class="choices"><button class="go" onclick={browserChosen}>Looks good</button></div>
        {:else if screen === 'microphone'}
          <div class="bubble">{LINES.microphone[0]}</div>
          <div class="choices row" role="radiogroup" aria-label="Microphone">
            {#each mics as m}
              <button class="pick" role="radio" aria-checked={mic === m.value} class:on={mic === m.value} onclick={() => (mic = m.value)}>{m.value === '' ? 'PC default' : m.label}</button>
            {/each}
          </div>
          {#if !micFound}<p class="fine warn">{BLOCKED.no_microphone} The voice part needs one; you can plug one in and carry on.</p>{/if}
          <div class="choices row">
            <button class="go" onclick={micChosen}>Looks good</button>
            <button class="quiet" onclick={() => invoke('onboarding_open_mic_settings').catch(() => {})}>Windows microphone settings</button>
          </div>
        {:else if screen === 'groq'}
          <div class="bubble">{LINES.groq[0]}</div>
          <p class="fine">Groq is an online service. With it, what you say is sent to Groq's servers to be turned into text. Pressing <b>Test</b> sends a short silent sample to check your key. Nothing is sent before that.</p>
          <div class="keyrow">
            <input type="password" placeholder="Paste your Groq key" autocomplete="off" spellcheck="false" bind:value={key} oninput={() => { passed = false; groqNote = ''; }} />
            <button class="quiet" disabled={!key.trim() || testing} onclick={testKey}>{testing ? 'Testing…' : 'Test'}</button>
          </div>
          <div class="note2" class:ok={passed} role="status">{groqNote}</div>
          <div class="choices row">
            <button class="quiet" onclick={() => invoke('onboarding_open_groq_keys').catch(() => {})}>Get a key from Groq</button>
            <button class="quiet" onclick={() => send('groq_declined')}>Not now</button>
            <button class="go" disabled={!passed} onclick={useGroq}>Use Groq</button>
          </div>
        {:else}
          <div class="bubble">{LINES.exit[0]}</div>
          <div class="choices"><button class="go" disabled={leaving} onclick={leave}>Open OASIS</button></div>
        {/if}
      </div>
    </div>
  {/if}

  <footer>
    <span class="hint" role="status">{download}</span>
    <span class="end">
      {#if screen === 'device' || screen === 'layout' || screen === 'browser' || screen === 'microphone' || screen === 'groq'}
        <button class="quiet" onclick={() => (screen === 'device' ? (asked = false) : send('back'))}>Back</button>
      {:else if screen === 'intro' && scene === 1}
        <button class="quiet" onclick={() => (scene = 0)}>Back</button>
      {/if}
      {#if screen === 'try_commands'}
        <button class="skip" onclick={() => send('skip_step')}>Skip</button>
        <button class="go" onclick={() => send('next')}>Finish</button>
      {:else}
        <button class="skip" onclick={() => send('skip_all')}>Skip the guide</button>
      {/if}
      {#if screen === 'intro'}<button class="go" onclick={() => (scene === 0 ? (scene = 1) : (asked = true))}>{scene === 0 ? 'Next' : 'Get started'}</button>{/if}
    </span>
  </footer>
</main>

<style>
  :global(html), :global(body) { background: var(--surface); overflow: hidden; }
  main { --loop: 7s; height: 100vh; display: flex; flex-direction: column; border: 1px solid var(--line); }

  header { height: 52px; flex: none; display: flex; align-items: center; justify-content: space-between; padding: 0 6px 0 28px; }
  .logo { height: 26px; width: auto; display: block; }
  .x { width: 32px; height: 28px; display: grid; place-items: center; border: 0; border-radius: var(--r-field); background: transparent; color: var(--ink); transition: background-color .24s var(--ease); }
  .x:hover { background: var(--hair); }
  .x svg { width: 14px; height: 14px; fill: none; stroke: currentColor; stroke-width: 1.8; stroke-linecap: round; }

  .words { padding: 6px 28px 0; }
  h1 { margin: 0; font-size: 34px; font-weight: 700; letter-spacing: -.025em; line-height: 1.1; }
  p { margin: 12px 0 0; font-size: 15px; color: var(--ink); }
  .words p span { animation: say var(--loop) var(--ease) infinite; }
  .words .b2 { animation-name: say2; }
  .words .b3 { animation-name: say3; }

  /* the scene: a sunk stage, like the pad's own stage in the app */
  .stage { flex: 1; min-height: 0; margin: 20px 28px 0; padding: 0 32px 30px; border-radius: var(--r-stage); background: var(--sunk); display: flex; align-items: flex-end; justify-content: space-between; gap: 24px; }
  .pad { display: flex; gap: 12px; }
  .col { position: relative; }
  /* the same key as in the Configure panel: the button's colour on a darker edge */
  .key { width: 84px; height: 64px; border-radius: 14px; background: var(--c); box-shadow: 0 8px 0 var(--e); display: grid; place-items: center; }
  .key span { font: 600 15px/1 var(--mono); color: #fff; opacity: .92; }
  .key.held { animation: press var(--loop) var(--ease) infinite; }

  .sci { position: absolute; left: 2px; bottom: 100%; line-height: 0; animation: hop var(--loop) var(--ease) infinite; }
  .sci :global(.eye) { transform-box: fill-box; transform-origin: center; animation: blink 4.3s steps(1) infinite; }
  /* a speech bubble from his shoulder: his name first, his voice while he holds the key */
  .callout { position: absolute; left: 100%; top: 2px; margin-left: 8px; height: 38px; padding: 0 14px; display: grid; align-items: center; border-radius: 19px 19px 19px 4px; background: var(--surface); border: 1px solid var(--line); box-shadow: 0 4px 14px rgb(0 0 0 / .08); white-space: nowrap; line-height: 1; transform-origin: left bottom; animation: pop var(--loop) var(--ease) infinite; }
  .callout > * { grid-area: 1 / 1; }
  .hello { font: 600 14px/1 var(--sans); color: var(--ink); animation: hello var(--loop) linear infinite; }
  .voice { display: flex; align-items: center; justify-content: center; gap: 4px; opacity: 0; animation: speak var(--loop) linear infinite; }
  .voice i { width: 3px; height: 18px; border-radius: 2px; background: var(--signal); animation: bar .85s ease-in-out infinite alternate; }

  .launch { display: flex; flex-direction: column; align-items: flex-start; gap: 10px; }
  .said { height: 20px; width: 13ch; overflow: hidden; white-space: nowrap; font: 500 14px/20px var(--mono); color: var(--signal-ink); animation: said var(--loop) linear infinite; }
  .app { margin-bottom: 4px; transform-origin: 20% 100%; animation: open var(--loop) var(--ease) infinite; }
  .body { height: 78px; padding: 14px; display: flex; flex-direction: column; gap: 9px; }
  .body i { height: 8px; border-radius: 4px; background: var(--hair); }
  .body i:nth-child(2) { width: 70%; } .body i:nth-child(3) { width: 45%; }
  .sci.second { left: 2px; }
  @keyframes said { 0%, 30% { width: 0; opacity: 1; animation-timing-function: steps(13); } 60%, 64% { width: 13ch; opacity: 1; } 68%, 100% { width: 13ch; opacity: 0; } }
  @keyframes open { 0%, 64% { opacity: 0; transform: scale(.7) translateY(10px); } 72%, 93% { opacity: 1; transform: none; } 97%, 100% { opacity: 0; } }
  .note { width: 250px; flex: none; margin-bottom: 4px; border-radius: var(--r-card); background: var(--surface); border: 1px solid var(--line); box-shadow: 0 10px 28px rgb(0 0 0 / .1); overflow: hidden; }
  .bar { height: 30px; display: flex; align-items: center; padding: 0 14px; border-bottom: 1px solid var(--hair); }
  .bar span { font: 500 11px/1 var(--mono); letter-spacing: .08em; text-transform: uppercase; color: var(--muted); }
  .text { height: 78px; padding: 16px 14px; display: flex; flex-direction: column; gap: 8px; font: 500 14px/1.4 var(--mono); animation: clear var(--loop) linear infinite; }
  .ln { position: relative; display: flex; align-items: flex-start; height: 20px; }
  .fixed { position: absolute; left: 0; top: 0; white-space: nowrap; opacity: 0; animation: fix var(--loop) linear infinite; }
  .tag { align-self: flex-start; padding: 2px 8px; border-radius: 999px; background: var(--sunk); color: var(--ok); font: 600 11px/1.4 var(--mono); opacity: 0; animation: fix var(--loop) linear infinite; }
  .typed { width: 17ch; overflow: hidden; white-space: nowrap; text-decoration: underline wavy transparent; animation: type var(--loop) linear infinite, slip var(--loop) linear infinite; }
  .caret { width: 2px; height: 18px; margin-left: 1px; background: var(--signal); animation: caret 1s steps(1) infinite; }

  /* the question screens: he stands on the left and talks; the answers sit under his bubble */
  .talk { flex: 1; min-height: 0; margin: 8px 28px 0; padding: 28px 32px; border-radius: var(--r-stage); background: var(--sunk); display: flex; align-items: center; gap: 28px; }
  .who { flex: none; }
  /* the practice: the same sunk stage, a smaller Mr. Science and bubble on top, the box and checklist filling the rest */
  .talk.practice { padding: 16px 22px; flex-direction: column; align-items: stretch; gap: 12px; }
  .line { flex: none; display: flex; align-items: flex-end; gap: 12px; animation: rise .24s var(--ease); }
  .line .bubble { align-self: auto; font-size: 16px; padding: 10px 16px; }
  .tryc { flex: 1; min-height: 0; padding: 12px 16px; border-radius: var(--r-card); background: var(--surface); border: 1px solid var(--line); }
  .who.leaving { animation: drop .6s var(--ease) forwards; }
  .say { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 14px; animation: rise .24s var(--ease); }
  .bubble { align-self: flex-start; padding: 14px 18px; border-radius: 20px 20px 20px 4px; background: var(--surface); border: 1px solid var(--line); box-shadow: 0 4px 14px rgb(0 0 0 / .08); font-size: 18px; font-weight: 600; line-height: 1.35; letter-spacing: -.01em; }
  .choices { display: flex; flex-direction: column; align-items: flex-start; gap: 8px; }
  .choices.row { flex-direction: row; flex-wrap: wrap; align-items: center; }
  .choice { height: 46px; padding: 0 20px; border-radius: var(--r-bar); border: 1.5px solid var(--line); background: var(--surface); font-size: 15px; font-weight: 600; transition: border-color .24s var(--ease), transform .24s var(--ease); }
  .choice:active, .go:active { transform: scale(.97); }
  .pick { height: 38px; padding: 0 16px; border-radius: var(--r-pill); border: 1.5px solid var(--line); background: var(--surface); font-size: 14px; font-weight: 600; transition: background-color .24s var(--ease), color .24s var(--ease), border-color .24s var(--ease); }
  .pick.on { background: var(--ink); color: var(--inv); border-color: var(--ink); }
  /* the layout question: two cards, each a small screen with the bars where they will sit, and its keys */
  .lay { width: 190px; padding: 12px 12px 14px; border-radius: var(--r-card); border: 1.5px solid var(--line); background: var(--surface); display: grid; justify-items: start; gap: 4px; text-align: left; transition: border-color .24s var(--ease), transform .24s var(--ease); }
  .lay:active { transform: scale(.97); }
  .mini { position: relative; width: 100%; height: 72px; margin-bottom: 6px; border-radius: 8px; background: var(--sunk); border: 1px solid var(--hair); }
  .mini .bars { position: absolute; display: flex; gap: 4px; left: 50%; bottom: 8px; transform: translateX(-50%); }
  .mini .bars i { width: 16px; height: 5px; border-radius: 3px; background: var(--c); }
  .mini.vert .bars { flex-direction: column; left: 8px; top: 50%; bottom: auto; transform: translateY(-50%); }
  .mini.vert .bars i { width: 5px; height: 12px; }
  .lname { font-size: 15px; font-weight: 700; }
  .lwhere { font-size: 12px; color: var(--muted); }
  .caps { display: flex; gap: 5px; margin-top: 4px; }
  kbd { min-width: 22px; height: 22px; padding: 0 6px; border-radius: 6px; border: 1px solid var(--line); border-bottom-width: 2px; display: inline-grid; place-items: center; font: 600 11px/1 var(--mono); color: var(--ink); background: var(--surface); }
  .fine { margin: 0; font-size: 13px; line-height: 1.5; color: var(--muted); }
  .fine.warn { color: var(--bad); }
  .keyrow { display: flex; gap: 8px; }
  input { flex: 1; min-width: 0; height: 40px; padding: 0 14px; border-radius: var(--r-row); border: 1.5px solid var(--line); background: var(--surface); color: var(--ink); font: 500 13px/1 var(--mono); outline: none; }
  input:focus-visible { border-color: var(--ink); }
  .quiet { height: 40px; padding: 0 16px; border-radius: var(--r-row); border: 1.5px solid var(--line); background: transparent; font-size: 13px; font-weight: 600; }
  .note2 { min-height: 18px; font-size: 12px; color: var(--bad); }
  .note2.ok { color: var(--ok); }

  footer { height: 76px; flex: none; display: flex; align-items: center; justify-content: space-between; gap: 16px; padding: 0 28px; }
  .hint { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: 12px; color: var(--muted); }
  .end { flex: none; display: flex; align-items: center; gap: 14px; }
  .skip { border: 0; background: transparent; padding: 8px 4px; font-size: 13px; color: var(--muted); text-decoration: underline; text-underline-offset: 3px; }
  .go { height: 44px; padding: 0 26px; border: 0; border-radius: var(--r-bar); background: var(--signal); color: var(--on-signal); font-size: 15px; font-weight: 600; transition: transform .24s var(--ease), box-shadow .24s var(--ease); }
  button:disabled { opacity: .4; cursor: default; }
  @media (hover: hover) {
    .go:hover:not(:disabled) { box-shadow: 0 0 0 4px var(--halo); }
    .choice:hover, .lay:hover, .pick:hover:not(.on), .quiet:hover:not(:disabled) { border-color: var(--ink); }
    .skip:hover { color: var(--ink); }
  }

  /* One loop, seven seconds: hop on (0-8%), hold and talk (10-62%), words typed (30-62%), rest, clear. */
  @keyframes press { 0%, 8% { transform: none; box-shadow: 0 8px 0 var(--e); } 10%, 62% { transform: translateY(6px); box-shadow: 0 2px 0 var(--e); } 65%, 100% { transform: none; box-shadow: 0 8px 0 var(--e); } }
  @keyframes hop { 0% { transform: none; } 4% { transform: translateY(-16px); } 8% { transform: none; } 10%, 62% { transform: translateY(6px); } 65%, 100% { transform: none; } }
  @keyframes pop { 0%, 9% { transform: none; } 11% { transform: scale(.94); } 14%, 61% { transform: none; } 63% { transform: scale(.94); } 66%, 100% { transform: none; } }
  @keyframes hello { 0%, 10% { opacity: 1; } 12%, 62% { opacity: 0; } 65%, 100% { opacity: 1; } }
  @keyframes speak { 0%, 11% { opacity: 0; } 13%, 61% { opacity: 1; } 63%, 100% { opacity: 0; } }
  @keyframes bar { from { transform: scaleY(.25); } to { transform: scaleY(1.15); } }
  @keyframes type { 0%, 30% { width: 0; animation-timing-function: steps(17); } 62%, 100% { width: 17ch; } }
  @keyframes slip { 0%, 62% { opacity: 1; color: var(--ink); } 66%, 70% { opacity: 1; color: var(--bad); text-decoration-color: var(--bad); } 74%, 100% { opacity: 0; } }
  @keyframes fix { 0%, 72% { opacity: 0; } 78%, 100% { opacity: 1; } }
  @keyframes clear { 0%, 93% { opacity: 1; } 97%, 100% { opacity: 0; } }
  @keyframes caret { 50% { opacity: 0; } }
  @keyframes blink { 0%, 100% { transform: none; } 94% { transform: scaleY(.15); } }
  @keyframes say { 0%, 6% { color: var(--faint); } 10%, 93% { color: var(--ink); } 97%, 100% { color: var(--faint); } }
  @keyframes say2 { 0%, 12% { color: var(--faint); } 16%, 93% { color: var(--ink); } 97%, 100% { color: var(--faint); } }
  @keyframes say3 { 0%, 30% { color: var(--faint); } 36%, 93% { color: var(--ink); } 97%, 100% { color: var(--faint); } }
  @keyframes rise { from { opacity: 0; transform: translateY(6px); } }
  @keyframes drop { 30% { transform: translateY(-22px); } 100% { transform: translateY(420px); } }

  /* Reduced motion: no loop. The still picture is the finished scene: his name in the callout, the corrected words. */
  @media (prefers-reduced-motion: reduce) {
    .words p span, .key.held, .sci, .sci :global(.eye), .callout, .hello, .voice, .voice i, .text, .typed, .fixed, .tag, .caret, .say, .line, .who.leaving { animation: none; }
    .typed { display: none; }
    .fixed, .tag { opacity: 1; }
    .said { opacity: 0; animation: none; } .app { opacity: 1; animation: none; }
  }
</style>
