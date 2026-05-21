import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import {
  getGitStatus, redetectGit, setGitPath, downloadPortableGit, cancelGitDownload, verifyGitPath,
  type GitCliStatus, type GitDownloadProgress,
} from '../ipc/gitCli';

export type SetupPhase = 'detecting' | 'ready' | 'missing' | 'downloading' | 'error';

function createGitCliStore() {
  let status   = $state<GitCliStatus | null>(null);
  let phase    = $state<SetupPhase>('detecting');
  let progress = $state<GitDownloadProgress | null>(null);
  let lastError = $state<string | null>(null);
  let progressUnlisten: UnlistenFn | null = null;

  function applyStatus(s: GitCliStatus) {
    status = s;
    phase  = (s.path && s.path.length > 0) ? 'ready' : 'missing';
  }

  /** First-launch detection — call once from AppShell.onMount before the rest
   *  of the bootstrap so any blocking modal renders before users can trigger
   *  git ops. */
  async function init(): Promise<void> {
    phase = 'detecting';
    try {
      applyStatus(await getGitStatus());
    } catch (e) {
      lastError = String(e);
      phase = 'error';
    }
    // Listen for streaming progress events from the Rust download command.
    if (!progressUnlisten) {
      progressUnlisten = await listen<GitDownloadProgress>(
        'arbor://git-download-progress',
        (e) => {
          progress = e.payload;
          if (e.payload.stage === 'error') {
            lastError = e.payload.message;
            phase = 'missing';
          }
        },
      );
    }
  }

  async function refresh(): Promise<GitCliStatus> {
    const s = await redetectGit();
    applyStatus(s);
    return s;
  }

  async function setPath(path: string | null): Promise<GitCliStatus> {
    lastError = null;
    try {
      const s = await setGitPath(path);
      applyStatus(s);
      return s;
    } catch (e) {
      lastError = String(e);
      throw e;
    }
  }

  async function verify(path: string): Promise<string> {
    return verifyGitPath(path);
  }

  async function download(): Promise<GitCliStatus> {
    lastError = null;
    progress  = { stage: 'resolving', message: 'Querying release…', bytes: 0, total: 0 };
    phase     = 'downloading';
    try {
      const s = await downloadPortableGit();
      applyStatus(s);
      progress = null;
      return s;
    } catch (e) {
      const msg = String(e);
      // "Operation cancelled" comes back as a regular error; treat it as a
      // graceful abort, not a failure.
      if (msg.toLowerCase().includes('cancelled')) {
        lastError = null;
      } else {
        lastError = msg;
      }
      // Restore phase from the status we had before kicking off the download.
      // A failed/cancelled portable download mustn't make Arbor claim there's
      // no git when the user already had a working binary.
      phase    = status?.path ? 'ready' : 'missing';
      progress = null;
      throw e;
    }
  }

  /** Best-effort abort of the running download. */
  async function cancel(): Promise<void> {
    try { await cancelGitDownload(); } catch { /* nothing to cancel */ }
  }

  return {
    get status()    { return status; },
    get phase()     { return phase; },
    get progress()  { return progress; },
    get lastError() { return lastError; },
    init, refresh, setPath, verify, download, cancel,
  };
}

export const gitCliStore = createGitCliStore();
