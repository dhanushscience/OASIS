// The installed apps (names only), fetched once. The first fetch can take a few seconds (Windows
// is asked), so the Profiles screen starts it as soon as it opens.
export const installed = $state({ /** @type {string[]} */ names: [], loaded: false });

let started = false;

export async function loadApps() {
  if (started) return;
  started = true;
  try {
    const list = await window.__TAURI__.core.invoke('installed_apps');
    installed.names = list.map((/** @type {{ name: string }} */ a) => a.name);
  } catch {
    started = false; // try again next time; the list is only a hint
  }
  installed.loaded = true;
}

// The programs with a window open right now. Asked for each time the box is clicked: it changes.
/** @type {{ list: { name: string, path: string, title: string }[] }} */
export const running = $state({ list: [] });

export async function loadRunning() {
  try {
    running.list = await window.__TAURI__.core.invoke('running_apps');
  } catch {
    running.list = [];
  }
}
