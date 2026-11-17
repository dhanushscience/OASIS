// When the update pop-up shows. Pure, so it is tested without a window (updatePrompt.test.js).

export const DAY_MS = 24 * 60 * 60 * 1000;

/** True when version `a` is newer than `b`, comparing numbers part by part ("0.1.10" > "0.1.9"). */
export function isNewer(/** @type {string} */ a, /** @type {string} */ b) {
  const pa = String(a).split(/[.+-]/).map((n) => parseInt(n, 10) || 0);
  const pb = String(b).split(/[.+-]/).map((n) => parseInt(n, 10) || 0);
  for (let i = 0; i < Math.max(pa.length, pb.length); i++) {
    if ((pa[i] ?? 0) !== (pb[i] ?? 0)) return (pa[i] ?? 0) > (pb[i] ?? 0);
  }
  return false;
}

/**
 * Show the pop-up? Only after Getting Started is over, for a newer version, while nothing is installing,
 * and at most once a day per version: `seen` is the last { version, at } it was shown for (Later included).
 * @param {{ current: string, available: string | null, progress: number | null }} info
 * @param {{ loaded: boolean, status: string }} guide
 * @param {{ version: string, at: number } | null} seen
 * @param {number} now
 */
export function shouldPrompt(info, guide, seen, now) {
  if (!guide.loaded || (guide.status !== 'dismissed' && guide.status !== 'complete')) return false;
  if (!info.available || !info.current || !isNewer(info.available, info.current) || info.progress !== null) return false;
  return !(seen && seen.version === info.available && now - seen.at < DAY_MS);
}
