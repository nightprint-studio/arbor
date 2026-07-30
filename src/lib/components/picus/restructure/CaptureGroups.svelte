<script lang="ts">
  /**
   * The distinct values one placeholder caught — the conflict view.
   *
   * This is the reading a repository of ten thousand INSERTs actually needs, and
   * it is not a rewrite. Group by the column list and the distinct values *are*
   * the distinct column orders in use:
   *
   *     keycode, langcode, stringvalue     9 812   ← what the repository does
   *     langcode, keycode, stringvalue        11   ← what somebody did once
   *
   * Commonest first, because that ordering is the answer: the top row is the
   * convention and everything under it is a deviation. Clicking one narrows the
   * grid to it, so the eleven can be read before anybody decides whether to
   * rewrite them — which is very often the right decision to *not* take.
   *
   * With one group there is no conflict, and the panel says so instead of drawing
   * a table of one row that means nothing.
   */
  import { TriangleAlert, Check, Filter } from 'lucide-svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import { tooltip } from '$lib/actions/tooltip';

  interface Group {
    value: string;
    count: number;
    files: number;
    deviant: boolean;
  }

  interface Props {
    groups: Group[];
    /** The value currently narrowing the grid; `null` is "all of them". */
    selected: string | null;
    name: string;
    onSelect: (value: string | null) => void;
  }

  let { groups, selected, name, onSelect }: Props = $props();
</script>

{#if groups.length <= 1}
  <div class="cg-note">
    <Alert
      variant="success"
      compact
      title={`Every match writes $${name}$ the same way`}
      text={groups.length
        ? `All ${groups[0].count} of them: ${groups[0].value}`
        : 'There is nothing to compare yet.'}
    />
  </div>
{:else}
  <div class="cg">
    <p class="cg-head">
      <TriangleAlert size={12} />
      <b>{groups.length}</b> different shapes of <code>${name}$</code>. The first is what the
      repository does; the rest are deviations.
    </p>
    <ul class="cg-list">
      {#each groups as group (group.value)}
        <li>
          <button
            type="button"
            class="cg-row"
            class:cg-on={selected === group.value}
            class:cg-deviant={group.deviant}
            aria-pressed={selected === group.value}
            use:tooltip={selected === group.value
              ? 'Showing only these — click to show every match again'
              : 'Show only the matches shaped like this'}
            onclick={() => onSelect(selected === group.value ? null : group.value)}
          >
            <span class="cg-mark">
              {#if group.deviant}<TriangleAlert size={11} />{:else}<Check size={11} />{/if}
            </span>
            <code class="cg-value">{group.value || '(empty)'}</code>
            <span class="cg-spacer"></span>
            <Badge
              variant="tone"
              tone={group.deviant ? 'warning' : 'accent'}
              size="sm"
              label={`${group.count}`}
            />
            <span class="cg-files">in {group.files} file{group.files === 1 ? '' : 's'}</span>
            {#if selected === group.value}<Filter size={11} />{/if}
          </button>
        </li>
      {/each}
    </ul>
  </div>
{/if}

<style>
  .cg-note { padding: 8px 10px; }

  .cg { display: flex; flex-direction: column; gap: 4px; padding: 8px 10px; }
  .cg-head {
    display: flex;
    align-items: center;
    gap: 5px;
    font-size: var(--font-size-xs);
    color: var(--text-secondary);
  }
  .cg-head code { font-family: var(--font-code); color: var(--accent); }

  .cg-list { display: flex; flex-direction: column; gap: 2px; max-height: 180px; overflow: auto; }

  .cg-row {
    display: flex;
    align-items: center;
    gap: 7px;
    width: 100%;
    padding: 4px 8px;
    background: transparent;
    border: 1px solid transparent;
    border-radius: var(--radius-sm);
    cursor: pointer;
    text-align: left;
    color: var(--text-primary);
  }
  .cg-row:hover { background: var(--bg-hover); }
  .cg-row.cg-on { border-color: var(--accent); background: var(--bg-hover); }
  .cg-row.cg-deviant .cg-mark { color: var(--warning); }
  .cg-row .cg-mark { display: flex; color: var(--success); flex-shrink: 0; }

  .cg-value {
    font-family: var(--font-code);
    font-size: var(--font-size-xs);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    min-width: 0;
  }
  .cg-spacer { flex: 1; }
  .cg-files { font-size: var(--font-size-2xs); color: var(--text-muted); white-space: nowrap; }
</style>
