<script lang="ts">
  /**
   * The rows behind one coverage number.
   *
   * The matrix says `3`, and the only useful next thought is *which three*. This
   * is the answer: every place the object is named under that column, in file
   * then line order, each one a jump into the editor.
   *
   * Fetched when it is opened and not before — there is a row per *mention* here
   * where the matrix has one per object, which in a real repository is one or two
   * orders of magnitude more. The Inventory tab must not pay for a drill-down
   * nobody has asked for.
   */
  import { ArrowRight, FileCode2, Pencil, PlusCircle } from 'lucide-svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import StateBlock from '$lib/components/shared/ui/StateBlock.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { objectUsages, type ObjectUsage } from '$lib/ipc/picus/scripts';
  import { picusProjectStore } from '$lib/stores/picus/project.svelte';
  import { picusTabsStore } from '$lib/stores/picus/tabs.svelte';
  import type { ObjectKind } from '$lib/types/picus';

  interface Props {
    name: string;
    kind: ObjectKind;
    /** Restrict to one coverage column's folders. Omitted, the whole repository. */
    folders?: string[];
    /** What the header calls the column this drills into. */
    label: string;
  }

  let { name, kind, folders, label }: Props = $props();

  let usages = $state<ObjectUsage[]>([]);
  let loading = $state(false);
  let error = $state('');

  /**
   * Load, once per (object, column).
   *
   * A column folds several folders — `AGGIORNAMENTO/2024/ORA` and
   * `.../2025/ORA` are one Oracle-update column — so it is one call per folder,
   * gathered. Guarded by a token because a user clicking down a column fires
   * several of these and the answers do not necessarily come back in order.
   */
  let token = 0;
  $effect(() => {
    const mine = ++token;
    const root = picusProjectStore.root;
    const wanted = folders;
    if (!root) return;

    loading = true;
    error = '';
    const ask = wanted?.length
      ? Promise.all(wanted.map((f) => objectUsages(root, kind, name, f))).then((all) => all.flat())
      : objectUsages(root, kind, name);

    void ask
      .then((rows) => {
        if (mine !== token) return;
        usages = rows.sort((a, b) => a.path.localeCompare(b.path) || a.line - b.line);
      })
      .catch((e) => {
        if (mine === token) error = String(e);
      })
      .finally(() => {
        if (mine === token) loading = false;
      });
  });

  function open(usage: ObjectUsage) {
    picusTabsStore.openFile(
      usage.path,
      usage.path.split('/').pop() ?? usage.path,
      picusProjectStore.dialectOfFile(usage.path),
      usage.line,
    );
  }
</script>

<div class="iu">
  <div class="iu-head">
    <span class="iu-title">{name} in {label}</span>
    {#if !loading && !error}
      <span class="iu-count">{usages.length} mention{usages.length === 1 ? '' : 's'}</span>
    {/if}
  </div>

  {#if loading}
    <div class="iu-loading"><Spinner size={12} /> <span>Reading the scripts…</span></div>
  {:else if error}
    <StateBlock tone="error" fill={false} label={error} />
  {:else if !usages.length}
    <StateBlock
      tone="info"
      fill={false}
      label={`Nothing under ${label} names ${name}. That is what the zero in the matrix means.`}
    />
  {:else}
    <ul class="iu-list">
      {#each usages as usage (usage.path + ':' + usage.line + ':' + usage.defining)}
        <li>
          <button class="iu-row" onclick={() => open(usage)}>
            <!-- Creating, altering, or merely using: three different facts, and
                 the one people are looking for is almost always the first. -->
            {#if usage.creating}
              <span class="iu-mark iu-create" use:tooltip={'Creates the object'}>
                <PlusCircle size={12} />
              </span>
            {:else if usage.defining}
              <span class="iu-mark iu-alter" use:tooltip={'Alters the object'}>
                <Pencil size={12} />
              </span>
            {:else}
              <span class="iu-mark iu-use" use:tooltip={`Used by a ${usage.statement} statement`}>
                <FileCode2 size={12} />
              </span>
            {/if}
            <span class="iu-path">{usage.path}</span>
            <span class="iu-line">:{usage.line}</span>
            <span class="iu-kind">{usage.statement}</span>
            <ArrowRight size={11} />
          </button>
        </li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  .iu { display: flex; flex-direction: column; gap: 6px; }

  .iu-head { display: flex; align-items: baseline; gap: 8px; }
  .iu-title { font-size: var(--font-size-xs); font-weight: 600; }
  .iu-count { font-size: var(--font-size-2xs); color: var(--text-muted); }

  .iu-loading {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: var(--font-size-xs);
    color: var(--text-muted);
  }

  .iu-list { display: flex; flex-direction: column; list-style: none; margin: 0; padding: 0; }

  .iu-row {
    display: flex;
    align-items: center;
    gap: 7px;
    width: 100%;
    padding: 3px 6px;
    background: none;
    border: none;
    border-radius: var(--radius-sm);
    color: var(--text-secondary);
    font-family: var(--font-code);
    font-size: var(--font-size-xs);
    text-align: left;
    cursor: pointer;
  }
  .iu-row:hover { background: var(--bg-hover); color: var(--text-primary); }
  .iu-row :global(svg) { flex-shrink: 0; }

  .iu-mark { display: inline-flex; }
  .iu-create { color: var(--success); }
  .iu-alter { color: var(--warning); }
  .iu-use { color: var(--text-disabled); }

  .iu-path { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .iu-line { color: var(--text-disabled); flex-shrink: 0; }
  .iu-kind {
    margin-left: auto;
    flex-shrink: 0;
    font-family: var(--font-ui-sans);
    font-size: var(--font-size-2xs);
    color: var(--text-disabled);
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }
</style>
