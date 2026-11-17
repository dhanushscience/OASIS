// The voice model setup as the window sees it (src/models.rs): one shared state for the top strip,
// the first-start pop-up and Settings › Voice models.

const tauri = window.__TAURI__;

/**
 * @typedef {{ id: string, label: string, size_mb: number, installed: boolean, suits: boolean, what: string }} SetOption
 * @typedef {{ choice: string, recommended: string | null, reason: string, pc: string[], target: string | null,
 *   sets: SetOption[], phase: string, check: string | null, check_slow: boolean, polish: boolean,
 *   done_mb: number, total_mb: number, eta_s: number | null, file: string, error: string | null,
 *   voice_ready: boolean, below_floor: boolean, upgrade: boolean }} ModelsInfo
 */

/** @type {{ info: ModelsInfo | null }} */
export const models = $state({ info: null });

let started = false;

export function startModels() {
  if (started || !tauri) return;
  started = true;
  tauri.core.invoke('models_status').then((/** @type {ModelsInfo} */ i) => (models.info = i));
  tauri.event.listen('models-status', (/** @type {{ payload: ModelsInfo }} */ e) => (models.info = e.payload));
}

/** @param {string} command @param {Record<string, unknown>} [args] */
export const modelsCall = (command, args) => tauri?.core.invoke(command, args).catch(() => {});

/** "about 2 min left", "under a minute left". @param {number | null} s */
export function etaText(s) {
  if (s === null || s === undefined) return '';
  if (s < 60) return 'under a minute left';
  if (s < 3600) return `about ${Math.round(s / 60)} min left`;
  return `about ${(s / 3600).toFixed(1)} h left`;
}

/** The set's name as people read it. @param {string | null} id */
export const setName = (id) => ({ light: 'Basic', balanced: 'Balanced', quality: 'Quality' })[id ?? ''] ?? '';

/** The "Better accuracy" button's text, with the download size of Balanced. @param {ModelsInfo | null} info */
export function upgradeText(info) {
  const balanced = info?.upgrade ? info.sets.find((s) => s.id === 'balanced') : null;
  return balanced ? `Better accuracy (${balanced.installed ? 'already downloaded' : `${balanced.size_mb} MB download`})` : '';
}
