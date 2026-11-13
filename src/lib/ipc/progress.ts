import { listen, type UnlistenFn } from '@tauri-apps/api/event';

/**
 * What a backend says while a long piece of work goes on — a build, a test run — in words (`Compiling voriz`,
 * `12 passed, 1 failed`), with a count when it knows one. One topic for every backend (`arbor://progress`); the
 * same events the shell forwards to an AI client waiting on the call.
 */
export interface BackendProgress {
  message: string;
  done?: number;
  total?: number;
}

export function onBackendProgress(handler: (p: BackendProgress) => void): Promise<UnlistenFn> {
  return listen<BackendProgress>('arbor://progress', (e) => handler(e.payload));
}
