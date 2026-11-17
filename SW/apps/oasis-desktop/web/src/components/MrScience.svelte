<script>
  // Mr. Science, drawn from the text grid in guide/mrscience.js. One drawing; a mood moves it.
  import { svg } from '../guide/mrscience.js';

  /** @type {{ mood?: 'idle' | 'point' | 'thinking' | 'success' | 'retry', scale?: number }} */
  let { mood = 'idle', scale = 4 } = $props();
  const theme = document.documentElement.dataset.theme === 'dark' ? 'dark' : 'light';
</script>

<span class="sci {mood}">{@html svg(theme, scale)}</span>

<style>
  .sci { display: inline-block; line-height: 0; transform-origin: 50% 100%; }
  .sci :global(.eye) { transform-box: fill-box; transform-origin: center; animation: blink 4.3s steps(1) infinite; }
  .idle { animation: bob 3.2s ease-in-out infinite; }
  .point { animation: lean 1.6s ease-in-out infinite alternate; }
  .thinking { animation: tilt 2.4s ease-in-out infinite alternate; }
  .success { animation: jump .72s var(--ease) 2; }
  .retry { animation: shake .48s ease-in-out 1; }
  @keyframes blink { 0%, 100% { transform: none; } 94% { transform: scaleY(.15); } }
  @keyframes bob { 50% { transform: translateY(-2px); } }
  @keyframes lean { to { transform: rotate(5deg) translateX(3px); } }
  @keyframes tilt { to { transform: rotate(-4deg); } }
  @keyframes jump { 40% { transform: translateY(-14px); } }
  @keyframes shake { 25% { transform: translateX(-4px); } 75% { transform: translateX(4px); } }
  @media (prefers-reduced-motion: reduce) { .sci, .sci :global(.eye) { animation: none; } }
</style>
