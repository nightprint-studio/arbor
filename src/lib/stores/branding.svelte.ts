import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

/**
 * Plugin-applied branding overrides — currently the app logo. Lives
 * entirely in RAM: the backend mirrors it in `AppState.branding` so HTML
 * exports can embed the same SVG, but nothing is persisted.
 *
 * The store hydrates once on init via `get_branding` (covers the case
 * where a plugin set the branding during its `on_plugin_load` before the
 * frontend was even listening) and then stays in sync via
 * `arbor://branding-changed` events.
 */

interface BrandingDto {
  logo_svg:         string | null;
  /** Absolute path to a PNG / ICO file used by the OS-level window-icon
   *  API. Tracked here for diagnostics; the actual swap happens on the
   *  backend via `WebviewWindow::set_icon` since this can't be done
   *  from the webview. */
  window_icon_path: string | null;
  owner:            string | null;
}

let _logoSvg        = $state<string | null>(null);
let _windowIconPath = $state<string | null>(null);
let _owner          = $state<string | null>(null);
let _ready          = $state(false);
let _started        = false;

async function init() {
  if (_started) return;
  _started = true;

  try {
    const dto = await invoke<BrandingDto>('get_branding');
    _logoSvg        = dto.logo_svg;
    _windowIconPath = dto.window_icon_path;
    _owner          = dto.owner;
  } catch {
    // Backend unavailable in dev mode — keep defaults.
  }

  await listen<BrandingDto>('arbor://branding-changed', (e) => {
    _logoSvg        = e.payload.logo_svg;
    _windowIconPath = e.payload.window_icon_path;
    _owner          = e.payload.owner;
  });

  _ready = true;
}

export const brandingStore = {
  get ready()   { return _ready; },
  /** Inline SVG markup for the override logo, or `null` for the default. */
  get logoSvg() { return _logoSvg; },
  /** Path to the raster window icon, or `null` for the default. */
  get windowIconPath() { return _windowIconPath; },
  /** Plugin name that owns the current override (diagnostics). */
  get owner()   { return _owner; },
  init,
};
