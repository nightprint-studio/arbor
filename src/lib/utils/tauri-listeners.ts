import { listen } from '@tauri-apps/api/event';

type ListenerDef = { event: string; handler: (e: any) => void };

/**
 * Registers multiple Tauri event listeners and returns a single cleanup function.
 *
 * Usage inside a store:
 *   function setupListeners(): () => void {
 *     return setupTauriListeners([
 *       { event: 'arbor://my-event', handler: (e) => { ... } },
 *     ]);
 *   }
 */
export function setupTauriListeners(listeners: ListenerDef[]): () => void {
  let cancelled = false;
  const unlisteners: Array<() => void> = [];

  for (const { event, handler } of listeners) {
    // listen() is async; if cleanup is called before the promise resolves
    // (e.g. Svelte 5 dev-mode double-effect), immediately unlisten to avoid
    // registering a ghost listener that causes duplicate event handling.
    listen(event, handler).then(fn => {
      if (cancelled) {
        fn();
      } else {
        unlisteners.push(fn);
      }
    });
  }

  return () => {
    cancelled = true;
    for (const fn of unlisteners) fn();
  };
}
