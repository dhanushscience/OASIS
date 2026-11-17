<script>
  /** The logo of the system OASIS is running on: the default profile belongs to the whole computer. */
  let { size = 28 } = $props();

  const hint = `${/** @type {any} */ (navigator).userAgentData?.platform ?? ''} ${navigator.platform ?? ''} ${navigator.userAgent ?? ''}`;
  const os = /win/i.test(hint) ? 'windows' : /mac|iphone|ipad/i.test(hint) ? 'mac' : /linux|x11|cros/i.test(hint) ? 'linux' : 'windows';
</script>

<svg viewBox="0 0 24 24" width={size} height={size} aria-hidden="true" class="os">
  {#if os === 'windows'}
    <path d="M3 5.4 10.4 4.4v6.8H3zM11.4 4.3 21 3v8.2h-9.6zM3 12.2h7.4v6.8L3 18zM11.4 12.2H21V21l-9.6-1.3z" fill="#0078D4" />
  {:else if os === 'mac'}
    <circle cx="9.2" cy="13.6" r="5" fill="currentColor" />
    <circle cx="14.8" cy="13.6" r="5" fill="currentColor" />
    <rect x="9" y="9.5" width="6" height="6" fill="currentColor" />
    <path d="M12 8.8c0-2.100 1-3.600 2.900-4.100.1 2.100-1 3.700-2.900 4.100z" fill="currentColor" />
  {:else}
    <ellipse cx="12" cy="13.6" rx="5.4" ry="7.2" fill="currentColor" />
    <ellipse cx="12" cy="15.4" rx="3.3" ry="4.8" fill="var(--surface)" />
    <circle cx="10.200" cy="8.200" r="1.200" fill="var(--surface)" /><circle cx="13.800" cy="8.200" r="1.200" fill="var(--surface)" />
    <circle cx="10.400" cy="8.300" r=".5" fill="currentColor" /><circle cx="13.600" cy="8.300" r=".5" fill="currentColor" />
    <path d="M10.600 10.200c.8-.7 2-.7 2.800 0-.3 1-.9 1.500-1.400 1.500s-1.100-.5-1.400-1.500z" fill="#F5A524" />
    <ellipse cx="9" cy="20.600" rx="2.400" ry="1.100" fill="#F5A524" /><ellipse cx="15" cy="20.600" rx="2.400" ry="1.100" fill="#F5A524" />
  {/if}
</svg>

<style>
  .os { display: block; flex: none; }
</style>
