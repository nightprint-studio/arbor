/**
 * The console's clean view, as a pure function of the buffer.
 *
 * ## It only changes what is drawn
 *
 * `buildView` reads the interpreted lines and answers "which rows, showing what". The buffer is
 * never touched: copy, save, the assistant's "Fix with Imber" and a select-all all still see the
 * text the program wrote. That is also why every transformation here works on **pieces** (the
 * backend's `arbor-logscan` already cut each line into timestamp / level / thread / logger / …)
 * and never re-parses text — the layout of a log line is the backend's knowledge, not ours.
 *
 * ## What it does
 *
 * - **Columns**: drop or shorten the timestamp, level word, thread and logger.
 * - **Rules**: hide, dim or highlight lines by logger, thread, level or message.
 * - **Stack traces**: fold the frames that are not the user's code into one row; the exception
 *   head, `Caused by:` and the user's own frames stay. With no known "own" package nothing is
 *   folded — folding everything because we could not tell would hide the one frame that matters.
 * - **Repeats**: identical consecutive lines become one with a count.
 * - **Filter**: lowest level and a text query. Both also apply when the clean view is off.
 */

import type { LogLevel, LogPiece } from '$lib/types/log';

export type TimeMode = 'time' | 'hide';
export type LevelMode = 'text' | 'letter' | 'hide';
export type ThreadMode = 'hide' | 'short' | 'full';
export type LoggerMode = 'abbr' | 'last' | 'full' | 'hide';
export type MdcMode = 'show' | 'hide';
export type RuleField = 'logger' | 'message' | 'thread' | 'level';
export type RuleOp = 'starts' | 'contains' | 'equals' | 'regex';
export type RuleAction = 'hide' | 'dim' | 'highlight';

/** Mirrors the backend's `LogViewRule`. */
export interface LogViewRule {
  on: boolean;
  field: RuleField;
  op: RuleOp;
  value: string;
  action: RuleAction;
  color: string;
}

/** Mirrors the backend's `LogViewConfig`. */
export interface LogViewSettings {
  /** The clean view is on. Off draws every line as the program wrote it. */
  clean: boolean;
  time: TimeMode;
  level: LevelMode;
  thread: ThreadMode;
  logger: LoggerMode;
  mdc: MdcMode;
  /** MDC keys never drawn, whatever `mdc` says. */
  hidden_mdc: string[];
  min_level: LogLevel;
  dedupe: boolean;
  fold_frames: boolean;
  /** Packages that count as the user's code, on top of the ones read from the project's sources. */
  extra_own_packages: string[];
  rules: LogViewRule[];
}

export const DEFAULT_LOG_VIEW: LogViewSettings = {
  clean: false,
  time: 'time',
  level: 'text',
  thread: 'hide',
  logger: 'abbr',
  mdc: 'show',
  hidden_mdc: [],
  min_level: 'trace',
  dedupe: true,
  fold_frames: true,
  extra_own_packages: [],
  rules: [],
};

/** What the view needs of a line — a structural subset of the run store's `RunLogLine`. */
export interface ViewLine {
  text: string;
  stream: 'out' | 'err' | 'meta' | 'in';
  level?: LogLevel | null;
  pieces?: LogPiece[];
}

export const LEVELS: LogLevel[] = ['trace', 'debug', 'info', 'warn', 'error', 'fatal'];
const rank = (l: LogLevel) => LEVELS.indexOf(l);

// ── fields ──────────────────────────────────────────────────────────────────────

/** The logger a line names, when the project's layout let the scanner find the column. */
export function loggerOf(line: ViewLine): string {
  return line.pieces?.find((p) => p.token === 'logger')?.text ?? '';
}

/** The thread's name without the brackets the layout wraps it in. */
export function threadOf(line: ViewLine): string {
  const t = line.pieces?.find((p) => p.token === 'thread')?.text ?? '';
  return t.replace(/^\[|\]$/g, '');
}

function fieldOf(line: ViewLine, field: RuleField): string {
  switch (field) {
    case 'logger': return loggerOf(line);
    case 'thread': return threadOf(line);
    case 'level': return line.level ?? '';
    default: return line.text;
  }
}

const regexCache = new Map<string, RegExp | null>();
function regexOf(source: string): RegExp | null {
  if (!regexCache.has(source)) {
    try { regexCache.set(source, new RegExp(source)); } catch { regexCache.set(source, null); }
  }
  return regexCache.get(source) ?? null;
}

export function ruleMatches(rule: LogViewRule, line: ViewLine): boolean {
  if (!rule.on || !rule.value) return false;
  const v = fieldOf(line, rule.field);
  if (!v) return false;
  switch (rule.op) {
    case 'starts': return v.startsWith(rule.value);
    case 'contains': return v.includes(rule.value);
    case 'equals': return v === rule.value;
    default: return regexOf(rule.value)?.test(v) ?? false;
  }
}

// ── stack frames ────────────────────────────────────────────────────────────────

const FRAME = /^\s+at\s+(?:[\w.$-]+\/)?([\w$.]+)\.([\w$<>]+)\(/;

/** The class a stack-frame line (`at com.acme.Foo.bar(Foo.java:12)`) names, or `null` for any other line. */
export function frameClass(text: string): string | null {
  return FRAME.exec(text)?.[1] ?? null;
}

export function isOwnClass(cls: string, own: readonly string[]): boolean {
  return own.some((p) => cls === p || cls.startsWith(`${p}.`));
}

// ── columns ─────────────────────────────────────────────────────────────────────

/** `com.acme.dao.FooDao` → `c.a.d.FooDao`. */
export function abbreviate(name: string): string {
  const parts = name.split('.');
  return parts.length < 2 ? name : `${parts.slice(0, -1).map((p) => p[0]).join('.')}.${parts[parts.length - 1]}`;
}

/** `[http-nio-8080-exec-3]` → `[exec-3]`: the pool's prefix is the same on every line. */
export function shortenThread(text: string): string {
  const bracketed = text.startsWith('[') && text.endsWith(']');
  const inner = bracketed ? text.slice(1, -1) : text;
  const short = inner.replace(/^[a-z]+-[a-z]+-\d+-/i, '');
  return bracketed ? `[${short}]` : short;
}

const TRIM_WS = /^\s+/;
/** A separator the layout puts after a column (`" - "`, `" : "`), so hiding the column does not leave it dangling. */
const TRIM_SEP = /^\s*(?:[-:|]\s+)?/;

const isPlainPiece = (p: LogPiece) => !p.token || p.token === 'text';
/** What a layout leaves behind once every MDC entry is gone: `| MDC:[]`, ` []`, `{}`-less equivalents. */
const EMPTY_MDC_WRAPPER = /(?:\s*[|,;-]\s*)?[A-Za-z_]*:?\s*\[\s*\]/;

/**
 * The pieces without the MDC entries the settings hide — all of them, or the named keys.
 *
 * An entry is a key, an `=` and a value, with a comma on one side: removing it takes the comma too, so
 * what is left still reads `[tenant=acme]` and not `[tenant=acme,]`. When every entry is gone the empty
 * wrapper the layout prints around them goes with it.
 */
export function hideMdc(pieces: LogPiece[], s: LogViewSettings): LogPiece[] {
  const all = s.mdc === 'hide';
  const keys = new Set(s.hidden_mdc);
  if (!all && keys.size === 0) return pieces;

  const ps = pieces.slice();
  const out: LogPiece[] = [];
  let removed = false;
  let eatComma = false;
  for (let i = 0; i < ps.length; i++) {
    const p = ps[i];
    if (p.token === 'mdckey' && (all || keys.has(p.text))) {
      removed = true;
      const eq = ps[i + 1];
      if (eq && isPlainPiece(eq) && eq.text.startsWith('=')) {
        const rest = eq.text.slice(1);
        if (rest) {
          ps[i + 1] = { ...eq, text: rest }; // an empty value: the gap carries on after the "="
        } else {
          i++; // the "="
          if (ps[i + 1]?.token === 'mdc') i++; // its value
        }
      }
      const last = out[out.length - 1];
      if (last && isPlainPiece(last) && /,\s*$/.test(last.text)) {
        const text = last.text.replace(/,\s*$/, '');
        if (text) out[out.length - 1] = { ...last, text };
        else out.pop();
      } else {
        eatComma = true;
      }
      continue;
    }
    if (all && p.token === 'mdc') { removed = true; continue; }
    if (eatComma && isPlainPiece(p)) {
      eatComma = false;
      const text = p.text.replace(/^\s*,\s*/, '');
      if (text) out.push({ ...p, text });
      continue;
    }
    eatComma = false;
    out.push(p);
  }
  if (!removed) return pieces;

  // Neighbouring plain pieces become one, so the wrapper the layout left (`MDC:[` … `]`) is a single string to clean.
  const merged: LogPiece[] = [];
  for (const p of out) {
    const last = merged[merged.length - 1];
    if (last && isPlainPiece(last) && isPlainPiece(p) && !last.link && !p.link) merged[merged.length - 1] = { ...last, text: last.text + p.text };
    else merged.push(p);
  }
  return merged
    .map((p) => (isPlainPiece(p) ? { ...p, text: p.text.replace(EMPTY_MDC_WRAPPER, '') } : p))
    .filter((p) => p.text);
}

/** The pieces of a line after the column settings: dropped, shortened, or as they were. */
export function transformPieces(source: LogPiece[], s: LogViewSettings): LogPiece[] {
  const pieces = hideMdc(source, s);
  const out: LogPiece[] = [];
  let trim: RegExp | null = null;
  for (const p of pieces) {
    if (!p.token || p.token === 'text') {
      const text = trim ? p.text.replace(trim, '') : p.text;
      trim = null;
      if (text) out.push(text === p.text ? p : { ...p, text });
      continue;
    }
    trim = null;
    const drop = (re: RegExp) => { trim = re; };
    switch (p.token) {
      case 'timestamp':
        if (s.time === 'hide') { drop(TRIM_WS); continue; }
        break;
      case 'level':
        if (s.level === 'hide') { drop(TRIM_WS); continue; }
        if (s.level === 'letter') { out.push({ ...p, text: p.text.replace(/[A-Za-z]+/, (w) => w[0].toUpperCase()) }); continue; }
        break;
      case 'thread':
        if (s.thread === 'hide') { drop(TRIM_WS); continue; }
        if (s.thread === 'short') { out.push({ ...p, text: shortenThread(p.text) }); continue; }
        break;
      case 'logger':
        if (s.logger === 'hide') { drop(TRIM_SEP); continue; }
        if (s.logger === 'abbr') { out.push({ ...p, text: abbreviate(p.text) }); continue; }
        if (s.logger === 'last') { out.push({ ...p, text: p.text.split('.').pop() ?? p.text }); continue; }
        break;
    }
    out.push(p);
  }
  return out;
}

// ── the view ────────────────────────────────────────────────────────────────────

export type ViewEntry =
  | { kind: 'line'; index: number; text: string; pieces?: LogPiece[]; count: number; dim: boolean; color?: string }
  /** A run of framework frames: closed it stands for them, open it heads them. */
  | { kind: 'fold'; key: number; count: number; open: boolean }
  /** A run of lines a rule or the filter hides. */
  | { kind: 'hidden'; key: number; count: number; open: boolean };

export interface ViewOptions {
  settings: LogViewSettings;
  /** The user's packages: detected from the project, plus the ones added by hand. */
  own: readonly string[];
  /** Case-insensitive text filter; empty shows everything. */
  query: string;
  /** Frame folds the reader opened, by the index of their first frame. */
  openFolds: ReadonlySet<number>;
  /** Hidden runs the reader peeked into, by the index of their first line. */
  openHidden: ReadonlySet<number>;
}

/** The text a marker row stands for — what the virtual list measures and a screen reader reads. */
export function entryText(e: ViewEntry): string {
  switch (e.kind) {
    case 'line': return e.text;
    case 'fold': return `${e.open ? '▾' : '▸'} ${e.count} framework frame${e.count > 1 ? 's' : ''}`;
    default: return `${e.open ? '▾' : '▸'} ${e.count} hidden line${e.count > 1 ? 's' : ''}`;
  }
}

export function buildView(lines: readonly ViewLine[], o: ViewOptions): ViewEntry[] {
  const { settings: s } = o;
  const clean = s.clean;
  const query = o.query.trim().toLowerCase();
  const minRank = rank(s.min_level);
  const foldable = clean && s.fold_frames && o.own.length > 0;

  const out: ViewEntry[] = [];
  let frames: number[] = [];
  let hidden: number[] = [];
  // `as`, not an annotation: the closures below assign it, and TypeScript would otherwise narrow it to `null` for good.
  let prev = null as Extract<ViewEntry, { kind: 'line' }> | null;

  const plain = (i: number, extra: Partial<Extract<ViewEntry, { kind: 'line' }>> = {}) => {
    const l = lines[i];
    const pieces = clean && l.pieces?.length ? transformPieces(l.pieces, s) : l.pieces;
    const text = clean && pieces && l.pieces?.length ? pieces.map((p) => p.text).join('') : l.text;
    const e: Extract<ViewEntry, { kind: 'line' }> = { kind: 'line', index: i, text, pieces, count: 1, dim: false, ...extra };
    out.push(e);
    return e;
  };
  const endRuns = () => {
    if (frames.length) {
      const key = frames[0];
      const open = o.openFolds.has(key);
      out.push({ kind: 'fold', key, count: frames.length, open });
      if (open) frames.forEach((i) => plain(i));
      frames = [];
    }
    if (hidden.length) {
      const key = hidden[0];
      const open = o.openHidden.has(key);
      out.push({ kind: 'hidden', key, count: hidden.length, open });
      if (open) hidden.forEach((i) => plain(i, { dim: true }));
      hidden = [];
    }
    prev = null;
  };

  for (let i = 0; i < lines.length; i++) {
    const l = lines[i];
    const ours = l.stream === 'meta' || l.stream === 'in';

    const filtered =
      !ours &&
      ((l.level && rank(l.level) < minRank) || (query !== '' && !l.text.toLowerCase().includes(query)));
    const ruled = clean && !ours && s.rules.some((r) => r.action === 'hide' && ruleMatches(r, l));
    if (filtered || ruled) {
      if (frames.length) endRuns();
      hidden.push(i);
      continue;
    }
    if (hidden.length) endRuns();

    const cls = foldable && !ours ? frameClass(l.text) : null;
    if (cls && !isOwnClass(cls, o.own)) {
      frames.push(i);
      continue;
    }
    if (frames.length) endRuns();

    if (clean && s.dedupe && !ours && !cls && prev && prev.text === transformedText(l, s)) {
      prev.count++;
      continue;
    }

    let dim = false;
    let color: string | undefined;
    if (clean && !ours) {
      for (const r of s.rules) {
        if (!ruleMatches(r, l)) continue;
        if (r.action === 'dim') dim = true;
        else if (r.action === 'highlight') color = r.color;
      }
    }
    prev = plain(i, { dim, color });
  }
  endRuns();
  return out;
}

function transformedText(l: ViewLine, s: LogViewSettings): string {
  return l.pieces?.length ? transformPieces(l.pieces, s).map((p) => p.text).join('') : l.text;
}

// ── rules born from a line ──────────────────────────────────────────────────────

/** A rule that hides/dims/highlights lines like `line` — the one the row's menu creates. */
export function ruleFromLine(
  line: ViewLine,
  how: 'logger' | 'package' | 'thread',
  action: RuleAction,
  color = '#d29922',
): LogViewRule | null {
  const logger = loggerOf(line);
  if (how === 'thread') {
    const thread = threadOf(line);
    return thread ? { on: true, field: 'thread', op: 'equals', value: thread, action, color } : null;
  }
  if (!logger) return null;
  const value = how === 'package' ? logger.split('.').slice(0, 3).join('.') : logger;
  return { on: true, field: 'logger', op: 'starts', value, action, color };
}

/** A short human label for a rule, as it is read on its chip. */
export function ruleLabel(r: LogViewRule): string {
  const sign = { starts: '', contains: '~ ', equals: '= ', regex: '≈ ' }[r.op];
  return `${r.field} ${sign}${r.value || '…'}${r.op === 'starts' ? '*' : ''}`;
}
