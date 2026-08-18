<script lang="ts">
  /**
   * A parsed translation, rendered.
   *
   * ## What it is for
   *
   * `'Hai trovato $good.bold{@potion{una pozione}} — {count} rimaste'` is a sentence written in a
   * markup that pushes the words apart. Reading it means holding the constructs in your head and
   * subtracting them, which is exactly the work a preview should be doing instead. Here the styles
   * are painted, the glossary term is marked as one, and what is left is the sentence — so "does this
   * read well in Italian" becomes a question you answer by looking.
   *
   * ## What it does not claim
   *
   * The engine draws the real thing. This shows the **distinctions** the stylesheet draws, at the
   * panel's scale (see `markup-style.ts`), and it says plainly where it cannot: a control is a chip
   * rather than an animation, because `~shake` is motion and a still picture pretending otherwise
   * would be worse than one that admits it.
   *
   * A placeholder with no sample value renders as its **name in a pill** — not as a guess, and not as
   * a gap: the shape of the sentence is what is being judged, and a hole in it hides a missing space.
   *
   * ## Why the markup below is jammed together
   *
   * This renders inline text under `white-space: pre-wrap`, so the translation's own spaces are the
   * sentence's spaces — and **any whitespace in the template is whitespace on screen**. A newline
   * between two segments would insert a space the translation does not have, which is the one class
   * of error a preview must not make: it would show a sentence that reads correctly while the file
   * says otherwise. So every part is computed in `parts` below and each branch renders exactly one
   * element with nothing between them. Line breaks inside a tag's attribute list are safe; line
   * breaks between tags are not.
   */
  import { tooltip } from '$lib/actions/tooltip';
  // Itself, for the recursion — a component importing itself is the runes-mode spelling of what
  // `<svelte:self>` used to do.
  import I18nMarkup from './I18nMarkup.svelte';
  import type { GlossaryDecl, Segment } from '$lib/ipc/bennu/i18n';
  import { styleAttr, type Appearance, type StyleSheet } from './markup-style';

  let {
    segments,
    sheet,
    glossary,
    samples,
    /** The appearance inherited from the enclosing style span — see `StyleSheet.chain`. */
    inherited = {},
  }: {
    segments: Segment[];
    sheet: StyleSheet;
    glossary: Map<string, GlossaryDecl>;
    samples: ReadonlyMap<string, string>;
    inherited?: Appearance;
  } = $props();

  /** The style the engine gives a glossary term when its entry does not name one. */
  const GLOSSARY_DEFAULT = 'glossary-item';

  /** One thing to draw. Flat by design — see the note on whitespace above. */
  type Part =
    /** Literal text. */
    | { tag: 'text'; text: string }
    /** A placeholder: its name, or the sample standing in for it. */
    | { tag: 'param' | 'sample'; text: string; tip: string }
    /** A construct with content: styled, recursed into. */
    | {
        tag: 'span';
        cls: string;
        style: string;
        tip: string;
        /** A leading chip — how a control says what it is without pretending to do it. */
        chip: string;
        content: Segment[];
        appearance: Appearance;
      };

  const parts: Part[] = $derived(segments.map(partOf));

  function partOf(seg: Segment): Part {
    const k = seg.kind;
    switch (k.kind) {
      case 'text':
        return { tag: 'text', text: k.text };
      case 'placeholder': {
        const name = k.name.text;
        const sample = samples.get(name);
        return sample
          ? { tag: 'sample', text: sample, tip: `{${name}}` }
          : { tag: 'param', text: name, tip: `{${name}} — no sample value yet` };
      }
      case 'style': {
        const names = k.styles.map((s) => s.text);
        const appearance = sheet.chain(names, inherited);
        return {
          tag: 'span',
          // A style the stylesheet does not declare renders as the default and loses the emphasis it
          // was written for — which is invisible unless it is said, so it is said.
          cls: names.every((n) => sheet.has(n)) ? 'mk-style' : 'mk-style unknown',
          style: styleAttr(appearance),
          tip: names.every((n) => sheet.has(n))
            ? `$${names.join('.')}`
            : `$${names.join('.')} — ${names.filter((n) => !sheet.has(n)).join(', ')} not in styles.toml`,
          chip: '',
          content: k.content,
          appearance,
        };
      }
      case 'glossary': {
        const key = k.key.text;
        const decl = glossary.get(key);
        const appearance = sheet.chain([decl?.style || GLOSSARY_DEFAULT], inherited);
        return {
          tag: 'span',
          cls: decl ? 'mk-gloss' : 'mk-gloss unknown',
          style: styleAttr(appearance),
          tip: glossaryTip(key, decl),
          chip: '',
          content: k.content,
          appearance,
        };
      }
      case 'control': {
        const call = k.args.length ? `~${k.name.text}(${k.args.join(', ')})` : `~${k.name.text}`;
        return {
          tag: 'span',
          cls: 'mk-ctrl-wrap',
          style: '',
          tip: `${call} — pacing or effect, the engine's to render`,
          chip: `~${k.name.text}`,
          content: k.content,
          appearance: inherited,
        };
      }
    }
  }

  function glossaryTip(key: string, decl: GlossaryDecl | undefined): string {
    if (!decl) return `@${key} — no glossary entry declares this`;
    const head = decl.name ? `${decl.name} (@${key})` : `@${key}`;
    return decl.description ? `${head} — ${decl.description}` : head;
  }
</script>

<!-- prettier-ignore -->
{#each parts as p, i (i)}{#if p.tag === 'text'}{p.text}{:else if p.tag === 'sample'}<span
      class="mk-sample"
      use:tooltip={p.tip}
    >{p.text}</span>{:else if p.tag === 'param'}<span
      class="mk-param"
      use:tooltip={p.tip}
    >{p.text}</span>{:else if p.tag === 'span'}<span
      class={p.cls}
      style={p.style}
      use:tooltip={p.tip}
    >{#if p.chip}<span class="mk-chip">{p.chip}</span>{/if}<I18nMarkup
        segments={p.content}
        {sheet}
        {glossary}
        {samples}
        inherited={p.appearance}
      /></span>{/if}{/each}
<style>
  /* Everything here is inline-level: the container (in the panel) sets `pre-wrap`, and these must
     not introduce boxes that would break the flow of a sentence across lines. */

  /* A placeholder with nothing standing in for it: its name, marked as not being words. */
  .mk-param {
    padding: 0 3px;
    border-radius: var(--radius-sm);
    background: color-mix(in srgb, var(--info) 16%, transparent);
    color: var(--info);
    font-family: var(--font-code);
    font-size: 0.86em;
  }
  /* A sample value IS words — it reads as part of the sentence, with just enough marking to remember
     it is standing in for something. */
  .mk-sample {
    border-bottom: 1px dotted color-mix(in srgb, var(--info) 60%, transparent);
  }

  .mk-style.unknown {
    /* No colour of its own: the point is that this span has LOST its styling, so inventing one here
       would hide exactly what is wrong. A wavy underline says "this name is not real" instead. */
    text-decoration-line: underline;
    text-decoration-style: wavy;
    text-decoration-color: var(--warning);
  }

  /* A glossary term: the entry's own style paints it, this only says it is a term. */
  .mk-gloss {
    border-bottom: 1px solid color-mix(in srgb, currentColor 40%, transparent);
  }
  .mk-gloss.unknown {
    border-bottom-style: dashed;
    border-bottom-color: var(--warning);
  }

  .mk-chip {
    margin-right: 2px;
    padding: 0 3px;
    border-radius: var(--radius-sm);
    background: var(--bg-hover);
    color: var(--text-muted);
    font-family: var(--font-code);
    font-size: 0.78em;
    vertical-align: 1px;
  }
</style>
