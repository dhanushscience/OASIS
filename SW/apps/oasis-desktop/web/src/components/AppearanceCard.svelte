<script>
  import { themeChoice, chooseTheme } from '../theme.js';
  import { view, setLayout } from '../pad.svelte.js';

  let choice = $state(themeChoice());
  const THEMES = [['system', 'System'], ['light', 'Light'], ['dark', 'Dark']];
  const LAYOUTS = [['auto', 'Automatic'], ['minimal', 'Minimal'], ['full', 'Full']];
  /** @param {'system' | 'light' | 'dark'} c */
  const pick = (c) => { choice = c; chooseTheme(c); };
</script>

<section class="group" aria-label="Appearance">
  <div class="ghead"><span class="lab">Theme and keyboard</span></div>

  <div class="srow">
    <div class="rtext"><div class="rname">Colour theme</div><div class="hint">System follows your Windows light or dark setting, and changes with it.</div></div>
    <div class="seg" role="radiogroup" aria-label="Colour theme">
      {#each THEMES as [id, name]}
        <button type="button" role="radio" aria-checked={choice === id} class:on={choice === id} onclick={() => pick(/** @type {any} */ (id))}>{name}</button>
      {/each}
    </div>
  </div>

  <div class="srow">
    <div class="rtext"><div class="rname">Keyboard view</div><div class="hint">How the keyboard is drawn on Profiles. Minimal shows only the keys you use, which reads best for shortcuts. Automatic goes minimal in a small window.</div></div>
    <div class="seg" role="radiogroup" aria-label="Keyboard view">
      {#each LAYOUTS as [id, name]}
        <button type="button" role="radio" aria-checked={view.layout === id} class:on={view.layout === id} onclick={() => setLayout(/** @type {any} */ (id))}>{name}</button>
      {/each}
    </div>
  </div>
</section>
