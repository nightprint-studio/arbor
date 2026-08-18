<script lang="ts">
  /**
   * The translation's parameters — every one of them, not just this language's.
   *
   * ## Why the union across languages
   *
   * A table of the placeholders *this* value happens to use would only tell you what you can already
   * read on the line above it. The useful question is the one that spans the languages: **`en` passes
   * `{amount}` and the Italian does not mention it**. That is a real defect — the number the caller
   * interpolates is dropped on the floor and the sentence reads as though it were never meant to have
   * one — and no compiler, no test and no schema will ever say so, because every file involved is
   * valid. It is also invisible in every view that shows one language at a time.
   *
   * So the rows are the union, and the column that matters is which languages use each one.
   *
   * ## The sample values
   *
   * Typing a sample here substitutes it into the preview, which is what turns the preview from "the
   * markup, minus the markup" into "the sentence, as somebody will read it" — and long values are the
   * point: `{name}` reads fine until it is *Bartolomeo della Fortezza* and the line wraps into three.
   * They are scratch, they live in memory, and they are gone on restart. See `bennuI18nStore`.
   */
  import { Plus } from 'lucide-svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import type { StudioView } from '$lib/ipc/bennu/i18n';

  let {
    view,
    samples,
    /** Set (or clear, with `''`) one sample value. */
    onSample,
    /** Write `{name}` at the caret. */
    onInsert,
  }: {
    view: StudioView;
    samples: ReadonlyMap<string, string>;
    onSample: (param: string, value: string) => void;
    onInsert: (param: string) => void;
  } = $props();

  interface Row {
    name: string;
    /** Whether the value being edited uses it. */
    here: boolean;
    /** The other languages that use it. */
    langs: string[];
  }

  const rows: Row[] = $derived.by(() => {
    const order: string[] = [...view.params];
    for (const s of view.siblings) {
      for (const p of s.params) if (!order.includes(p)) order.push(p);
    }
    return order.map((name) => ({
      name,
      here: view.params.includes(name),
      langs: view.siblings.filter((s) => s.params.includes(name)).map((s) => s.lang),
    }));
  });
</script>

{#if rows.length === 0}
  <p class="pt-none">
    No parameters. <span class="pt-hint">A translation that interpolates nothing needs none.</span>
  </p>
{:else}
  <table class="pt">
    <thead>
      <tr>
        <th class="pt-name">Parameter</th>
        <th class="pt-langs">Used in</th>
        <th class="pt-sample">Sample value</th>
      </tr>
    </thead>
    <tbody>
      {#each rows as row (row.name)}
        <tr>
          <td class="pt-name">
            <code class="pt-code" class:absent={!row.here}>{'{'}{row.name}{'}'}</code>
          </td>
          <td class="pt-langs">
            {#if row.here}
              <span class="pt-lang here" use:tooltip={'used by the value you are editing'}
                >{view.lang}</span
              >
            {:else}
              <!-- The whole point of the table, said as plainly as it can be said. -->
              <button
                class="pt-add"
                type="button"
                use:tooltip={`${view.lang} does not use {${row.name}} — insert it at the caret`}
                onclick={() => onInsert(row.name)}
              >
                <Plus size={10} /> {view.lang}
              </button>
            {/if}
            {#each row.langs as lang (lang)}
              <span class="pt-lang">{lang}</span>
            {/each}
          </td>
          <td class="pt-sample">
            <Input
              value={samples.get(row.name) ?? ''}
              size="sm"
              placeholder={row.name}
              ariaLabel={`Sample value for ${row.name}`}
              oninput={(v) => onSample(row.name, v)}
            />
          </td>
        </tr>
      {/each}
    </tbody>
  </table>
{/if}

<style>
  .pt-none {
    margin: 0;
    padding: 8px 2px;
    font-size: var(--font-size-xs);
    color: var(--text-secondary);
  }
  .pt-hint { color: var(--text-disabled); }

  .pt {
    width: 100%;
    border-collapse: collapse;
    font-size: var(--font-size-xs);
  }
  .pt th {
    padding: 0 6px 4px 0;
    text-align: left;
    font-size: var(--font-size-3xs);
    font-weight: 500;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--text-disabled);
    border-bottom: 1px solid var(--border-subtle);
    white-space: nowrap;
  }
  .pt td {
    padding: 3px 6px 3px 0;
    vertical-align: middle;
    border-bottom: 1px solid var(--border-subtle);
  }
  .pt tr:last-child td { border-bottom: none; }

  .pt-name { width: 1%; white-space: nowrap; }
  .pt-langs { width: 1%; white-space: nowrap; }
  /* The sample field takes whatever is left: it is the only cell whose content is a sentence. */
  .pt-sample { width: auto; min-width: 120px; }

  .pt-code {
    font-family: var(--font-code);
    color: var(--info);
  }
  .pt-code.absent { color: var(--text-disabled); }

  .pt-lang {
    display: inline-block;
    margin-right: 3px;
    padding: 0 4px;
    border-radius: var(--radius-sm);
    background: var(--bg-hover);
    color: var(--text-muted);
    font-family: var(--font-code);
    font-size: var(--font-size-3xs);
  }
  .pt-lang.here {
    background: color-mix(in srgb, var(--success) 18%, transparent);
    color: var(--success);
  }

  .pt-add {
    display: inline-flex;
    align-items: center;
    gap: 2px;
    margin-right: 3px;
    padding: 0 4px;
    border: 1px dashed color-mix(in srgb, var(--warning) 55%, transparent);
    border-radius: var(--radius-sm);
    background: none;
    color: var(--warning);
    font-family: var(--font-code);
    font-size: var(--font-size-3xs);
    cursor: pointer;
  }
  .pt-add:hover { background: color-mix(in srgb, var(--warning) 14%, transparent); }
</style>
