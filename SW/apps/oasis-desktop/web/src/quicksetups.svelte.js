// The Quick Launch setups (id and name), for the button editor's "Quick Launch" step. Asked for when
// that step is picked, and again after the Quick Launch screen saves or deletes one.
/** @type {{ list: { id: string, name: string }[] }} */
export const quick = $state({ list: [] });

export async function loadQuick() {
  try {
    const all = await window.__TAURI__.core.invoke('quick_setups');
    quick.list = all.map((/** @type {{ id: string, name: string }} */ s) => ({ id: s.id, name: s.name }));
  } catch {
    quick.list = [];
  }
}
