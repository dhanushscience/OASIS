/**
 * Workbench story chapters. Progress windows must match assembly.js.
 */

/** Five-stage build key shown in the chrome (replaces dots / 149 hours). */
export const STAGES = [
  { id: 'idea', label: 'THE IDEA', short: 'IDEA', start: 0.0 },
  { id: 'hardware', label: 'THE HARDWARE', short: 'HW', start: 0.10 },
  { id: 'assembly', label: 'THE ASSEMBLY', short: 'ASM', start: 0.20 },
  { id: 'firmware', label: 'THE FIRMWARE', short: 'FW', start: 0.48 },
  { id: 'software', label: 'THE SOFTWARE', short: 'SW', start: 0.56 },
];

export const CHAPTERS = [
  {
    id: 'idea',
    start: 0.0,
    end: 0.10,
    phase: 'build',
    stage: 'idea',
    label: '01 // THE IDEA',
    title: 'An interface built to know what you want',
    body: 'OASIS is a three-button pad and a free Windows app. It watches which app is in front and gives each button a meaning for that app. Hold a button and talk: your words are typed, or run as a command.',
    status: 'Free public beta for Windows',
    details: [
      { step: '01', text: 'A profile for each app: Chrome, VS Code, WhatsApp, and more' },
      { step: '02', text: 'Hold a button and talk: type with dictation, or open apps' },
      { step: '03', text: 'Quick Launch and an overlay that shows each button' },
      { step: '04', text: 'No pad? Your keyboard works as the three buttons' },
    ],
  },
  {
    id: 'board',
    start: 0.10,
    end: 0.20,
    phase: 'build',
    stage: 'hardware',
    label: '02 // THE HARDWARE',
    title: 'Wireless. All-day battery. USB-C.',
    body: 'ESP32-C3 over BLE, USB-C for charge and flash, and three MEC switches with real travel. Built to sit on your desk for a full workday without tethered HID drama.',
    status: 'BLE · battery · USB-C',
  },
  {
    id: 'keys',
    start: 0.20,
    end: 0.34,
    phase: 'build',
    stage: 'assembly',
    label: '03 // THE ASSEMBLY',
    title: 'DIY-friendly by design.',
    body: 'Three switches press onto marked footprints. No soldering gymnastics: align, seat, and you are on the board. Built so developers can assemble and modify without a factory line.',
    status: 'Snap-fit switches',
  },
  {
    id: 'shell',
    start: 0.34,
    end: 0.48,
    phase: 'build',
    stage: 'assembly',
    label: '03 // THE ASSEMBLY',
    title: 'Case closes in minutes.',
    body: 'Shell and top drop over the board with key cutouts and a clean Type-C path. Print, screw, done. Easy to open again when you want to tweak hardware.',
    status: 'Tool-light build',
  },
  {
    id: 'brain',
    start: 0.48,
    end: 0.56,
    phase: 'build',
    stage: 'firmware',
    label: '04 // THE FIRMWARE',
    title: 'BLE that developers can own.',
    body: 'ESP-IDF firmware: pair as OASIS Macro Pad, stream battery %, and push overlay updates over BLE. Flash, debug, and extend the stack yourself.',
    status: 'ESP-IDF · BLE HID',
  },
  {
    id: 'ready',
    start: 0.56,
    end: 0.62,
    phase: 'build',
    stage: 'software',
    label: '05 // THE SOFTWARE',
    title: 'One Windows app. Full control.',
    body: 'Profiles for each app, an overlay that shows what each button does, Bluetooth pairing, and voice that runs on your PC. No pad yet? The three buttons work from your keyboard, so you can start today. Set each button to keys, an app or site, or a few steps in a row.',
    status: 'Windows app · public beta',
  },
  {
    id: 'btn1',
    start: 0.62,
    end: 0.74,
    phase: 'demo',
    stage: 'software',
    buttonIndex: 0,
    label: '05 // THE SOFTWARE',
    title: 'Profiles that follow the app you are in.',
    body: 'Switch from VS Code to Chrome and the pad remaps with you. Each profile holds its own three buttons, and the overlay always shows what they do.',
    features: [
      'A profile for each app, switched for you',
      'Keys, App / site and Steps action types',
      'Overlay labels stay in sync with each button',
      'Use the pad or your keyboard, whichever you have',
    ],
    overlayProfile: 'CHROME',
    overlayLabels: ['YOUTUBE', 'MAIL', 'NEW TAB'],
    mockSection: 'profiles',
    mockTag: 'PROFILES',
  },
  {
    id: 'btn2',
    start: 0.74,
    end: 0.86,
    phase: 'demo',
    stage: 'software',
    buttonIndex: 1,
    label: '05 // THE SOFTWARE',
    title: 'Hold a button and talk.',
    body: 'In a text box, your words are typed, and slips like a misheard name are fixed as it types. Anywhere else, what you say runs as a command: open an app, a new tab, a new window.',
    features: [
      'Voice typing with automatic clean-up',
      'Voice commands: open Spotify, new tab, new window',
      'Speech is recognised on your PC',
      'Optional online speech with your own key',
    ],
    overlayProfile: 'VOICE',
    overlayLabels: ['HOLD TO TALK', 'OPEN APP', 'NEW TAB'],
    mockSection: 'voice',
    mockTag: 'VOICE',
  },
  {
    id: 'btn3',
    start: 0.86,
    end: 1.0,
    phase: 'demo',
    stage: 'software',
    buttonIndex: 2,
    label: '05 // THE SOFTWARE',
    title: 'Your whole setup, in one press.',
    body: 'Quick Launch opens a set of apps and puts each one where you want it across your screens. Start work, or start a call, with one button.',
    features: [
      'Open a set of apps in one go',
      'Place each app on the screen you choose',
      'Make a set for each kind of day',
      'Calendar, Spotify and the browser extension are coming soon',
    ],
    overlayProfile: 'QUICK LAUNCH',
    overlayLabels: ['WORK SET', 'CODING SET', 'CALL SET'],
    mockSection: 'quicklaunch',
    mockTag: 'QUICK LAUNCH',
    showCta: true,
  },
];

export function chapterAt(progress) {
  const p = Math.max(0, Math.min(1, progress));
  let active = CHAPTERS[0];
  for (const ch of CHAPTERS) {
    if (p >= ch.start) active = ch;
  }
  return active;
}

export function chapterLocal(progress, chapter) {
  const span = Math.max(0.0001, chapter.end - chapter.start);
  return Math.max(0, Math.min(1, (progress - chapter.start) / span));
}

let lastOverlayChapter = null;
let lastOverlayBucket = -1;
let lastMockSection = null;

function syncDeskOverlay(ch, isDemo, showEnd) {
  const deskOverlay = document.getElementById('desk-overlay');
  const profileEl = document.getElementById('desk-overlay-profile');
  if (!deskOverlay) return;

  const show = isDemo && !showEnd && Array.isArray(ch.overlayLabels);
  deskOverlay.hidden = !show;
  deskOverlay.setAttribute('aria-hidden', show ? 'false' : 'true');
  deskOverlay.classList.toggle('is-visible', show);
  if (!show) return;

  if (profileEl) profileEl.textContent = ch.overlayProfile || 'PROFILE';

  const labels = ch.overlayLabels || [];
  deskOverlay.querySelectorAll('[data-lbl]').forEach((el) => {
    const i = Number(el.getAttribute('data-lbl'));
    el.textContent = labels[i] || `BTN ${i + 1}`;
  });

  const active = typeof ch.buttonIndex === 'number' ? ch.buttonIndex : -1;
  deskOverlay.querySelectorAll('.desk-overlay-row').forEach((row) => {
    const i = Number(row.getAttribute('data-btn'));
    row.classList.toggle('is-pressed', i === active);
  });
}

function statusForChapter(ch) {
  if (typeof ch.buttonIndex === 'number') {
    const action = ch.overlayLabels?.[ch.buttonIndex];
    if (action) return `Key ${ch.buttonIndex + 1} · ${action}`;
    return `Key ${ch.buttonIndex + 1} pressed`;
  }
  return ch.status || '';
}

function syncPressHint(pressHint, ch, showEnd) {
  if (!pressHint) return;
  const text = showEnd ? '' : statusForChapter(ch);
  if (!text) {
    pressHint.hidden = true;
    return;
  }
  pressHint.hidden = false;
  pressHint.textContent = text;
}

function applyFeatureSection(ch) {
  const featureList = document.getElementById('feature-list');
  const featureTag = document.getElementById('feature-section-tag');
  const featureMain = document.getElementById('feature-main');
  const navItems = document.querySelectorAll('#feature-nav .feature-nav-item');
  const section = ch.mockSection || 'interface';

  navItems.forEach((item) => {
    item.classList.toggle('active', item.dataset.section === section);
  });

  const paint = () => {
    if (featureTag) featureTag.textContent = ch.mockTag || section.toUpperCase();
    if (featureList && Array.isArray(ch.features)) {
      featureList.innerHTML = ch.features.map((f) => `<li>${f}</li>`).join('');
    }
    if (!featureMain) return;
    featureMain.classList.remove('is-leave');
    featureMain.classList.add('is-enter');
  };

  if (featureMain && lastMockSection && lastMockSection !== section) {
    featureMain.classList.remove('is-enter');
    featureMain.classList.add('is-leave');
    window.setTimeout(paint, 180);
  } else {
    paint();
  }

  lastMockSection = section;
}

export function syncOverlay(progress) {
  const ch = chapterAt(progress);
  const progressBucket = Math.floor(progress * 200);
  const chapterChanged = ch.id !== lastOverlayChapter;
  lastOverlayChapter = ch.id;
  lastOverlayBucket = progressBucket;

  const labelEl = document.getElementById('story-label');
  const titleEl = document.getElementById('story-title');
  const bodyEl = document.getElementById('story-body');
  const detailsEl = document.getElementById('story-details');
  const railLabel = document.getElementById('assembly-label');
  const fill = document.getElementById('progress-fill');
  const endCta = document.getElementById('end-cta');
  const endOrbit = document.getElementById('end-orbit');
  const storyCard = document.getElementById('story-card');
  const featurePanel = document.getElementById('feature-panel');
  const pressHint = document.getElementById('press-hint');
  const dots = document.querySelectorAll('.chapter-dot');
  const chrome = document.querySelector('.workbench-chrome');

  if (fill) fill.style.width = `${progress * 100}%`;

  const isDemo = ch.phase === 'demo';
  const showEnd = !!ch.showCta && progress >= 0.90;
  const stageId = ch.stage || STAGES[0].id;
  const stageIdx = Math.max(0, STAGES.findIndex((s) => s.id === stageId));
  const stage = STAGES[stageIdx] || STAGES[0];

  document.body.classList.toggle('is-end-reveal', showEnd);
  if (endOrbit) {
    endOrbit.classList.toggle('is-visible', showEnd);
    endOrbit.setAttribute('aria-hidden', showEnd ? 'false' : 'true');
  }
  if (chrome) chrome.classList.toggle('is-dimmed', showEnd);

  syncDeskOverlay(ch, isDemo, showEnd);
  syncPressHint(pressHint, ch, showEnd);

  dots.forEach((dot, i) => {
    dot.classList.toggle('active', i <= stageIdx);
    dot.classList.toggle('current', i === stageIdx);
  });
  if (railLabel) {
    const n = String(stageIdx + 1).padStart(2, '0');
    railLabel.textContent = `${n} // ${stage.label}`;
  }

  if (!chapterChanged) {
    if (endCta) endCta.hidden = !showEnd;
    if (storyCard) {
      storyCard.hidden = showEnd;
      storyCard.classList.toggle('dimmed', showEnd);
      storyCard.classList.toggle('demo-mode', isDemo && !showEnd);
    }
    if (featurePanel) {
      const showPanel = isDemo && !showEnd;
      featurePanel.hidden = !showPanel;
      featurePanel.classList.toggle('visible', showPanel);
    }
    return;
  }

  if (labelEl) labelEl.textContent = ch.label;
  if (titleEl) titleEl.textContent = ch.title;
  if (bodyEl) bodyEl.textContent = ch.body;

  if (detailsEl) {
    if (Array.isArray(ch.details) && ch.details.length) {
      detailsEl.innerHTML = ch.details.map((d) => {
        if (typeof d === 'string') return `<li>${d}</li>`;
        return `<li><span class="story-step mono">${d.step}</span><span class="story-step-text">${d.text}</span></li>`;
      }).join('');
      detailsEl.hidden = false;
    } else {
      detailsEl.innerHTML = '';
      detailsEl.hidden = true;
    }
  }

  if (featurePanel) {
    const showPanel = isDemo && !showEnd;
    featurePanel.hidden = !showPanel;
    featurePanel.classList.toggle('visible', showPanel);
    if (showPanel) applyFeatureSection(ch);
    else lastMockSection = null;
  }

  if (endCta) endCta.hidden = !showEnd;
  if (storyCard) {
    storyCard.hidden = showEnd;
    storyCard.classList.toggle('dimmed', showEnd);
    storyCard.classList.toggle('demo-mode', isDemo && !showEnd);
    storyCard.classList.toggle('is-hero', ch.id === 'idea');
  }
}

function scrollToStage(start) {
  const stage = document.getElementById('workbench-stage');
  if (!stage || typeof ScrollTrigger === 'undefined') {
    const doc = document.documentElement;
    const max = doc.scrollHeight - window.innerHeight;
    window.scrollTo({ top: Math.max(0, start) * max, behavior: 'smooth' });
    return;
  }
  const st = ScrollTrigger.getAll().find((t) => t.trigger === stage || t.vars?.trigger === '#workbench-stage');
  if (st) {
    const y = st.start + (st.end - st.start) * Math.max(0, Math.min(1, start));
    window.scrollTo({ top: y, behavior: 'smooth' });
    return;
  }
  const max = document.documentElement.scrollHeight - window.innerHeight;
  window.scrollTo({ top: Math.max(0, start) * max, behavior: 'smooth' });
}

function buildDots() {
  const host = document.getElementById('chapter-dots');
  if (!host) return;
  host.innerHTML = '';
  STAGES.forEach((stage, i) => {
    const dot = document.createElement('button');
    dot.type = 'button';
    dot.className = 'chapter-dot' + (i === 0 ? ' active current' : '');
    dot.dataset.stage = stage.id;
    dot.setAttribute('aria-label', stage.label);
    dot.addEventListener('click', () => scrollToStage(stage.start));
    host.appendChild(dot);
  });
}

buildDots();
syncOverlay(0);

window.__OASIS_STORY = { CHAPTERS, STAGES, chapterAt, chapterLocal, syncOverlay };
