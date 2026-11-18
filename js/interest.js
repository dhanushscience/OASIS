/**
 * End CTA: drive GitHub stars as the interest signal.
 * Star count is read from the public GitHub API when available.
 */

const REPO = 'dhanushscience/OASIS'; // shows stars for the repo linked in the footer
const REPO_URL = `https://github.com/${REPO}`;

function setNote(text, kind = '') {
  const note = document.getElementById('interest-note');
  if (!note) return;
  note.textContent = text;
  note.dataset.kind = kind;
}

function updateStarCount(n) {
  const el = document.getElementById('interest-count');
  if (!el) return;
  if (!Number.isFinite(n) || n < 1) {
    el.hidden = true;
    return;
  }
  el.hidden = false;
  el.textContent = `${n} GitHub star${n === 1 ? '' : 's'} so far`;
}

async function fetchStarCount() {
  try {
    const res = await fetch(`https://api.github.com/repos/${REPO}`, {
      headers: { Accept: 'application/vnd.github+json' },
    });
    if (!res.ok) return null;
    const data = await res.json();
    const n = Number(data.stargazers_count);
    return Number.isFinite(n) ? n : null;
  } catch {
    return null;
  }
}

function initStarCta() {
  fetchStarCount().then((n) => {
    if (n != null) updateStarCount(n);
  });
}

initStarCta();
