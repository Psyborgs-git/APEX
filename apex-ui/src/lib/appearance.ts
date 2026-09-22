import { getAppSettings } from './tauri';

/** Apply theme + density to <html> data attributes (tokens.css hooks). */
export function applyAppearance(theme?: string, density?: string) {
  const el = document.documentElement;
  el.dataset.theme = theme === 'light' ? 'light' : 'dark';
  el.dataset.density = density === 'compact' ? 'compact' : 'comfortable';
}

/** Load persisted appearance from settings and apply it. */
export async function syncAppearance() {
  try {
    const settings = await getAppSettings();
    applyAppearance(settings.appearance.theme, settings.appearance.density);
  } catch {
    // defaults stay
  }
}
