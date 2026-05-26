import { untrack } from 'svelte';
import { getAppearanceConfig, setAppearanceConfig } from '$lib/ipc/config';
import type {
  AppearanceConfig, WindowControlsStyle, ActivityBarPosition,
} from '$lib/types/config';

const DEFAULT: AppearanceConfig = {
  window_controls_style: 'mac',
  font_scale:            1.0,
  use_theme_fonts:       false,
  activity_bar_position: 'left',
  compact_title_bar:     false,
  parked_modals_max:     5,
};

const FONT_SCALE_MIN = 0.8;
const FONT_SCALE_MAX = 1.4;

export const PARKED_MODALS_MAX_MIN = 1;
export const PARKED_MODALS_MAX_MAX = 20;

function clampScale(n: number): number {
  if (!Number.isFinite(n)) return 1;
  return Math.max(FONT_SCALE_MIN, Math.min(FONT_SCALE_MAX, n));
}

function clampParkedMax(n: number): number {
  if (!Number.isFinite(n)) return DEFAULT.parked_modals_max;
  const i = Math.round(n);
  return Math.max(PARKED_MODALS_MAX_MIN, Math.min(PARKED_MODALS_MAX_MAX, i));
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
  let activityBarPosition  = $state<ActivityBarPosition>(DEFAULT.activity_bar_position);
  let compactTitleBar      = $state<boolean>(DEFAULT.compact_title_bar);
  let parkedModalsMax      = $state<number>(DEFAULT.parked_modals_max);
  let loaded               = $state(false);

  // Defaults applied once at module load; subsequent changes go through the
  // setters and through `loadConfig`. Wrap in `untrack` so the references are
  // explicit one-shots and don't trip `state_referenced_locally`.
  untrack(() => {
    applyAttribute(windowControlsStyle);
    applyFontScale(fontScale);
    applyActivityBarPosition(activityBarPosition);
    applyCompactTitleBar(compactTitleBar);
  });

  async function loadConfig() {
    try {
      const cfg = await getAppearanceConfig();
      windowControlsStyle = (cfg.window_controls_style === 'windows' ? 'windows' : 'mac');
      fontScale           = clampScale(cfg.font_scale);
      useThemeFonts       = !!cfg.use_theme_fonts;
      activityBarPosition = normalisePosition(cfg.activity_bar_position);
      compactTitleBar     = !!cfg.compact_title_bar;
      parkedModalsMax     = clampParkedMax(cfg.parked_modals_max);
      applyAttribute(windowControlsStyle);
      applyFontScale(fontScale);
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
      activity_bar_position: activityBarPosition,
      compact_title_bar:     compactTitleBar,
      parked_modals_max:     parkedModalsMax,
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

  function setParkedModalsMax(n: number) {
    const clamped = clampParkedMax(n);
    if (clamped === parkedModalsMax) return;
    parkedModalsMax = clamped;
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
    get activityBarPosition()  { return activityBarPosition; },
    get compactTitleBar()      { return compactTitleBar; },
    get parkedModalsMax()      { return parkedModalsMax; },
    get loaded()               { return loaded; },
    loadConfig,
    setWindowControlsStyle,
    setFontScale,
    setUseThemeFonts,
    setActivityBarPosition,
    setCompactTitleBar,
    setParkedModalsMax,
    syncThemeFonts,
  };
}

export const appearanceStore = createAppearanceStore();
