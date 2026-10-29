/**
 * The build queue — numbered tickets for every build, test run and validation of a project.
 *
 * Thin wrappers over the `bennu_build_queue*` handlers plus the one push topic, which carries the whole
 * list on every change (so the panel never reconciles deltas).
 */

import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { bennu } from '../rpc';

export type BuildQueueKind = 'build' | 'test' | 'check';
export type BuildQueueState = 'running' | 'waiting' | 'riding';

export interface BuildQueueOwner {
  /** Empty for the user. */
  conversation: string;
  title: string;
}

export interface BuildQueueRow {
  ticket: number;
  target: string;
  kind: BuildQueueKind;
  label: string;
  owner: BuildQueueOwner;
  state: BuildQueueState;
  /** 1 = the run in progress (or the one this ticket rides), 2 = next, per target. */
  position: number;
  behind: { owner: BuildQueueOwner; kind: BuildQueueKind; label: string } | null;
  waited_ms: number;
}

const TOPIC = 'arbor://bennu/build-queue';

export const buildQueue = (): Promise<{ rows: BuildQueueRow[] }> => bennu('bennu_build_queue', {});

/** Withdraw a waiting ticket; false when it is the run in progress or already gone. */
export const buildQueueCancel = (ticket: number): Promise<boolean> => bennu('bennu_build_queue_cancel', { ticket });

export const onBuildQueueChanged = (cb: (rows: BuildQueueRow[]) => void): Promise<UnlistenFn> =>
  listen<{ rows: BuildQueueRow[] }>(TOPIC, (e) => cb(e.payload.rows ?? []));
