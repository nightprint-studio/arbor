import { getAppearanceConfig, setAppearanceConfig } from '$lib/ipc/config';
import type {
  AppearanceConfig, WindowControlsStyle, UiDensity, ActivityBarPosition,
} from '$lib/types/config';

const DEFAULT: AppearanceConfig = {
  window_controls_style: 'mac',
  font_scale:            1.0,
  use_theme_fonts:       false,
  ui_density:            'comfortable',
  activity_bar_position: 'left',
  compact_title_bar:     false,
};

const FONT_SCALE_MIN = 0.8;
const FONT_SCALE_MAX = 1.4;

function clampScale(n: number): number {
  if (!Number.isFinite(n)) return 1;
  return Math.max(FONT_SCALE_MIN, Math.min(FONT_SCALE_MAX, n));
}

/** Mirror the active style onto `<html data-window-controls="…">` so global
 *  CSS rules (notably `.mac-close-btn`, used by 15+ modal/panel headers) can
 *  swap their look without every callsite knowing about the store. */
function applyAttribute(style: WindowControlsStyle) {
  if (typeof document === 'undefined') return;
  document.documentElement.dataset.windowControls = style;
}

function applyFontScale(scale: number) {
  if (typeof document === 'undefined') return;
  document.documentElement.style.setProperty('--font-scale', String(scale));
}

function applyUiDensity(density: UiDensity) {
  if (typeof document === 'undefined') return;
  document.documentElement.dataset.uiDensity = density;
}

function applyActivityBarPosition(pos: ActivityBarPosition) {
  if (typeof document === 'undefined') return;
  document.documentElement.dataset.activityBarPosition = pos;
}

function applyCompactTitleBar(on: boolean) {
  if (typeof document === 'undefined') return;
  if (on) document.documentElement.dataset.compactTitleBar = 'true';
  else    delete document.documentElement.dataset.compactTitleBar;
}

function applyThemeFontVars(useThemeFonts: boolean, themeVars: Record<string, string>) {
  if (typeof document === 'undefined') return;
  const root = document.documentElement;
  const ui   = (themeVars['--theme-font-ui']   ?? '').trim();
  const code = (themeVars['--theme-font-code'] ?? '').trim();
  if (useThemeFonts && ui)   root.style.setProperty('--theme-font-ui-active', ui);
  else                       root.style.removeProperty('--theme-font-ui-active');
  if (useThemeFonts && code) root.style.setProperty('--theme-font-code-active', code);
  else                       root.style.removeProperty('--theme-font-code-active');
}

function normaliseDensity(v: unknown): UiDensity {
  return v === 'compact' || v === 'spacious' ? v : 'comfortable';
}
function normalisePosition(v: unknown): ActivityBarPosition {
  return v === 'right' || v === 'hidden' ? v : 'left';
}

function createAppearanceStore() {
  // Defaults render immediately on first paint; disk values overwrite once
  // `loadConfig()` resolves (called from AppShell.onMount). Persistence is
  // routed through the backend — never localStorage.
  let windowControlsStyle  = $state<WindowControlsStyle>(DEFAULT.window_controls_style);
  let fontScale            = $state<number>(DEFAULT.font_scale);
  let useThemeFonts        = $state<boolean>(DEFAULT.use_theme_fonts);
  let uiDensity            = $state<UiDensity>(DEFAULT.ui_density);
  let activityBarPosition  = $state<ActivityBarPosition>(DEFAULT.activity_bar_position);
  let compactTitleBar      = $state<boolean>(DEFAULT.compact_title_bar);
  let loaded               = $state(false);

  applyAttribute(windowControlsStyle);
  applyFontScale(fontScale);
  applyUiDensity(uiDensity);
  applyActivityBarPosition(activityBarPosition);
  applyCompactTitleBar(compactTitleBar);

  async function loadConfig() {
    try {
      const cfg = await getAppearanceConfig();
      windowControlsStyle = (cfg.window_controls_style === 'windows' ? 'windows' : 'mac');
      fontScale           = clampScale(cfg.font_scale);
      useThemeFonts       = !!cfg.use_theme_fonts;
      uiDensity           = normaliseDensity(cfg.ui_density);
      activityBarPosition = normalisePosition(cfg.activity_bar_position);
      compactTitleBar     = !!cfg.compact_title_bar;
      applyAttribute(windowControlsStyle);
      applyFontScale(fontScale);
      applyUiDensity(uiDensity);
      applyActivityBarPosition(activityBarPosition);
      applyCompactTitleBar(compactTitleBar);
      loaded = true;
    } catch {
      // First-run / backend not ready — keep defaults; next call will retry.
    }
  }

  function persist() {
    void setAppearanceConfig({
      window_controls_style: windowControlsStyle,
      font_scale:            fontScale,
      use_theme_fonts:       useThemeFonts,
      ui_density:            uiDensity,
      activity_bar_position: activityBarPosition,
      compact_title_bar:     compactTitleBar,
    }).catch(() => {});
  }

  function setWindowControlsStyle(s: WindowControlsStyle) {
    if (windowControlsStyle === s) return;
    windowControlsStyle = s;
    applyAttribute(s);
    persist();
  }

  function setFontScale(n: number) {
    const clamped = clampScale(n);
    if (clamped === fontScale) return;
    fontScale = clamped;
    applyFontScale(clamped);
    persist();
  }

  function setUiDensity(d: UiDensity) {
    if (uiDensity === d) return;
    uiDensity = d;
    applyUiDensity(d);
    persist();
  }

  function setActivityBarPosition(p: ActivityBarPosition) {
    if (activityBarPosition === p) return;
    activityBarPosition = p;
    applyActivityBarPosition(p);
    persist();
  }

  function setCompactTitleBar(on: boolean) {
    if (compactTitleBar === on) return;
    compactTitleBar = on;
    applyCompactTitleBar(on);
    persist();
  }

  /** Toggle whether the active theme's optional font preferences win over
   *  the global font stack. `themeVars` is the active theme's `vars` map so
   *  the change can be applied without going through the theme store. */
  function setUseThemeFonts(value: boolean, themeVars: Record<string, string>) {
    if (useThemeFonts === value) return;
    useThemeFonts = value;
    applyThemeFontVars(value, themeVars);
    persist();
  }

  /** Re-apply the `--theme-font-*-active` CSS variables — called by the
   *  theme store whenever the active theme (or its vars) changes so the
   *  opt-in stays in sync without forcing every caller to know about both
   *  stores. */
  function syncThemeFonts(themeVars: Record<string, string>) {
    applyThemeFontVars(useThemeFonts, themeVars);
  }

  return {
    get windowControlsStyle()  { return windowControlsStyle; },
    get fontScale()            { return fontScale; },
    get useThemeFonts()        { return useThemeFonts; },
    get uiDensity()            { return uiDensity; },
    get activityBarPosition()  { return activityBarPosition; },
    get compactTitleBar()      { return compactTitleBar; },
    get loaded()               { return loaded; },
    loadConfig,
    setWindowControlsStyle,
    setFontScale,
    setUseThemeFonts,
    setUiDensity,
    setActivityBarPosition,
    setCompactTitleBar,
    syncThemeFonts,
  };
}

export const appearanceStore = createAppearanceStore();
