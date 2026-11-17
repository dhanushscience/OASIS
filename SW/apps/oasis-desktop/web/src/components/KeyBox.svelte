<script>
  import { chordFromKeyEvent } from '../actionKinds.js';

  /** A shortcut that is recorded, not typed: click the box, then press the keys. Tap Shift, Ctrl,
   *  Alt or Win by itself to use that key alone. */
  /** @type {{ value: string, label: string, disabled?: boolean, onchange: (value: string) => void, autofocus?: boolean, onleave?: () => void, bare?: boolean }} */
  let { value, label, disabled = false, onchange, autofocus = false, onleave = () => {}, bare = false } = $props();

  let focused = $state(false);
  const MODIFIERS = /** @type {Record<string, string>} */ ({ Control: 'Ctrl', Shift: 'Shift', Alt: 'Alt', Meta: 'Win' });
  /** A modifier that went down with nothing else held and has not been joined by another key yet. */
  let alone = '';

  /** @param {KeyboardEvent & { currentTarget: HTMLInputElement }} e */
  function keys(e) {
    if (disabled) return;
    const mods = e.ctrlKey || e.altKey || e.metaKey || e.shiftKey;
    e.preventDefault();
    const modifier = MODIFIERS[e.key];
    if (modifier) {
      // alone only if it is the one modifier held; it records when it is let go with nothing between
      alone = [e.ctrlKey, e.altKey, e.metaKey, e.shiftKey].filter(Boolean).length === 1 ? modifier : '';
      return;
    }
    alone = '';
    if (e.key === 'Escape' && !mods) { e.stopPropagation(); e.currentTarget.blur(); return; }
    if ((e.key === 'Backspace' || e.key === 'Delete') && !mods) return onchange('');
    const chord = chordFromKeyEvent(e, true);
    if (chord) onchange(chord);
  }

  /** @param {KeyboardEvent} e */
  function released(e) {
    if (disabled) return;
    const modifier = MODIFIERS[e.key];
    if (modifier && modifier === alone) onchange(modifier);
    alone = '';
  }
</script>

<label class="box" class:on={focused} class:bare>
  {#if !bare}<span class="lab">{label}</span>{/if}
  <!-- svelte-ignore a11y_autofocus -->
  <input class="mono" {value} readonly {disabled} {autofocus} autocomplete="off" spellcheck="false" aria-label="{label}: click, then press the keys"
    placeholder={focused ? 'Press the keys…' : 'Click to set keys'}
    onkeydown={keys} onkeyup={released} onfocus={() => (focused = true)} onblur={() => { focused = false; alone = ''; onleave(); }} />
</label>

<style>
  .box { display: flex; flex-direction: column; gap: 6px; min-width: 0; }
  .lab { font: 600 13px/1.3 var(--sans); letter-spacing: .08em; text-transform: uppercase; color: var(--muted); }
  input { height: 52px; border-radius: var(--r-row); border: 1.5px solid var(--ink); background: var(--surface); color: var(--ink); padding: 0 14px; font-size: 18px; font-weight: 600; outline: none; cursor: pointer; caret-color: transparent; min-width: 0; }
  input::placeholder { font-size: 15px; font-weight: 400; color: var(--muted); }
  /* bare: only the key capture, invisible, laid over the parent (which draws the keys) */
  .bare { position: absolute; inset: 0; }
  .bare input { position: absolute; inset: 0; width: 100%; height: 100%; opacity: 0; border: 0; }
  .box.on input { border-color: var(--signal); }
  input:disabled { opacity: .5; cursor: default; }
</style>
