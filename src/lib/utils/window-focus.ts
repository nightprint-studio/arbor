/**
 * Follow this window's OS focus — for pollers that should rest while nobody is looking.
 *
 * Tauri's own focus event, as `AppShell` uses: the DOM's `blur` / `focus` are unreliable across the
 * webview's windows. In the background the webview is power-throttled while a backend is not, so a
 * poller that keeps asking only fills a queue that is delivered all at once on the way back.
 *
 * If the window API is unavailable (a plain browser, a test) the watch reports "focused" forever, so
 * a poller behaves exactly as it did before it was gated.
 */

import { getCurrentWindow } from '@tauri-apps/api/window';

export interface WindowFocusWatch {
  /** Whether the window has the focus right now. */
  readonly focused: boolean;
  /** Stop listening. Safe to call more than once. */
  stop(): void;
}

/**
 * Start watching. `onChange` fires on every focus change (not for the initial state, which
 * `focused` carries). Resolves once the listener is attached.
 */
export async function watchWindowFocus(onChange: (focused: boolean) => void): Promise<WindowFocusWatch> {
  let focused = true;
  let stopped = false;
  let unlisten: (() => void) | null = null;
  try {
    const win = getCurrentWindow();
    focused = await win.isFocused();
    const un = await win.onFocusChanged(({ payload }) => {
      focused = payload;
      if (!stopped) onChange(payload);
    });
    if (stopped) un();
    else unlisten = un;
  } catch {
    focused = true;
  }
  return {
    get focused() { return focused; },
    stop() {
      stopped = true;
      unlisten?.();
      unlisten = null;
    },
  };
}
