<script lang="ts">
  /**
   * Settings › Struts &amp; JSP — the pages whose action was pinned by hand.
   *
   * A binding is made where the question comes up, on the JSP itself: a view-only page maps to
   * several actions, or to none the reverse lookup can see, and you say which one its properties are
   * checked against. What this page adds is the list — the one place a pin made months ago on a page
   * since renamed can be found and dropped.
   */
  import { Trash2, Workflow } from 'lucide-svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { bennuConfigStore } from '$lib/stores/bennu/config.svelte';
  import { projectStore } from '$lib/stores/bennu/project.svelte';

  const cfg = $derived(bennuConfigStore.cfg);
  const root = $derived(projectStore.project?.root ?? null);

  const bindings = $derived(cfg?.jsp_action_bindings ?? {});

  /** Only this project's pages: the map is keyed by absolute path and holds every project you have
   *  ever pinned one in, which is not a list anybody wants to read. */
  const mine = $derived.by(() => {
    const prefix = root ? `${root.replace(/\\/g, '/')}/` : null;
    return Object.entries(bindings)
      .filter(([file]) => !prefix || file.startsWith(prefix))
      .map(([file, action]) => ({ file, action, short: file.split('/').pop() ?? file }))
      .sort((a, b) => a.file.localeCompare(b.file));
  });

  async function unpin(file: string) {
    const next = { ...bindings };
    delete next[file];
    await bennuConfigStore.patch({ jsp_action_bindings: next });
  }
</script>

<div class="section-header">
  <h2>Struts &amp; JSP</h2>
  <p>The pages whose Struts action you pinned instead of letting the reverse lookup decide.</p>
</div>

<div class="card">
  <div class="card-section-title"><Workflow size={12} /> Pinned pages</div>
  <p class="set-hint">
    A JSP with a <code>&lt;form&gt;</code> names its own action, and that is what is used. Pinning is
    for the rest: a view-only page that reads OGNL properties and maps to several actions, or to none
    that can be found from the page. Pin one from the JSP; drop it here when the page or the action
    is gone.
  </p>
  {#if mine.length === 0}
    <p class="set-empty">Nothing pinned — every page is resolved from its forms and the reverse lookup.</p>
  {:else}
    <div class="set-list">
      {#each mine as row (row.file)}
        <div class="set-list-row">
          <span class="set-list-text" use:tooltip={row.file}>{row.short}</span>
          <span class="bound" use:tooltip={row.action}>{row.action}</span>
          <button class="set-list-del" type="button" onclick={() => void unpin(row.file)} aria-label={`Unpin ${row.short}`}>
            <Trash2 size={13} />
          </button>
        </div>
      {/each}
    </div>
  {/if}
</div>

<style>
  .bound {
    flex-shrink: 0; max-width: 45%;
    font-family: var(--font-code); font-size: var(--font-size-2xs); color: var(--text-muted);
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
</style>
