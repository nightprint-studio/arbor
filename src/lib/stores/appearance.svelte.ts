import { getAppearanceConfig, setAppearanceConfig } from '$lib/ipc/config';
import type { AppearanceConfig, WindowControlsStyle } from '$lib/types/config';

const DEFAULT: AppearanceConfig = { window_controls_style: 'mac' };

/** Mirror the active style onto `<html data-window-controls="…">` so global
 *  CSS rules (notably `.mac-close-btn`, used by 15+ modal/panel headers) can
 *  swap their look without every callsite knowing about the store. */
function applyAttribute(style: WindowControlsStyle) {
  if (typeof document === 'undefined') return;
  document.documentElement.dataset.windowControls = style;
}

function createAppearanceStore() {
  // Defaults render immediately on first paint; disk values overwrite once
  // `loadConfig()` resolves (called from AppShell.onMount). Persistence is
  // routed through the backend — never localStorage.
  let windowControlsStyle = $state<WindowControlsStyle>(DEFAULT.window_controls_style);
  let loaded              = $state(false);

  applyAttribute(windowControlsStyle);

  async function loadConfig() {
    try {
      const cfg = await getAppearanceConfig();
      windowControlsStyle = (cfg.window_controls_style === 'windows' ? 'windows' : 'mac');
      applyAttribute(windowControlsStyle);
      loaded = true;
    } catch {
      // First-run / backend not ready — keep defaults; next call will retry.
    }
  }

  function persist() {
    void setAppearanceConfig({ window_controls_style: windowControlsStyle }).catch(() => {});
  }

  function setWindowControlsStyle(s: WindowControlsStyle) {
    if (windowControlsStyle === s) return;
    windowControlsStyle = s;
    applyAttribute(s);
    persist();
  }

  return {
    get windowControlsStyle() { return windowControlsStyle; },
    get loaded()              { return loaded; },
    loadConfig,
    setWindowControlsStyle,
  };
}

export const appearanceStore = createAppearanceStore();
