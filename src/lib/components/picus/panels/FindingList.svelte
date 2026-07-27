<script lang="ts">
  /**
   * Finding list — the consistency report.
   *
   * Every row says the same four things in the same order: how bad it is, what
   * it is, **what happens in practice if it stays**, and where it lives. The
   * consequence line is the one that matters — "duplicate INSERT" tells you
   * nothing, "the second statement fails on the primary key and aborts the rest
   * of the run" tells you whether to care.
   *
   * A corrective action never applies itself: it proposes a patch, which is
   * reviewed like any other write.
   *
   * Suppressions are declared in the script (`-- picus: ignore DML001 — reason`)
   * and stay visible with their reason attached: silencing a rule without saying
   * why is not possible.
   */
  import { TriangleAlert, CircleAlert, Wrench, MessageSquareOff, ArrowRight } from 'lucide-svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import StateBlock from '$lib/components/shared/ui/StateBlock.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { toastStore } from '$lib/feedback/stores/toasts.svelte';
  import { consistencyStore } from '$lib/stores/picus/consistency.svelte';
  import { picusProjectStore } from '$lib/stores/picus/project.svelte';
  import { picusTabsStore } from '$lib/stores/picus/tabs.svelte';
  import type { Finding } from '$lib/types/picus';

  /** Jump to the exact place — the whole point of a location being clickable. */
  function open(finding: Finding) {
    const file = picusProjectStore.fileByPath(finding.file);
    if (!file) {
      toastStore.show(`${finding.file} is not in the project index.`, 'error');
      return;
    }
    picusTabsStore.openFile(file.path, file.name, picusProjectStore.dialectOfFile(file.path));
  }

  function fix(finding: Finding) {
    toastStore.show(
      `“${finding.fixLabel}” proposes a patch for review — applying it arrives with the rewriter.`,
      'info',
    );
  }
</script>

{#if !consistencyStore.visible.length}
  <StateBlock tone="success">
    <div class="fl-clean">
      <strong>No consistency problem.</strong>
      <span>
        {consistencyStore.lastRunAt
          ? `Last checked at ${consistencyStore.lastRunAt}.`
          : 'The rules have not been run yet.'}
        {#if consistencyStore.suppressedCount}
          {consistencyStore.suppressedCount} finding{consistencyStore.suppressedCount === 1 ? ' is' : 's are'}
          silenced by a declared suppression.
        {/if}
      </span>
    </div>
  </StateBlock>
{:else}
  <div class="fl">
    {#each consistencyStore.groups as group (group.key)}
      <div class="fl-group-head">
        <span>{group.label}</span>
        <Badge variant="count" label={String(group.items.length)} />
      </div>

      {#each group.items as finding (finding.id)}
        <div class="fl-row" class:fl-suppressed={!!finding.suppressedBecause}>
          <span class="fl-sev" class:fl-blocking={finding.severity === 'blocking'}>
            {#if finding.severity === 'blocking'}
              <TriangleAlert size={14} />
            {:else}
              <CircleAlert size={14} />
            {/if}
          </span>

          <div class="fl-body">
            <div class="fl-title-row">
              <span class="fl-title">{finding.title}</span>
              <span use:tooltip={'Rule identifier — stable across versions'}>
                <Badge variant="tone" tone="neutral" size="sm" label={finding.rule} />
              </span>
              {#if finding.suppressedBecause}
                <span class="fl-mute" use:tooltip={`Suppressed in the script: ${finding.suppressedBecause}`}>
                  <MessageSquareOff size={11} /> suppressed
                </span>
              {/if}
            </div>

            <p class="fl-consequence">{finding.consequence}</p>

            {#if finding.suppressedBecause}
              <p class="fl-reason">Declared reason: {finding.suppressedBecause}</p>
            {/if}

            <div class="fl-foot">
              <button class="fl-loc" onclick={() => open(finding)}>
                {finding.file}{finding.line ? `:${finding.line}` : ''}
                <ArrowRight size={10} />
              </button>
              {#if finding.alsoAt}
                <span class="fl-also">also at {finding.alsoAt}</span>
              {/if}
            </div>
          </div>

          {#if finding.fixLabel && !finding.suppressedBecause}
            <Button
              variant="secondary"
              size="xs"
              tooltip={'Builds a patch for review — nothing is written until you confirm'}
              onclick={() => fix(finding)}
            >
              {#snippet iconStart()}<Wrench size={12} />{/snippet}
              {finding.fixLabel}
            </Button>
          {/if}
        </div>
      {/each}
    {/each}
  </div>
{/if}

<style>
  .fl { display: flex; flex-direction: column; }

  .fl-group-head {
    display: flex;
    align-items: center;
    gap: 7px;
    padding: 6px 12px;
    background: var(--bg-elevated);
    border-bottom: 1px solid var(--border-subtle);
    font-size: 10px;
    font-weight: 600;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--text-muted);
    position: sticky;
    top: 0;
    z-index: 1;
  }

  .fl-row {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    padding: 9px 12px;
    border-bottom: 1px solid var(--border-subtle);
  }
  .fl-row:hover { background: var(--bg-hover); }
  .fl-suppressed { opacity: 0.62; }

  .fl-sev { display: inline-flex; padding-top: 1px; color: var(--warning); flex-shrink: 0; }
  .fl-sev.fl-blocking { color: var(--error); }

  .fl-body { flex: 1; min-width: 0; }

  .fl-title-row { display: flex; align-items: center; gap: 7px; flex-wrap: wrap; }
  .fl-title { font-size: 12px; font-weight: 600; }
  .fl-mute {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    font-size: 10px;
    color: var(--text-disabled);
  }

  /* The line that decides whether this matters. */
  .fl-consequence {
    margin-top: 3px;
    font-size: 11.5px;
    line-height: 1.5;
    color: var(--text-secondary);
    max-width: 100ch;
  }
  .fl-reason {
    margin-top: 3px;
    font-size: 11px;
    font-style: italic;
    color: var(--text-muted);
  }

  .fl-foot { display: flex; align-items: center; gap: 10px; margin-top: 4px; flex-wrap: wrap; }
  .fl-loc {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 0;
    background: none;
    border: none;
    color: var(--text-muted);
    font-family: var(--font-code);
    font-size: 10.5px;
    cursor: pointer;
  }
  .fl-loc:hover { color: var(--accent); text-decoration: underline; text-underline-offset: 2px; }
  .fl-also { font-family: var(--font-code); font-size: 10.5px; color: var(--text-disabled); }

  .fl-clean { display: flex; flex-direction: column; gap: 3px; text-align: left; }
  .fl-clean strong { font-size: 12px; }
  .fl-clean span { font-size: 11.5px; line-height: 1.5; color: var(--text-muted); }
</style>
