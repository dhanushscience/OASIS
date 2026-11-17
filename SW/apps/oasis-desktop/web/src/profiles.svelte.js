// The Profiles screen's data: the profiles as the engine reports them (`settings_view`), what is
// selected, and every change, through the same commands the old editor used. Every change reloads
// the view, so the screen always shows what is really saved.
import { overlaps, tokens } from './matching.js';

const { invoke } = window.__TAURI__?.core ?? { invoke: async () => { throw new Error('Not running inside OASIS.'); } };

/**
 * @typedef {{ kind: string, value: string }} Step
 * @typedef {{ kind: string, value: string, label: string, steps: Step[] }} Button
 * @typedef {{ index: number, name: string, context_match: string, is_primary: boolean, can_delete: boolean, when: string, panel_id: string, panel_label: string, label_mode: string, buttons: Button[] }} Config
 * @typedef {{ index: number, name: string, context_match: string, kind: string, configs: Config[], primary_config: number, multi: boolean, label_mode: string, icon_emoji: string }} Profile
 */

export const data = $state({
  /** @type {Profile[]} */ profiles: [],
  /** @type {string[]} */ actionTypes: [],
  /** @type {string[]} */ stepTypes: [],
  /** @type {string[]} */ writableStepTypes: [],
  /** @type {{ value: string, label: string }[]} */ panelPresets: [],
  /** @type {{ value: string, label: string }[]} */ whenOptions: [],
  maxSets: 8,
  /** @type {Record<string, number>} */ uses: {},
  loaded: false,
  /** @type {string | null} */ error: null,
  /** the profile chosen, an index into `profiles`: the strip follows this at once */ profile: 0,
  /** the profile the screen shows: it follows `profile` 80 ms later, once the old one has faded out */ shown: 0,
  /** the old profile is fading out */ out: false,
  /** +1 going to a later tile, -1 to an earlier one */ dir: 1,
  /** the set of buttons being edited; null = the profile's main set */ config: /** @type {number | null} */ (null),
  /** 0, 1 or 2 */ button: 0,
  /** the profile settings panel covers the pad */ settings: false,
  /** profile number -> picture icon as a data address */ icons: /** @type {Record<number, string>} */ ({}),
  /** a profile that already exists for what was asked: shown as "modify it?" */ duplicate: /** @type {{ index: number, name: string, target: string } | null} */ (null),
  /** counts up when the settings should open on the profile now shown */ editRequest: 0,
  /** the ways of pressing the three buttons that are on */ input: { hardware: true, keyboard: false, keys: ['', '', ''], exclusive: true },
});

export const isEverywhere = (/** @type {Profile} */ p) => p.context_match.trim() === '' || p.context_match.trim() === '*';

/**
 * Tile order: the default profile ("everything else") always first, then the rest in the order they were
 * made (a new one is added at the end). The person can drag a tile to rearrange the rest.
 */
export function order() {
  const index = (/** @type {Profile} */ p) => p.index;
  return [...data.profiles.filter(isEverywhere).map(index), ...data.profiles.filter((p) => !isEverywhere(p)).map(index)];
}

export const current = () => data.profiles[data.shown];
const cfgIndex = () => /** @type {Config} */ (currentConfig()).index;

export function currentConfig() {
  const p = current();
  if (!p) return undefined;
  const i = p.multi && data.config != null && data.config < p.configs.length ? data.config : p.primary_config;
  return p.configs[i];
}

/** Reads the profiles again. `keep` leaves the selection where it is. */
export async function load(keep = true) {
  try {
    const view = await invoke('settings_view');
    data.profiles = view.profiles;
    data.actionTypes = view.action_types;
    data.stepTypes = view.step_types;
    data.writableStepTypes = view.writable_step_types;
    data.panelPresets = view.panel_presets;
    data.whenOptions = view.when_options;
    data.maxSets = view.max_sets;
    data.input = view.input;
    data.error = null;
    invoke('profile_icons').then((/** @type {Record<number, string>} */ i) => (data.icons = i), () => {});
    invoke('profile_use_counts').then((/** @type {Record<string, number>} */ u) => (data.uses = u), () => {});
  } catch (e) {
    data.error = String(e);
  }
  if (!keep || data.profile >= data.profiles.length) jump(0);
  else if (data.shown >= data.profiles.length) jump(data.profile);
  if (!current()?.multi) data.config = null;
  data.loaded = true;
}

/** How long the old profile takes to fade out before the new one is shown (DESIGN.md section 0). */
export const OUT_MS = 80;
let swapTimer = 0;
const reducedMotion = () => window.matchMedia?.('(prefers-reduced-motion: reduce)').matches ?? false;

/** Show a profile at once, with no fade (opening, adding, deleting). */
export function jump(/** @type {number} */ index) {
  clearTimeout(swapTimer);
  data.profile = data.shown = index;
  data.config = null;
  data.button = 0;
  data.out = false;
}

/**
 * Choose a profile. The strip moves now; the screen fades the old one out, then shows the new one.
 * A new choice while that happens starts again from where it is, so holding "next" turns smoothly.
 */
export function select(/** @type {number} */ index) {
  if (index === data.profile) return;
  const tiles = order();
  data.dir = tiles.indexOf(index) > tiles.indexOf(data.profile) ? 1 : -1;
  data.profile = index;
  if (reducedMotion()) return jump(index);
  data.out = true;
  clearTimeout(swapTimer);
  swapTimer = setTimeout(() => jump(data.profile), OUT_MS);
}

async function change(/** @type {() => Promise<any>} */ call) {
  const result = await call();
  await load();
  return result;
}

export const saveButton = (/** @type {number} */ button, /** @type {Button} */ edit) =>
  change(() => invoke('save_button', { profile: current().index, config: cfgIndex(), button, edit }));

export const saveProfile = (/** @type {{ name: string, context_match: string, kind: string }} */ p) =>
  change(() => invoke('save_profile', { profile: current().index, name: p.name, contextMatch: p.context_match, kind: p.kind }));

export const saveLook = (/** @type {string} */ labelMode, /** @type {string} */ iconEmoji) =>
  change(() => invoke('save_profile_look', { profile: current().index, labelMode, iconEmoji }));

/**
 * The other profile that already applies to this app or site, if any. When there is one it is put up
 * as a "modify it?" question and `true` comes back, so the caller stops.
 */
export function askIfExists(/** @type {string} */ target, /** @type {number} */ except = -1) {
  const found = data.profiles.find((p) => p.index !== except && overlaps(target, p.context_match));
  data.duplicate = found ? { index: found.index, name: found.name, target: target.trim() } : null;
  return !!found;
}

/** Go to that profile and open its settings. */
export function editExisting(/** @type {number} */ index) {
  data.duplicate = null;
  jump(index);
  data.editRequest++;
}

export const setIconImage = (/** @type {number[] | null} */ png) =>
  change(() => invoke('set_profile_icon', { profile: current().index, png }));

export const appIcon = (/** @type {string} */ target) => invoke('app_icon', { target });

/**
 * A new profile starts with the app's own icon, or the website's logo. It is fetched in the
 * background; if it cannot be had, the profile just keeps its letter.
 */
async function giveDefaultIcon(/** @type {number} */ index, /** @type {string} */ target, /** @type {string} */ kind) {
  try {
    const png = kind === 'website' ? await invoke('site_icon', { host: tokens(target)[0] ?? '' }) : await invoke('app_icon', { target });
    await change(() => invoke('set_profile_icon', { profile: index, png }));
  } catch { /* the letter stays */ }
}

export async function addProfile(/** @type {string} */ name, /** @type {string} */ target, /** @type {string} */ kind) {
  const index = await invoke('add_profile', { name, target, kind });
  await load();
  jump(index);
  if (target.trim() && target.trim() !== '*') void giveDefaultIcon(index, target, kind);
  return index;
}

export async function deleteProfile() {
  const was = data.shown;
  await change(() => invoke('delete_profile', { profile: current().index }));
  jump(Math.max(0, Math.min(was, data.profiles.length - 1)));
}

/** Move the profile at stored position `from` to position `to`, and show it there. */
export async function moveProfileTo(/** @type {number} */ from, /** @type {number} */ to) {
  await change(() => invoke('move_profile', { from, to }));
  jump(to);
}

export async function moveProfile(/** @type {number} */ delta) {
  const from = data.shown;
  await change(() => invoke('move_profile', { from, to: from + delta }));
  jump(from + delta);
}

export const saveInput = (/** @type {{ hardware: boolean, keyboard: boolean, keys: string[], exclusive: boolean }} */ i) =>
  change(() => invoke('save_input', { hardware: i.hardware, keyboard: i.keyboard, keys: i.keys, exclusive: i.exclusive }));

export const setMulti = (/** @type {boolean} */ enabled) => change(() => invoke('set_multi_config', { profile: current().index, enabled }));

export async function addSet(/** @type {{ name: string, context_match: string, when: string, panel_id: string, label_mode: string }} */ edit) {
  const index = await invoke('add_config', { profile: current().index, edit });
  await load();
  data.config = index;
}

export const updateSet = (/** @type {{ name: string, context_match: string, when: string, panel_id: string, label_mode: string }} */ edit) =>
  change(() => invoke('update_config', { profile: current().index, config: cfgIndex(), edit }));

export async function deleteSet() {
  await change(() => invoke('delete_config', { profile: current().index, config: cfgIndex() }));
  data.config = null;
}

// ---- how a button reads in the window ---------------------------------------------------------

/** The four kinds on the board's segmented control; anything else is shown by its own name. */
export const SEGMENTS = ['Keys', 'App / site', 'Text', 'Steps'];

export function segmentOf(/** @type {string} */ kind) {
  return { Shortcut: 0, LaunchApp: 1, OpenUrl: 1, PasteText: 2, Automation: 3, '': 0 }[kind] ?? -1;
}

export { looksLikeSite } from './actionKinds.js';
import { splitBrowser } from './actionKinds.js';

/** A site with its own browser is saved as an app ("chrome|address"); it is still shown as a website. */
const siteIn = (/** @type {Button} */ b) => (b.kind === 'LaunchApp' ? splitBrowser(b.value) : null);

export function kindWord(/** @type {Button} */ b) {
  const known = { Shortcut: 'Keys', LaunchApp: 'App', OpenUrl: 'Website', PasteText: 'Text', Automation: 'Steps', '': 'Not set' };
  return siteIn(b) ? 'Website' : /** @type {Record<string, string>} */ (known)[b.kind] ?? b.kind;
}

export function doesWord(/** @type {Button} */ b) {
  if (b.kind === 'Automation') return `${b.steps.length} step${b.steps.length === 1 ? '' : 's'}`;
  return siteIn(b)?.url || b.value || (b.kind ? '' : 'nothing yet');
}
