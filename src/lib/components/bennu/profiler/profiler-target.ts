/**
 * Whether the selected run configuration can be profiled, and if not, why. Pure, so it is tested.
 *
 * The profiler button is enabled only when there is something to record: a JVM application under
 * Flight Recorder, or a Cargo binary under samply. A script, a JUnit run or a `cargo check` is not a
 * program to look inside, and the button says so on hover rather than just going grey.
 */

export interface ProfileTarget {
  ok: boolean;
  /** What the recording would use; `null` when it cannot. */
  how: 'jfr' | 'samply' | null;
  /** Why not (when `ok` is false), or what will happen (when it is true) — shown as the tooltip. */
  note: string;
}

/** The parts of a run configuration this depends on — a structural subset, so nothing here imports the store. */
export interface ConfigFacts {
  kind: string;
  name: string;
  cargoCommand: string;
}

/** Flight Recorder's availability for the project's JDK: `null` while it has not been checked. */
export interface JfrFacts {
  available: boolean | null;
  reason: string;
}

const KIND_LABELS: Record<string, string> = {
  application: 'Application',
  springboot: 'Spring Boot',
  junit: 'JUnit',
  cargo: 'Cargo',
  script: 'Script',
};

export function profileTargetOf(cfg: ConfigFacts | null, jfr: JfrFacts): ProfileTarget {
  if (!cfg) return { ok: false, how: null, note: 'Select a run configuration to profile.' };
  if (cfg.kind === 'application' || cfg.kind === 'springboot') {
    if (jfr.available === false) {
      return { ok: false, how: null, note: jfr.reason || 'Flight Recorder is not available for this JDK.' };
    }
    return { ok: true, how: 'jfr', note: `Profile “${cfg.name}” with Flight Recorder` };
  }
  if (cfg.kind === 'cargo') {
    const command = cfg.cargoCommand.trim() || 'check';
    if (command !== 'run') {
      return { ok: false, how: null, note: `Profiling needs a Cargo configuration that runs a binary (cargo run), not cargo ${command}.` };
    }
    return { ok: true, how: 'samply', note: `Profile “${cfg.name}” with samply` };
  }
  const label = KIND_LABELS[cfg.kind] ?? cfg.kind;
  return { ok: false, how: null, note: `A ${label} configuration cannot be profiled — only a JVM application or a Cargo binary can.` };
}
