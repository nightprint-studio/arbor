import { describe, expect, it } from 'vitest';
import type { ProjectSession } from '$lib/ipc/bennu/config';
import {
  composeSnapshot, createSerial, createWriteQueue, inStoredOrder, planMove, shouldRememberCaret, type SnapshotInput,
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

describe('composeSnapshot — members attached with nothing on screen', () => {
  it('keeps the live members: a project moved into an empty workspace is part of its session', () => {
    // Regression: the workspace manager moves a project into the (active, empty) workspace by
    // attaching it in the background. The snapshot used to ignore it, and the next flush overwrote
    // the workspace's stored projects with an empty list.
    const a = session('/p/a', ['/p/a/X.java']);
    const snap = composeSnapshot({ ...base, activeRoot: undefined, live: [a], wantedRoot: '' })!;
    expect(snap.projects.map((p) => p.root)).toEqual(['/p/a']);
  });
});

describe('createSerial', () => {
  it('runs operations one at a time, in call order, even when an earlier one is slower', async () => {
    const run = createSerial();
    const log: string[] = [];
    const op = (name: string, ms: number) => run(async () => {
      log.push(`start ${name}`);
      await new Promise((r) => setTimeout(r, ms));
      log.push(`end ${name}`);
    });
    await Promise.all([op('1', 20), op('2', 1), op('3', 1)]);
    expect(log).toEqual(['start 1', 'end 1', 'start 2', 'end 2', 'start 3', 'end 3']);
  });

  it('survives a failing operation and still reports it to its own caller', async () => {
    const run = createSerial();
    const bad = run(async () => { throw new Error('boom'); });
    const good = run(async () => 7);
    await expect(bad).rejects.toThrow('boom');
    await expect(good).resolves.toBe(7);
  });
});

describe('planMove', () => {
  const ws = (id: string, ...roots: string[]) => ({ id, active_project: roots[0] ?? '', projects: roots.map((r) => session(r)) });
  const roots = (list: ReturnType<typeof ws>[], id: string) => list.find((w) => w.id === id)!.projects.map((p) => p.root);

  it('moves one project into another workspace, in the chosen gap', () => {
    const out = planMove([ws('a', '/1', '/2'), ws('b', '/9')], 'a', '/1', 'b', 0);
    expect(roots(out, 'a')).toEqual(['/2']);
    expect(roots(out, 'b')).toEqual(['/1', '/9']);
    expect(out.find((w) => w.id === 'a')!.active_project).toBe('/2');
  });

  it('three successive moves into a new workspace lose nothing, whatever the order', () => {
    const orders = [['/1', '/2', '/3'], ['/3', '/1', '/2'], ['/2', '/3', '/1']];
    for (const order of orders) {
      let list = [ws('a', '/1', '/2', '/3'), ws('new')];
      for (const r of order) list = planMove(list, 'a', r, 'new', 99);
      expect(roots(list, 'new')).toEqual(order);
      expect(roots(list, 'a')).toEqual([]);
    }
  });

  it('never leaves a project in neither workspace: unknown ids or roots change nothing', () => {
    const list = [ws('a', '/1'), ws('b')];
    expect(planMove(list, 'a', '/1', 'ghost', 0)).toBe(list);
    expect(planMove(list, 'ghost', '/1', 'b', 0)).toBe(list);
    expect(planMove(list, 'a', '/nope', 'b', 0)).toBe(list);
  });

  it('does not duplicate a project the target already holds', () => {
    const out = planMove([ws('a', '/1'), ws('b', '/1', '/2')], 'a', '/1', 'b', 0);
    expect(roots(out, 'a')).toEqual([]);
    expect(roots(out, 'b')).toEqual(['/1', '/2']);
  });

  it('reorders inside one workspace', () => {
    const out = planMove([ws('a', '/1', '/2', '/3')], 'a', '/1', 'a', 3);
    expect(roots(out, 'a')).toEqual(['/2', '/3', '/1']);
  });

  it('does not touch its input', () => {
    const list = [ws('a', '/1'), ws('b')];
    planMove(list, 'a', '/1', 'b', 0);
    expect(roots(list, 'a')).toEqual(['/1']);
  });
});
