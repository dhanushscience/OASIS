/* PROJECT OASIS — minimal chrome for workbench story */

(function () {
  'use strict';

  const nav = document.getElementById('nav');
  if (nav) {
    window.addEventListener('scroll', () => {
      nav.classList.toggle('scrolled', window.scrollY > 24);
    }, { passive: true });
  }
})();

// Show the newest released version next to the download button; the static text stays if the API is unreachable.
fetch('https://api.github.com/repos/dhanushscience/OASIS-releases/releases/latest')
  .then((r) => (r.ok ? r.json() : null))
  .then((d) => {
    const m = document.getElementById('hero-meta');
    if (d && d.tag_name && m) m.textContent = `${d.tag_name} · Free beta · 11 MB installer · Windows 10 or 11, 64-bit`;
  })
  .catch(() => {});
