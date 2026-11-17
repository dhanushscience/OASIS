// The Settings pop-up: open or shut, and which section shows. Anything may open it at a section
// (the pad card opens "Pad & keys", the voice-model strip opens "Voice models"), or with a search
// already typed (the search box in the top bar).
export const settingsUi = $state({ open: false, section: 'input', query: '' });

/** @param {string} [section] @param {string} [query] */
export function openSettings(section, query) {
  if (section) settingsUi.section = section;
  settingsUi.query = query ?? '';
  settingsUi.open = true;
}

export function closeSettings() {
  settingsUi.open = false;
}
