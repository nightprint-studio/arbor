import { invoke } from '@tauri-apps/api/core';

/** Mirrors `arbor_scheduler::ScheduleKey` — namespace ("plugin", "marketplace", …) +
 *  consumer-local name. The two-part split lets the modal group rows by subsystem. */
export interface ScheduleKey {
  namespace: string;
  name:      string;
}

/** Mirrors `arbor_scheduler::Trigger` — discriminated by `kind`. Durations are
 *  serialized as `{ secs, nanos }` by serde's `Duration` impl. */
export type Trigger =
  | { kind: 'fixed_rate';  interval: { secs: number; nanos: number } }
  | { kind: 'fixed_delay'; delay:    { secs: number; nanos: number } }
  | { kind: 'cron';        expr:     string };

/** Mirrors `arbor_scheduler::ScheduleSnapshot`. */
export interface ScheduleSnapshot {
  key:                 ScheduleKey;
  trigger:             Trigger;
  enabled:             boolean;
  fire_on_load:        boolean;
  only_when_focused:   boolean;
}

/** Snapshot every schedule currently registered against the shared engine.
 *  Returns `[]` if the scheduler isn't installed yet (boot window). */
export function listSchedules(): Promise<ScheduleSnapshot[]> {
  return invoke('list_schedules');
}
