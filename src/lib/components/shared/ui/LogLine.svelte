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

  interface Props {
    /** The whole line — the fallback when there are no pieces, and what a copy should see. */
    text: string;
    /** The interpreted pieces, in order. Their `text` concatenates back to `text`. */
    pieces?: LogPiece[];
    /** The line's severity. Colours the level word — a piece does not carry its own. */
    level?: LogLevel | null;
    /** What clicking a linked piece means. Omit for a read-only transcript. */
    onopen?: (link: LogLink) => void;
  }

  let { text, pieces, level = null, onopen }: Props = $props();

  /** Whether this piece is something to click: it points somewhere AND someone is listening. */
  function isLink(p: LogPiece): boolean {
    return !!p.link && !!onopen;
  }

  /** The tooltip of a link — where it goes, since the text on the page rarely says. */
  function hint(link?: LogLink): string {
    if (!link) return '';
    if (link.kind === 'url') return link.url;
    if (link.kind === 'file') return link.line ? `${link.path}:${link.line}` : link.path;
    const member = link.method ? `${link.class}.${link.method}` : link.class;
    return link.line ? `${member} — line ${link.line}` : member;
  }

  function open(link?: LogLink) {
    if (link) onopen?.(link);
  }
</script>

<span class="logln" data-level={level ?? undefined}
>{#if pieces && pieces.length}{#each pieces as p, i (i)}{#if isLink(p)}<button
        type="button"
        class="lg lg-link"
        data-token={p.token ?? 'text'}
        data-colour={p.colour ?? undefined}
        data-bold={p.bold ? '' : undefined}
        data-lib={p.link?.kind === 'source' ? '' : undefined}
        use:tooltip={hint(p.link)}
        onclick={() => open(p.link)}
      >{p.text}</button>{:else}<span
        class="lg"
        data-token={p.token ?? 'text'}
        data-colour={p.colour ?? undefined}
        data-bold={p.bold ? '' : undefined}
      >{p.text}</span>{/if}{/each}{:else}{text}{/if}</span
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
</style>
