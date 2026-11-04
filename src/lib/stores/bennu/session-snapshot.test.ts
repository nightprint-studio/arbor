import { describe, expect, it } from 'vitest';
import type { ProjectSession } from '$lib/ipc/bennu/config';
import {
  composeSnapshot, createWriteQueue, inStoredOrder, shouldRememberCaret, type SnapshotInput,
} from './session-snapshot';

const session = (root: string, files: string[] = []): ProjectSession => ({
  root, open_files: files, active_file: files[0] ?? '',
});
const canon = (p: string) => p.replace(/\\/g, '/');

const base: SnapshotInput = {
  restoring: false, isDemo: false, activeRoot: '/p/a', live: [session('/p/a', ['/p/a/X.java'])],
  unopened: [], wantedRoot: '/p/a', canon,
};

describe('composeSnapshot', () => {
  it('is null while a restore is running, whatever else holds', () => {
    expect(composeSnapshot({ ...base, restoring: true })).toBeNull();
    expect(composeSnapshot({ ...base, restoring: true, activeRoot: undefined, live: [] })).toBeNull();
  });

  it('is the open members, then the ones that did not open', () => {
    const gone = session('/p/b', ['/p/b/Y.java']);
    const snap = composeSnapshot({ ...base, unopened: [gone] })!;
    expect(snap.active_project).toBe('/p/a');
    expect(snap.projects.map((p) => p.root)).toEqual(['/p/a', '/p/b']);
    expect(snap.projects[1].open_files).toEqual(['/p/b/Y.java']);
  });

  it('keeps the stored members when nothing opened — a failed restore is not an empty session', () => {
    const a = session('/p/a', ['/p/a/X.java']);
    const snap = composeSnapshot({ ...base, activeRoot: undefined, live: [], unopened: [a] })!;
    expect(snap.projects).toEqual([a]);
    expect(snap.active_project).toBe('/p/a');
  });

  it('is empty only when the user really closed everything, or in the demo', () => {
    expect(composeSnapshot({ ...base, activeRoot: undefined, live: [] })).toEqual({ active_project: '', projects: [] });
    expect(composeSnapshot({ ...base, isDemo: true })).toEqual({ active_project: '', projects: [] });
  });

  it('reads the wanted project across slash styles', () => {
    const a = session('C:\\p\\a');
    const snap = composeSnapshot({ ...base, activeRoot: undefined, live: [], unopened: [a], wantedRoot: 'C:/p/a' })!;
    expect(snap.active_project).toBe('C:/p/a');
  });
});

describe('createWriteQueue', () => {
  it('never has two writes in flight and snapshots when each one starts', async () => {
    let state = 'v1';
    let inFlight = 0;
    let maxInFlight = 0;
    const written: string[] = [];
    const queue = createWriteQueue(async () => {
      const snapshot = state; // taken when this write starts
      inFlight += 1;
      maxInFlight = Math.max(maxInFlight, inFlight);
      await new Promise((r) => setTimeout(r, 5));
      written.push(snapshot);
      inFlight -= 1;
    });
    const first = queue();
    state = 'v2';
    const second = queue();
    state = 'v3';
    const third = queue();
    await Promise.all([first, second, third]);
    expect(maxInFlight).toBe(1);
    // Each write read the state when it started, so the last one on disk is the newest — which is what
    // out-of-order writes used to break.
    expect(written.at(-1)).toBe('v3');
  });

  it('keeps going after a write failed', async () => {
    let calls = 0;
    const queue = createWriteQueue(async () => {
      calls += 1;
      if (calls === 1) throw new Error('backend away');
    });
    await queue().catch(() => {});
    await queue();
    expect(calls).toBe(2);
  });
});

describe('shouldRememberCaret', () => {
  it('ignores the 1:1 a loading buffer reports until the restore has run', () => {
    expect(shouldRememberCaret(false, null, '/a')).toBe(false);
    expect(shouldRememberCaret(undefined, '/other', '/a')).toBe(false);
  });

  it('remembers a user move, and anything after the restore handled the tab', () => {
    expect(shouldRememberCaret(true, null, '/a')).toBe(true);
    expect(shouldRememberCaret(false, '/a', '/a')).toBe(true);
  });
});

describe('inStoredOrder', () => {
  it('restores the saved order and puts unknown roots last', () => {
    expect(inStoredOrder(['/c', '/a', '/new'], ['/a', '/b', '/c'])).toEqual(['/a', '/c', '/new']);
  });
});
