<script lang="ts">
  /**
   * The things the reader wants to say about a repository, as a list.
   *
   * Opening a project is not a yes/no operation: a folder whose role could not be
   * decided from its name, a folder whose engine is a guess, an object indexed
   * outside every classified folder — each is a **question to the user**, and burying it
   * would make Picus quietly wrong about the one thing it exists to be right
   * about. So they get a place on screen, sorted with the questions first.
   *
   * One component for all four lists (inferences, unresolved problems, orphans,
   * refused suppressions) because they differ only in wording, and four ad-hoc
   * markups would drift apart by the second one.
   */
  import { CircleAlert, Info, FileCode2 } from 'lucide-svelte';
  import type { ProjectNote } from '$lib/ipc/picus/scripts';

  interface Props {
    notes: ProjectNote[];
    /** Section heading. Omitted for a bare list. */
    label?: string;
    /** Called with a project-relative path when a row names one and is clicked. */
    onOpen?: (path: string) => void;
  }

  let { notes, label, onOpen }: Props = $props();

  // Questions first: an unresolved one costs the user something, an inference is
  // only worth knowing.
  const ordered = $derived(
    [...notes].sort((a, b) => Number(b.needsAttention) - Number(a.needsAttention)),
  );
</script>

{#if ordered.length}
  <div class="nl">
    {#if label}
      <div class="nl-head">
        <span>{label}</span>
        <span class="nl-count">{ordered.length}</span>
      </div>
    {/if}
    {#each ordered as note, i (`${note.path}:${i}`)}
      <div class="nl-row" class:nl-ask={note.needsAttention}>
        <span class="nl-icon">
          {#if note.needsAttention}<CircleAlert size={12} />{:else}<Info size={12} />{/if}
        </span>
        <div class="nl-body">
          <span class="nl-msg">{note.message}</span>
          {#if note.path}
            {#if onOpen}
              <button class="nl-path" onclick={() => onOpen(note.path)}>
                <FileCode2 size={10} />
                {note.path}
              </button>
            {:else}
              <span class="nl-path nl-static">{note.path}</span>
            {/if}
          {/if}
        </div>
      </div>
    {/each}
  </div>
{/if}

<style>
  .nl { display: flex; flex-direction: column; }

  .nl-head {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 7px 12px 4px;
    font-size: var(--font-size-2xs);
    font-weight: 600;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--text-muted);
  }
  .nl-count { color: var(--text-disabled); letter-spacing: 0; }

  .nl-row {
    display: flex;
    align-items: flex-start;
    gap: 7px;
    padding: 5px 12px;
    font-size: var(--font-size-xs);
    line-height: 1.5;
    color: var(--text-secondary);
  }
  .nl-icon { display: inline-flex; padding-top: 2px; flex-shrink: 0; color: var(--text-disabled); }
  /* An unanswered question is not decoration — it carries the warning colour. */
  .nl-ask .nl-icon { color: var(--warning); }
  .nl-ask .nl-msg { color: var(--text-primary); }

  .nl-body { display: flex; flex-direction: column; gap: 1px; min-width: 0; }
  .nl-msg { overflow-wrap: anywhere; }

  .nl-path {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    align-self: flex-start;
    padding: 0;
    background: none;
    border: none;
    color: var(--text-disabled);
    font-family: var(--font-code);
    font-size: var(--font-size-2xs);
    cursor: pointer;
    text-align: left;
    overflow-wrap: anywhere;
  }
  .nl-path.nl-static { cursor: default; }
  button.nl-path:hover { color: var(--accent); text-decoration: underline; text-underline-offset: 2px; }
</style>
