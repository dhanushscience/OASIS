<script>
  import { onMount } from 'svelte';
  import { STEPS, stepFor, tick } from '../tryCommands.js';

  // A practice box: say the phrase, the page sees the effect on the textarea and ticks the step.
  /** `compact`: the guide's frame. The box beside the checklist, no intro or extra list (Mr. Science says the intro). */
  let { compact = false } = $props();
  let done = $state(/** @type {Record<string, boolean>} */ ({}));
  let toast = $state(false);
  let text = $state('');
  /** @type {HTMLTextAreaElement | undefined} */
  let box = $state();
  let toastTimer = 0;
  const count = $derived(STEPS.filter((s) => !s.optional && done[s.id]).length);
  const required = STEPS.filter((s) => !s.optional).length;

  onMount(() => box?.focus());

  function see(/** @type {Event} */ ev) {
    const t = /** @type {HTMLTextAreaElement} */ (ev.currentTarget);
    const k = /** @type {KeyboardEvent} */ (ev);
    const step = stepFor({ type: ev.type, inputType: /** @type {InputEvent} */ (ev).inputType, key: k.key, ctrlKey: k.ctrlKey, metaKey: k.metaKey,
      text: t.value, start: t.selectionStart, end: t.selectionEnd, length: t.value.length }, done);
    // "press escape" and "find" are practice here: they must not close the window around it or jump to its search.
    if (ev.type === 'keydown' && (k.key === 'Escape' || ((k.ctrlKey || k.metaKey) && k.key.toLowerCase() === 'f'))) ev.stopPropagation();
    if (ev.type === 'keydown' && step === 'save') {
      ev.preventDefault(); // keep the webview from opening its own save
      toast = true; clearTimeout(toastTimer); toastTimer = window.setTimeout(() => (toast = false), 2000);
    }
    done = tick(done, step);
  }
  function reset() { text = ''; done = {}; toast = false; box?.focus(); }

  const MORE = [
    ['Edit', 'select all, copy, paste, cut, undo, redo, save, find, bold, italic, underline'],
    ['Keys', 'press enter, tab, escape, backspace'],
    ['Move', 'go to start / end of line, start / end of document, page up, page down, scroll up, scroll down'],
    ['Select', 'select line, select to end of line, select to start of line'],
    ['Browser', 'refresh, go back, go forward, next tab, previous tab, zoom in, zoom out'],
  ];
</script>

<div class="try" class:compact>
  {#if !compact}<p class="intro">Hold your talk key and speak. The box below is where it types.</p>{/if}
  <div class="box">
  <label class="lab" for="try-box">Practice box</label>
  <textarea id="try-box" bind:this={box} bind:value={text} rows="4" placeholder="Speak here…"
    oninput={see} onkeydown={see} onkeyup={see} onselect={see} oncopy={see} onpaste={see}></textarea>
  </div>

  <div class="list">
  <div class="head"><span class="lab">Try these in order · {count} of {required}</span><button type="button" class="quiet" onclick={reset}>Reset</button></div>
  <ol aria-live="polite">
    {#each STEPS as s, i (s.id)}
      <li class:on={done[s.id]}>
        <span class="mark" aria-hidden="true">{done[s.id] ? '✓' : i + 1}</span>
        <span class="t">
          {#if i === 0}{s.say}{:else}Say: <b>“{s.say}”</b>{/if}{#if s.optional}<span class="opt">{' (optional)'}</span>{/if}
          {#if done[s.id]}<span class="ok"> · {s.done}</span>{:else if s.hint}<span class="hint">{s.hint}</span>{/if}
        </span>
      </li>
    {/each}
  </ol>
  </div>

  {#if !compact}
  <details>
    <summary>More things you can say</summary>
    <dl>{#each MORE as [g, w]}<dt class="lab">{g}</dt><dd>{w}</dd>{/each}</dl>
    <p class="hint">These work only as the whole sentence: “copy” copies, “copy the file” is typed.</p>
  </details>
  {/if}

  <div class="toast" class:show={toast} role="status" aria-live="polite">{#if toast}<span aria-hidden="true">✓</span> Saved{/if}</div>
</div>

<style>
  .try { position: relative; display: flex; flex-direction: column; gap: 10px; min-width: 0; }
  .intro { margin: 0; font-size: 13px; line-height: 1.5; color: var(--muted); }
  .lab { font: 500 11px/1.3 var(--mono); letter-spacing: .1em; text-transform: uppercase; color: var(--muted); }
  textarea { width: 100%; box-sizing: border-box; resize: vertical; padding: 10px 12px; border-radius: var(--r-field); border: 1.5px solid var(--line); background: var(--surface); color: var(--ink); font: 14px/1.5 var(--sans); outline: none; }
  textarea:focus-visible { border-color: var(--ink); }
  .head { display: flex; align-items: center; justify-content: space-between; gap: 10px; margin-top: 4px; }
  .quiet { height: 32px; padding: 0 12px; border-radius: var(--r-pill); border: 1px solid var(--line); background: transparent; color: var(--ink); font-size: 12px; font-weight: 600; }
  ol { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; }
  li { display: flex; gap: 10px; align-items: flex-start; padding: 8px 0; border-top: 1px solid var(--hair); font-size: 13px; line-height: 1.45; color: var(--ink); }
  .mark { flex: none; width: 22px; height: 22px; border-radius: 50%; display: grid; place-items: center; font: 500 11px var(--mono); background: var(--hair); color: var(--muted); }
  li.on .mark { background: var(--ok); color: #fff; }
  .t { min-width: 0; overflow-wrap: anywhere; }
  .opt, .hint { color: var(--muted); font-size: 12px; }
  .t .hint { display: block; }
  p.hint { margin: 8px 0 0; line-height: 1.45; }
  .ok { color: var(--ok); }
  summary { cursor: pointer; font-size: 13px; font-weight: 600; padding: 6px 0; }
  summary:focus-visible { outline: 2px solid var(--ink); outline-offset: 2px; }
  dl { margin: 6px 0 0; display: grid; gap: 2px 14px; grid-template-columns: auto minmax(0, 1fr); }
  dt { padding-top: 2px; }
  dd { margin: 0; font-size: 13px; line-height: 1.45; overflow-wrap: anywhere; }
  .toast { position: absolute; right: 0; top: 0; padding: 8px 14px; border-radius: var(--r-pill); background: var(--ink); color: var(--inv); font-size: 13px; font-weight: 600; opacity: 0; transform: translateY(-6px); pointer-events: none; transition: opacity .18s var(--ease), transform .18s var(--ease); }
  .toast.show { opacity: 1; transform: none; }
  .box, .list { display: flex; flex-direction: column; gap: 10px; min-width: 0; }
  /* the guide's frame: the box on the left filling the height, the checklist on the right, one tight row per step */
  .try.compact { height: 100%; display: grid; grid-template-columns: minmax(0, 4fr) minmax(0, 6fr); gap: 16px; }
  .compact .box, .compact .list { gap: 6px; min-height: 0; }
  .compact textarea { flex: 1; min-height: 0; resize: none; }
  .compact .head { margin-top: 0; }
  .compact .quiet { height: 26px; padding: 0 10px; }
  .compact li { gap: 8px; padding: 3px 0; font-size: 12.5px; line-height: 1.35; }
  .compact .mark { width: 20px; height: 20px; font-size: 10px; }
  .compact .opt, .compact .hint { font-size: 11.5px; }
  .compact .toast { top: auto; bottom: 8px; right: auto; left: 8px; }
  @media (prefers-reduced-motion: reduce) { .toast { transition: none; transform: none; } }
</style>
