// What a profile "applies to", compared the way a person would: `Figma`, `figma.exe` and `FIGMA` are one
// app, and `https://www.netflix.com/browse` and `netflix.com` are one site.

/** The separate things a match names, in a plain form (the "everything else" star is none of them). */
export function tokens(/** @type {string} */ text) {
  return text
    .split(',')
    .map((t) => t.trim().toLowerCase().replace(/^https?:\/\//, '').replace(/^www\./, '').replace(/\/.*$/, '').replace(/\.exe$/, ''))
    .filter((t) => t && t !== '*');
}

/** Whether two matches share an app or a site. */
export function overlaps(/** @type {string} */ a, /** @type {string} */ b) {
  const other = tokens(b);
  return tokens(a).some((t) => other.includes(t));
}
