<script>
  import { tick } from 'svelte';
  import { confirmUi, answer } from '../confirm.svelte.js';

  /** @type {HTMLButtonElement} */
  let keepBtn;
  $effect(() => { if (confirmUi.open) tick().then(() => keepBtn?.focus()); }); // the safe choice is focused, so Enter never deletes

  /** Esc answers no; captured so a Settings pop-up behind it stays open. */
  function esc(/** @type {Event} */ e) {
    if (!confirmUi.open || (e instanceof KeyboardEvent && e.key !== 'Escape')) return;
    e.preventDefault(); e.stopPropagation(); answer(false);
  }
</script>

<svelte:window onkeydowncapture={esc} />

<div class="scrim" class:open={confirmUi.open} inert={!confirmUi.open} role="presentation" onclick={(e) => e.target === e.currentTarget && answer(false)}>
  <div class="box" role="alertdialog" aria-modal="true" aria-labelledby="confirm-title" aria-describedby="confirm-body">
    <h2 id="confirm-title">{confirmUi.title}</h2>
    <p id="confirm-body">{confirmUi.body}</p>
    <div class="btns">
      <button bind:this={keepBtn} class="keep" onclick={() => answer(false)}>{confirmUi.keep}</button>
      <button class="yes" onclick={() => answer(true)}>{confirmUi.yes}</button>
    </div>
  </div>
</div>

<style>
  .scrim { position: fixed; inset: 0; z-index: 200; display: flex; align-items: center; justify-content: center; padding: 24px; background: rgb(20 20 24 / .45);
    opacity: 0; visibility: hidden; transition: opacity .18s var(--ease), visibility 0s linear .18s; }
  .scrim.open { opacity: 1; visibility: visible; transition-delay: 0s; }
  .box { width: min(400px, 100%); padding: 24px; background: var(--surface); border: 1px solid var(--line); border-radius: var(--r-panel); box-shadow: 0 24px 60px var(--shadow);
    transform: translateY(8px) scale(.98); transition: transform .22s var(--ease); }
  .scrim.open .box { transform: none; }
  h2 { margin: 0 0 8px; font-size: 18px; font-weight: 700; letter-spacing: -.02em; }
  p { margin: 0 0 20px; font-size: 14px; line-height: 1.5; color: var(--muted); }
  p:empty { display: none; }
  .btns { display: flex; justify-content: flex-end; gap: 10px; }
  button { height: 38px; padding: 0 18px; border-radius: var(--r-pill); font-size: 13px; font-weight: 600; }
  .keep { border: 1px solid var(--line); background: transparent; color: var(--ink); }
  .yes { border: 0; background: var(--bad); color: var(--surface); }
  button:focus-visible { outline: 2px solid var(--signal); outline-offset: 2px; }
</style>
