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
  import { TriangleAlert, CircleAlert, Wrench, MessageSquareOff, ArrowRight, CircleSlash } from 'lucide-svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import StateBlock from '$lib/components/shared/ui/StateBlock.svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import CopyButton from '$lib/components/shared/ui/CopyButton.svelte';
  import { findingToText } from './finding-text';
  import { tooltip } from '$lib/actions/tooltip';
  import { toastStore } from '$lib/feedback/stores/toasts.svelte';
  import { consistencyStore } from '$lib/stores/picus/consistency.svelte';
  import { picusProjectStore } from '$lib/stores/picus/project.svelte';
  import { picusTabsStore } from '$lib/stores/picus/tabs.svelte';
  import NoticeList from './NoticeList.svelte';
  import type { Finding } from '$lib/types/picus';

  /** `path` or `path:line` — the shape `alsoAt` arrives in. */
  function openLocation(location: string) {
    const match = /^(.*?):(\d+)$/.exec(location);
    const path = match ? match[1] : location;
    const line = match ? Number(match[2]) : undefined;
    const file = picusProjectStore.fileByPath(path);
    if (!file) {
      toastStore.show(`${path} is not in the repository index.`, 'error');
      return;
    }
    picusTabsStore.openFile(file.path, file.name, picusProjectStore.dialectOfFile(file.path), line);
  }

  /** Jump to the exact place — the whole point of a location being clickable. */
  function open(finding: Finding) {
    consistencyStore.focus(finding.id);
    openLocation(finding.line ? `${finding.file}:${finding.line}` : finding.file);
  }

  function fix(finding: Finding) {
    toastStore.show(
      `“${finding.fixLabel}” proposes a patch for review — applying it arrives with the rewriter.`,
      'info',
    );
  }

  /**
   * Keep the finding F8 landed on in view.
   *
   * Stepping through a long report with the keyboard is useless if the row you
   * are on is off screen; the highlight and this are the same feature.
   */
  let listEl = $state<HTMLDivElement | undefined>();
  $effect(() => {
    const id = consistencyStore.focusedId;
    if (!id || !listEl) return;
    listEl.querySelector(`[data-finding="${CSS.escape(id)}"]`)
      ?.scrollIntoView({ block: 'nearest' });
  });
</script>

{#if consistencyStore.error}
  <div class="fl-pad">
    <Alert variant="error" title="The rules could not be run" text={consistencyStore.error} />
  </div>
{:else if !consistencyStore.visible.length}
  <StateBlock tone={consistencyStore.skipped.length ? 'info' : 'success'}>
    <div class="fl-clean">
      <strong>
        {consistencyStore.hasRun ? 'No consistency problem found.' : 'The rules have not been run yet.'}
      </strong>
      <span>
        {#if consistencyStore.lastRunAt}
          Last checked at {consistencyStore.lastRunAt}.
        {/if}
        {#if consistencyStore.suppressedCount}
          {consistencyStore.suppressedCount} finding{consistencyStore.suppressedCount === 1 ? ' is' : 's are'}
          silenced by a declared suppression.
        {/if}
        {#if consistencyStore.skipped.length}
          {consistencyStore.skipped.length} rule{consistencyStore.skipped.length === 1 ? '' : 's'}
          could not run — see below. Nothing found is not the same as nothing wrong.
        {/if}
      </span>
    </div>
  </StateBlock>
{:else}
  <div class="fl" bind:this={listEl}>
    {#each consistencyStore.groups as group (group.key)}
      <div class="fl-group-head">
        <span>{group.label}</span>
        <Badge variant="count" label={String(group.items.length)} />
      </div>

      {#each group.items as finding (finding.id)}
        <div
          class="fl-row"
          class:fl-suppressed={!!finding.suppressedBecause}
          class:fl-focused={consistencyStore.focusedId === finding.id}
          data-finding={finding.id}
        >
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
                <!-- The second half of a paired rule: a duplicate is only readable
                     when both places are one click away, not one of them. -->
                <button
                  class="fl-loc fl-also"
                  use:tooltip={'The other place this rule pairs with'}
                  onclick={() => openLocation(finding.alsoAt!)}
                >
                  also at {finding.alsoAt}
                  <ArrowRight size={10} />
                </button>
              {/if}
            </div>
          </div>

          <!-- A report is rarely the end of the conversation: it goes into a
               ticket, a commit message, a chat with whoever wrote the other
               dialect's half. Retyping a rule id and a path is exactly the kind
               of transcription that arrives one character wrong. -->
          <CopyButton
            value={() => findingToText(finding)}
            title="Copy this finding"
            toastSuccess="Finding copied."
          />

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

<!-- Below the findings, always: a rule that could not run is part of the verdict.
     Reporting "clean" while VER003 stood down for want of readable version bounds
     would be claiming something nobody checked. -->
{#if consistencyStore.skipped.length}
  <div class="fl-skipped">
    <div class="fl-group-head">
      <CircleSlash size={11} />
      <span>Rules that could not run</span>
      <Badge variant="count" label={String(consistencyStore.skipped.length)} />
    </div>
    {#each consistencyStore.skipped as s, i (`${s.rule}:${s.scope}:${i}`)}
      <div class="fl-skip-row">
        <Badge variant="tone" tone="neutral" size="sm" label={s.rule} />
        <div class="fl-skip-body">
          <span class="fl-skip-reason">{s.reason}</span>
          {#if s.scope}<span class="fl-skip-scope">{s.scope}</span>{/if}
        </div>
      </div>
    {/each}
  </div>
{/if}

<!-- A suppression comment the analysis refused: it named nothing, or named a rule
     that never fired there. The user believes that line is silencing something. -->
{#if consistencyStore.rejectedSuppressions.length}
  <NoticeList
    notes={consistencyStore.rejectedSuppressions}
    label="Suppressions that did not apply"
    onOpen={openLocation}
  />
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
  /* Suppressed: visible, and unmistakably silenced. Hiding it would defeat the
     point of requiring a written reason. */
  .fl-suppressed { opacity: 0.62; }
  /* Where F8 last landed — the keyboard walk needs somewhere to be. */
  .fl-focused {
    background: color-mix(in srgb, var(--accent) 12%, transparent);
    box-shadow: inset 2px 0 0 var(--accent);
  }

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
  .fl-also { color: var(--text-disabled); }

  .fl-clean { display: flex; flex-direction: column; gap: 3px; text-align: left; }
  .fl-clean strong { font-size: 12px; }
  .fl-clean span { font-size: 11.5px; line-height: 1.5; color: var(--text-muted); }

  .fl-pad { padding: 10px 12px; }

  .fl-skipped { display: flex; flex-direction: column; }
  .fl-skipped .fl-group-head :global(svg) { color: var(--text-disabled); }
  .fl-skip-row {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    padding: 7px 12px;
    border-bottom: 1px solid var(--border-subtle);
  }
  .fl-skip-body { display: flex; flex-direction: column; gap: 2px; min-width: 0; }
  .fl-skip-reason { font-size: 11.5px; line-height: 1.5; color: var(--text-secondary); max-width: 100ch; }
  .fl-skip-scope { font-family: var(--font-code); font-size: 10.5px; color: var(--text-disabled); }
</style>
