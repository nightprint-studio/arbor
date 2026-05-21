/**
 * Shared log-line syntax highlighter.
 *
 * Used by surfaces that render captured log/output lines and want a bit of
 * visual structure without pulling in a full lexer:
 *   • PluginLogsPanel — `arbor.log.*` stream (formatted as
 *     `HH:MM:SS LEVEL [plugin] message` — uses both the body highlighter
 *     here and its own time/level/plugin token wrapping).
 *   • PipelineRunDetailModal — per-step output of a pipeline run.
 *   • Anywhere else that wraps `<LogStream>` and wants the same look.
 *
 * Output is always HTML-safe: every input string is escaped first, so it's
 * safe to pass into Svelte's `{@html …}` even when the line came from
 * untrusted plugin output.
 */

/** HTML-escape a raw log line so it can be fed to `{@html …}` safely. */
export function escapeHtml(s: string): string {
  return s
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;');
}

/**
 * Highlight a message body (no time/level/plugin prefix). Tokens recognised:
 *   • Quoted strings  → `.log-tok-str`
 *   • [Bracketed tags] → `.log-tok-tag`
 *   • `key=` lvalues  → `.log-tok-key`
 *   • Numbers (int/float, signed) → `.log-tok-num`
 *
 * Order matters: strings are wrapped first so subsequent passes don't
 * accidentally match identifiers/numbers inside string literals. The
 * regexes are intentionally conservative — false positives are cheap (just
 * a stray colour) but false negatives matter more for readability.
 *
 * Token CSS lives in app.css (the `.log-tok-*` ruleset) so any consumer
 * gets the same palette without copying styles into each component.
 */
export function highlightLogBody(msg: string): string {
  const escaped = escapeHtml(msg);
  return escaped
    // key= MUST run before any rule that injects `<span class="…">`,
    // otherwise its regex would match `class=` inside our own attribute
    // markup and tear the HTML apart (the user sees literal
    // `class="log-tok-tag">[stderr]` text leaking into the rendered line).
    // Only the identifier part is wrapped; the `=` stays plain so the
    // value can pick up its own number/string colouring.
    .replace(/\b([A-Za-z_][\w-]*)=/g, '<span class="log-tok-key">$1</span>=')
    // Quoted strings (single or double — escapeHtml already turned the
    // raw quotes into &quot; / &#39; so we match the entity form).
    .replace(/(&quot;[^&]*?&quot;|&#39;[^&]*?&#39;)/g, '<span class="log-tok-str">$1</span>')
    // [tag] — keeps the brackets visible for readability.
    .replace(/\[([^\]\s][^\]]*)\]/g, '<span class="log-tok-tag">[$1]</span>')
    // Numbers — anchored after whitespace / punctuation so we don't paint
    // the digits inside identifiers like `arg1` or `v2`.
    .replace(/(^|[\s,([{=:])(-?\d+(?:\.\d+)?)\b/g, '$1<span class="log-tok-num">$2</span>');
}

// ─────────────────────────────────────────────────────────────────────────
// Structured-line helpers — `<time>  <LEVEL>  [<plugin>]  <message>`
// ─────────────────────────────────────────────────────────────────────────
//
// Used both by the global Plugin Logs panel (entries already structured by
// the backend) and by surfaces that synthesise a structured line on the
// fly (pipeline step output, where the runtime captures only raw lines and
// the level is inferred from a small set of conventions). Keeping the
// formatter here means both consumers render the same way without copying
// regexes / DOM strings into each component.

export type LogLevel = 'debug' | 'info' | 'warn' | 'error';

/** Two-digit zero-padded number for HH:MM:SS rendering. */
function pad2(n: number): string { return n < 10 ? `0${n}` : String(n); }

/** Locale-independent HH:MM:SS for a unix-ms timestamp. */
export function formatLogTime(ms: number): string {
  const d = new Date(ms);
  return `${pad2(d.getHours())}:${pad2(d.getMinutes())}:${pad2(d.getSeconds())}`;
}

/**
 * Mirror of `infer_step_log_level` in `src-tauri/src/pipeline/mod.rs`.
 * Frontend surfaces (e.g. the pipeline-step view) use this to attach a
 * level to each captured line so the structured renderer can colour it.
 * Conservative — anything we don't recognise stays at info. `[stderr]` is
 * stripped before inspection: git/cargo/npm write progress to stderr, so
 * the stream alone is not a level signal.
 */
export function inferLogLevel(line: string): LogLevel {
  const trimmed = line.replace(/^\s+/, '');
  const body    = (trimmed.startsWith('[stderr]') ? trimmed.slice(8) : trimmed).replace(/^\s+/, '');
  if (body.startsWith('⚠')
    || body.startsWith('FAIL')
    || body.startsWith('error') || body.startsWith('ERROR') || body.startsWith('Error')
    || body.startsWith('fatal:') || body.startsWith('Fatal')
    || body.startsWith('panic')) {
    return 'error';
  }
  if (body.startsWith('WARN') || body.startsWith('WARNING')
    || body.startsWith('warning:') || body.startsWith('Warning')) return 'warn';
  if (body.startsWith('DEBUG')) return 'debug';
  return 'info';
}

export interface StructuredLogLine {
  ts_ms:   number;
  level:   LogLevel | string;
  plugin:  string;
  message: string;
  /** Pipeline run id when the entry was mirrored from a pipeline step.
   *  Rendered as `#<short-id>` between the plugin tag and the message —
   *  lets the user correlate a log line to a specific run at a glance,
   *  matching the `#29` shorthand the run-detail modal header uses. */
  run_id?: string;
}

/**
 * Strip the `pipe-run-` prefix from a backend-assigned run id so the
 * panel can render the compact `#N` form. Falls back to the raw id when
 * the prefix isn't present (defensive — covers ids minted by future
 * code paths that don't use the same convention).
 */
export function shortRunId(runId: string): string {
  return runId.startsWith('pipe-run-') ? runId.slice('pipe-run-'.length) : runId;
}

/**
 * Render a structured log entry as the canonical
 *   `<time>  <LEVEL>  [<plugin>]  [#<run>]  <highlighted message>`
 * HTML string. The `#<run>` token is omitted when the entry has no
 * `run_id`, keeping plain `arbor.log.*` lines unchanged. Safe for
 * `{@html …}`: every interpolated string runs through `escapeHtml` (or
 * the body highlighter, which escapes first).
 */
export function renderStructuredLogLine(e: StructuredLogLine): string {
  const time   = `<span class="log-tok-time">${escapeHtml(formatLogTime(e.ts_ms))}</span>`;
  const lvl    = e.level.toUpperCase().padEnd(5);
  const level  = `<span class="log-tok-lvl log-tok-lvl-${e.level}">${escapeHtml(lvl)}</span>`;
  const plugin = `<span class="log-tok-plugin">[${escapeHtml(e.plugin)}]</span>`;
  const run    = e.run_id
    ? `  <span class="log-tok-run">#${escapeHtml(shortRunId(e.run_id))}</span>`
    : '';
  const msg    = highlightLogBody(e.message);
  return `${time}  ${level}  ${plugin}${run}  ${msg}`;
}
