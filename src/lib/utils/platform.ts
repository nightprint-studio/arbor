/**
 * Platform detection — single source of truth for the "is this macOS?" question.
 *
 * macOS drives real UX branches in Arbor: on the Mac the OS paints the native
 * traffic lights over our custom title bar (see `window/mod.rs::native_titlebar`),
 * so the frontend must hide its faux window controls and reserve a left gutter for
 * the lights. Detection is best-effort from the user agent (Tauri's WebView has no
 * reliable synchronous platform API on first paint); `navigator.platform` is
 * deprecated but still the most direct signal, with the UA string as fallback.
 */

/** True when running on macOS. Evaluated once at module load. */
export const isMac: boolean =
  typeof navigator !== 'undefined' &&
  /Mac/i.test(navigator.platform || navigator.userAgent || '');

/**
 * Stamp `<html data-os="mac">` (or `"other"`) so global CSS can branch without
 * every component importing {@link isMac}. Notably it drives `--mac-traffic-gutter`
 * (app.css), the left padding that keeps title-bar content clear of the native
 * traffic lights. Idempotent — safe to call from every window's entry point.
 */
export function applyOsAttribute(): void {
  if (typeof document === 'undefined') return;
  document.documentElement.dataset.os = isMac ? 'mac' : 'other';
}

/**
 * Track this window's fullscreen state and stamp `<html data-fullscreen>`.
 * In fullscreen macOS hides the native traffic lights, so the reserved gutter
 * (`--mac-traffic-gutter`) becomes dead space — the attribute lets global CSS
 * collapse it to 0 and hide the divider, so every header's leading content slides
 * left. macOS-only (the gutter is the sole consumer); a no-op elsewhere.
 *
 * Returns a disposer. Re-checks on every resize because Tauri exposes no dedicated
 * fullscreen event and entering/leaving fullscreen always resizes the window.
 */
export function watchFullscreen(): () => void {
  if (typeof document === 'undefined' || !isMac) return () => {};
  let active = true;
  let unlisten: (() => void) | null = null;
  // Lazy import keeps this module free of a hard Tauri dependency (it's also read
  // for `isMac` in plain-navigator contexts).
  void import('@tauri-apps/api/window').then(({ getCurrentWindow }) => {
    if (!active) return;
    const w = getCurrentWindow();
    const apply = async () => {
      try {
        const fs = await w.isFullscreen();
        if (active) document.documentElement.dataset.fullscreen = fs ? 'true' : 'false';
      } catch { /* non-Tauri / SSR */ }
    };
    void apply();
    void w.onResized(apply).then(fn => { if (active) unlisten = fn; else fn(); });
  });
  return () => { active = false; unlisten?.(); };
}
