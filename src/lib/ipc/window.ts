/**
 * Cross-window shell commands (DIRECT Tauri commands, not the product rpc bridge).
 */
import { invoke } from '@tauri-apps/api/core';

/**
 * Reveal the current window now that its shell has painted — the app-wide
 * anti-white-flash signal.
 *
 * Every launcher/product window is built HIDDEN by the shell: an opaque WebView2
 * window (a transparent one gets no input on Windows) would otherwise flash its white
 * default page during load. This tells the shell to show + focus the caller's own
 * window once the first frame is up. Fire it after the shell mounts + two frames (so
 * a real frame has painted); it's idempotent and only ever reveals the caller.
 */
export function signalWindowReady(): Promise<void> {
  return invoke('window_ready');
}
