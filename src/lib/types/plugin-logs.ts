export type PluginLogLevel = 'debug' | 'info' | 'warn' | 'error';

export interface PluginLogEntry {
  /** Monotonic sequence number assigned by the backend ring buffer. */
  seq:     number;
  /** Wall-clock unix-ms timestamp. */
  ts_ms:   number;
  level:   PluginLogLevel;
  plugin:  string;
  message: string;
  /** Pipeline display name when this entry was mirrored from a pipeline
   *  step's captured output. Absent for plain `arbor.log.*` calls.
   *  Drives the panel's "filter by pipeline" / "clear pipeline logs"
   *  affordances. */
  pipeline?: string;
  /** Pipeline run id when applicable (same gating as `pipeline`). */
  run_id?:   string;
}
