<script lang="ts">
  import { tooltip } from '$lib/actions/tooltip';
  /**
   * What the fix will NOT touch, and why — grouped by reason.
   *
   * A bulk fix that quietly skips things is worse than one that fixes less: the count would say "done"
   * while a dozen names stayed as they were. So the refusals are a tab of their own, grouped by the
   * kind of reason (a collision, a library override, a name an annotation writes into JSON, a package
   * that is a directory…), each with the sentence the backend gave and a way to open the declaration.
   */
  import { ExternalLink } from 'lucide-svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import FilterInput from '$lib/components/shared/ui/FilterInput.svelte';
  import VirtualList from '$lib/components/shared/ui/VirtualList.svelte';
  import type { FixRefusal } from '$lib/ipc/bennu/naming';
  import { refusedLines, type RefusedLine } from './naming-fix-selection';

  interface Props {
    refused: FixRefusal[];
    onGoTo: (file: string, line: number) => void;
  }

  let { refused, onGoTo }: Props = $props();

  const ROW_H = 30;
  let search = $state('');
  const lines = $derived(refusedLines(refused, search));

  const baseName = (path: string) => path.split(/[\\/]/).pop() ?? path;
  const lineKey = (line: RefusedLine, i: number) =>
    line.kind === 'group' ? `g:${line.key}` : `r:${i}`;
</script>

<div class="refused">
  <FilterInput bind:value={search} placeholder="Filter by name, file or reason…" ariaLabel="Filter left-alone names" />
  {#if lines.length === 0}
    <EmptyState message={search ? 'Nothing matches this filter.' : 'Nothing was left alone.'} />
  {:else}
    <div class="list-area">
      <VirtualList items={lines} rowHeight={ROW_H} getKey={lineKey} role="list" ariaLabel="Names left alone">
        {#snippet row({ item }: { item: RefusedLine })}
          {#if item.kind === 'group'}
            <div class="grp">
              <span class="grp-label">{item.label}</span>
              <Badge size="sm" tone="warning" variant="tone">{item.count}</Badge>
              <span class="grp-hint">{item.hint}</span>
            </div>
          {:else}
            <div class="itm">
              <code class="name">{item.refusal.name}</code>
              <span class="reason" use:tooltip={item.refusal.reason}>{item.refusal.reason}</span>
              <span class="where" use:tooltip={item.refusal.file}>{baseName(item.refusal.file)}:{item.refusal.line}</span>
              <Button variant="ghost" size="xs" tooltip="Go to declaration"
                      onclick={() => onGoTo(item.refusal.file, item.refusal.line)}>
                <ExternalLink size={12} />
              </Button>
            </div>
          {/if}
        {/snippet}
      </VirtualList>
    </div>
  {/if}
</div>

<style>
  .refused { display: flex; flex-direction: column; gap: 8px; min-height: 0; flex: 1; }
  .list-area { flex: 1; min-height: 0; display: grid; }
  .grp, .itm {
    display: flex; align-items: center; gap: 8px; height: 30px; padding: 0 8px;
    font-size: 12px; box-sizing: border-box; border-radius: var(--radius-sm);
  }
  .grp { background: var(--bg-elevated); color: var(--text-secondary); font-weight: 600; }
  .grp-hint { font-weight: 400; font-size: 11px; color: var(--text-muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .itm:hover { background: var(--bg-hover); }
  .name { font-family: var(--font-mono); color: var(--text-primary); flex: none; max-width: 28%; overflow: hidden; text-overflow: ellipsis; }
  .reason { flex: 1; min-width: 0; color: var(--text-muted); font-size: 11px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .where { flex: none; font-size: 10px; color: var(--text-muted); }
</style>
