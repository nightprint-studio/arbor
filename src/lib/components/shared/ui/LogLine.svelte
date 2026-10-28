<script lang="ts">
  /**
   * One interpreted line of log output.
   *
   * Renders the **content** of a line — inline, as a `<span>` — so the consumer keeps the
   * row: its own padding, its own stream colour, its own gutter. What this owns is the
   * inside: the pieces, their colours, and the fact that some of them are clickable.
   *
   * The interpretation happens in the backend (`arbor-logscan`), which is why this is
   * presentation and nothing else: it does not know what a stack frame is, only that a piece
   * carries a {@link LogLink} and that clicking it should call `onopen`. A consumer that
   * passes no `onopen` gets the same colours with nothing clickable, which is the right
   * rendering for a transcript nobody can act on.
   *
   * With no `pieces` it renders `text`, so a line the backend never interpreted (one the
   * frontend wrote itself, say) still shows.
   *
   * The markup is deliberately written without whitespace between the pieces: a log row is
   * `white-space: pre-wrap`, and a newline in the template would arrive on the page as a
   * space the program never printed.
   */
  import type { LogLevel, LogLink, LogPiece } from '$lib/types/log';
  import { tooltip } from '$lib/actions/tooltip';
  import { findSql, sqlTokens } from '$lib/utils/sql-tokens';

  interface Props {
    /** The whole line — the fallback when there are no pieces, and what a copy should see. */
    text: string;
    /** The interpreted pieces, in order. Their `text` concatenates back to `text`. */
    pieces?: LogPiece[];
    /** The line's severity. Colours the level word — a piece does not carry its own. */
    level?: LogLevel | null;
    /** What clicking a linked piece means. Omit for a read-only transcript. */
    onopen?: (link: LogLink) => void;
    /** A SQL statement in the line (a Hibernate `show_sql`, a MyBatis `Preparing:`) is always
     *  coloured; with this, pressing it hands the whole statement to the host — to copy it, say. */
    onsql?: (sql: string) => void;
  }

  let { text, pieces, level = null, onopen, onsql }: Props = $props();

  /** One run of the line: a piece (or part of one), and whether it lies inside the SQL. */
  interface Seg { text: string; piece: LogPiece | null; sql: boolean }

  const sqlRange = $derived(findSql(text));
  const sqlText = $derived(sqlRange ? text.slice(sqlRange.start, sqlRange.end) : '');

  /** The line as runs: its pieces, each cut at the edges of the SQL it overlaps. A piece that is a link
   *  stays whole — a path inside a statement is still a path. */
  const segs = $derived.by((): Seg[] => {
    const parts: { text: string; piece: LogPiece | null }[] =
      pieces && pieces.length ? pieces.map((p) => ({ text: p.text, piece: p })) : [{ text, piece: null }];
    if (!sqlRange) return parts.map((p) => ({ ...p, sql: false }));
    const out: Seg[] = [];
    let at = 0;
    for (const part of parts) {
      const from = at;
      const to = at + part.text.length;
      at = to;
      if (part.piece?.link || to <= sqlRange.start || from >= sqlRange.end) {
        out.push({ ...part, sql: false });
        continue;
      }
      const cutA = Math.max(from, sqlRange.start) - from;
      const cutB = Math.min(to, sqlRange.end) - from;
      if (cutA > 0) out.push({ text: part.text.slice(0, cutA), piece: part.piece, sql: false });
      out.push({ text: part.text.slice(cutA, cutB), piece: part.piece, sql: true });
      if (cutB < part.text.length) out.push({ text: part.text.slice(cutB), piece: part.piece, sql: false });
    }
    return out;
  });

  /** Whether this piece is something to click: it points somewhere AND someone is listening. */
  function isLink(p: LogPiece | null): boolean {
    return !!p?.link && !!onopen;
  }

  /** The tooltip of a link — where it goes, since the text on the page rarely says. */
  function hint(link?: LogLink): string {
    if (!link) return '';
    if (link.kind === 'url') return link.url;
    if (link.kind === 'file') {
      if (link.line) return `${link.path}:${link.line}`;
      return link.method ? `${link.path} — ${link.method}()` : link.path;
    }
    const member = link.method ? `${link.class}.${link.method}` : link.class;
    return link.line ? `${member} — line ${link.line}` : member;
  }

  function open(link?: LogLink) {
    if (link) onopen?.(link);
  }
</script>

<span class="logln" data-level={level ?? undefined}
>{#each segs as s, i (i)}{#if s.sql}{#if onsql}<button
        type="button"
        class="lg lg-sql"
        data-token={s.piece?.token ?? 'text'}
        use:tooltip={'Click to copy the SQL'}
        onclick={() => onsql?.(sqlText)}
      >{#each sqlTokens(s.text) as t, j (j)}<span class="sq" data-sql={t.kind}>{t.text}</span>{/each}</button>{:else}<span
        class="lg"
        data-token={s.piece?.token ?? 'text'}
      >{#each sqlTokens(s.text) as t, j (j)}<span class="sq" data-sql={t.kind}>{t.text}</span>{/each}</span>{/if}{:else if s.piece && isLink(s.piece)}<button
        type="button"
        class="lg lg-link"
        data-token={s.piece.token ?? 'text'}
        data-colour={s.piece.colour ?? undefined}
        data-bold={s.piece.bold ? '' : undefined}
        data-lib={s.piece.link?.kind === 'source' ? '' : undefined}
        use:tooltip={hint(s.piece.link)}
        onclick={() => open(s.piece?.link)}
      >{s.text}</button>{:else}<span
        class="lg"
        data-token={s.piece?.token ?? 'text'}
        data-colour={s.piece?.colour ?? undefined}
        data-bold={s.piece?.bold ? '' : undefined}
      >{s.text}</span>{/if}{/each}</span
>

<style>
  /* Inline by design — the row belongs to the consumer, including its `white-space`. */
  .logln { display: inline; }

  /* ── what a piece IS ──────────────────────────────────────────────────────────
     Muted for the parts that are the same on every line (the timestamp, the thread,
     the logger): they are how you find the message, not the message. */
  .lg[data-token='timestamp'] { color: var(--text-disabled); }
  .lg[data-token='thread'] { color: var(--text-muted); }
  .lg[data-token='package'] { color: var(--text-muted); }
  /* The class a layout's logger column names: the one column that says WHO is talking, so it
     reads as a name rather than as chrome. MDC values are context for the line, and recede. */
  .lg[data-token='logger'] { color: var(--accent); }
  /* An MDC block is context for the line, so it is the one place two hues pair up: the key
     recedes, the value it names stands out — `tenant=` quiet, `acme` readable. */
  .lg[data-token='mdckey'] { color: var(--text-muted); }
  .lg[data-token='mdc'] { color: var(--success); }
  .lg[data-token='exception'] { color: var(--error); font-weight: 600; }
  .lg[data-token='level'] { font-weight: 700; }
  .lg[data-token='url'],
  .lg[data-token='path'],
  .lg[data-token='frame'] { color: var(--info); }

  /* Clickable things look clickable, and only on hover — a console with a hundred
     permanently underlined frames is a console you cannot read. */
  .lg-link {
    /* `inline`, and inheriting the row's wrapping: an inline-block button would refuse to
       break a long path and push the line out of the panel. */
    display: inline;
    padding: 0;
    background: none;
    border: none;
    font: inherit;
    white-space: inherit;
    word-break: inherit;
    color: var(--info);
    cursor: pointer;
    text-decoration: none;
  }
  .lg-link:hover,
  .lg-link:focus-visible { text-decoration: underline; color: var(--accent); }

  /* A frame in someone else's code — still openable, but quieter. A Spring stack trace is
     forty lines of framework around three lines of yours, and telling those apart at a
     glance is worth more than the click. */
  .lg-link[data-lib] { color: var(--text-muted); }
  .lg-link[data-lib]:hover,
  .lg-link[data-lib]:focus-visible { color: var(--accent); }

  /* ── the level word, coloured by the line's severity ────────────────────────── */
  .logln[data-level='trace'] .lg[data-token='level'],
  .logln[data-level='debug'] .lg[data-token='level'] { color: var(--text-muted); }
  .logln[data-level='info'] .lg[data-token='level'] { color: var(--info); }
  .logln[data-level='warn'] .lg[data-token='level'] { color: var(--warning); }
  .logln[data-level='error'] .lg[data-token='level'],
  .logln[data-level='fatal'] .lg[data-token='level'] { color: var(--error); }

  /* ── what the PROGRAM asked for (ANSI SGR) ───────────────────────────────────
     After the token rules, so a colour the program chose wins over one we inferred: it
     knows something about its own output that a scanner does not. The theme's own hues,
     so a coloured log sits inside the app rather than beside it. */
  .lg[data-colour='black'] { color: var(--text-disabled); }
  .lg[data-colour='red'] { color: var(--error); }
  .lg[data-colour='green'] { color: var(--success); }
  .lg[data-colour='yellow'] { color: var(--warning); }
  .lg[data-colour='blue'] { color: var(--info); }
  .lg[data-colour='magenta'] { color: var(--accent); }
  .lg[data-colour='cyan'] { color: var(--info); }
  .lg[data-colour='white'] { color: var(--text-primary); }
  .lg[data-bold] { font-weight: 700; }

  /* ── SQL inside a line ───────────────────────────────────────────────────────
     Keywords, literals and placeholders get the editor's own colours, so a statement in a log reads
     like the one in the source. A statement you can press looks pressable only on hover. */
  .sq[data-sql='keyword'] { color: var(--syntax-keyword, #cc7832); font-weight: 600; }
  .sq[data-sql='string'] { color: var(--syntax-string, #6a8759); }
  .sq[data-sql='number'] { color: var(--syntax-number, #6897bb); }
  .sq[data-sql='comment'] { color: var(--syntax-comment, #808080); font-style: italic; }
  .sq[data-sql='param'] { color: var(--syntax-field, #9876aa); font-weight: 600; }
  .lg-sql {
    display: inline; padding: 0; background: none; border: none; font: inherit; color: inherit;
    white-space: inherit; word-break: inherit; cursor: copy; text-align: left;
    border-radius: 3px; transition: background var(--transition-fast);
  }
  .lg-sql:hover, .lg-sql:focus-visible { background: color-mix(in srgb, var(--accent) 12%, transparent); }
</style>
