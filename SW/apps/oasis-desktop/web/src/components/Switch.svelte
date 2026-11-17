<script>
  /** An on/off switch: the knob slides to the right when on, and the word moves to the free side. */
  /** @type {{ checked: boolean, label: string, disabled?: boolean, onchange: (checked: boolean) => void }} */
  let { checked, label, disabled = false, onchange } = $props();
</script>

<button type="button" class="switch" class:on={checked} role="switch" aria-checked={checked} aria-label={label} {disabled} onclick={() => onchange(!checked)}>
  <span class="knob"></span><span class="word">{checked ? 'On' : 'Off'}</span>
</button>

<style>
  .switch { position: relative; flex: none; width: 78px; height: 32px; padding: 0 11px; display: flex; align-items: center; justify-content: flex-end; border-radius: var(--r-pill); border: 1.5px solid var(--line); background: var(--hair); font-size: 12px; font-weight: 600; color: var(--muted); transition: background-color .2s var(--ease), border-color .2s var(--ease), color .2s var(--ease); }
  .knob { position: absolute; top: 3px; left: 3px; width: 24px; height: 24px; border-radius: 50%; background: var(--surface); box-shadow: var(--shadow-chosen); transition: transform .22s var(--ease), background-color .2s var(--ease); }
  .switch.on { justify-content: flex-start; background: var(--ink); border-color: var(--ink); color: var(--inv); }
  .switch.on .knob { transform: translateX(46px); background: var(--signal); }
  .switch:disabled { opacity: .4; cursor: default; }
</style>
