<script lang="ts">
  import { slide } from 'svelte/transition';
  import {
    Loader, Trash2, ChevronDown, ChevronRight, Server, Hammer, Tag,
  } from 'lucide-svelte';
  import JobRow from './JobRow.svelte';
  import Toggle from '$lib/components/shared/ui/Toggle.svelte';
  import { jobsStore } from '$lib/feedback/stores/jobs.svelte';
  import { uiStore }   from '$lib/stores/ui.svelte';
  import { animStore } from '$lib/stores/animations.svelte';
  import type { JobInfo } from '$lib/feedback/types/jobs';
  import { tooltip } from '$lib/actions/tooltip';

  // ── Show / hide hidden jobs ────────────────────────────────────────────────

  const visibleJobs = $derived(
    jobsStore.showHidden
      ? jobsStore.jobs
      : jobsStore.jobs.filter(j => !j.hidden)
  );
  const hiddenTotal = $derived(jobsStore.jobs.filter(j => j.hidden).length);

  // ── Grouping ───────────────────────────────────────────────────────────────

  interface JobGroup {
    name: string | null;
    jobs: JobInfo[];
    runningCount: number;
  }

  const groups = $derived.by<JobGroup[]>(() => {
    const map: Record<string, JobInfo[]> = {};
    const uncategorized: JobInfo[] = [];

    for (const job of visibleJobs) {
      if (job.category) {
        if (!map[job.category]) map[job.category] = [];
        map[job.category].push(job);
      } else {
        uncategorized.push(job);
      }
    }

    const result: JobGroup[] = Object.entries(map).map(([name, jobs]) => ({
      name,
      jobs,
      runningCount: jobs.filter(j => j.status.type === 'running').length,
    }));

    result.sort((a, b) => {
      if (a.runningCount !== b.runningCount) return b.runningCount - a.runningCount;
      return (a.name ?? '').localeCompare(b.name ?? '');
    });

    if (uncategorized.length > 0) {
      result.push({
        name: null,
        jobs: uncategorized,
        runningCount: uncategorized.filter(j => j.status.type === 'running').length,
      });
    }

    return result;
  });

  const hasCategories = $derived(groups.some(g => g.name !== null));

  let collapsed = $state<Record<string, boolean>>({});
  function toggleGroup(name: string) { collapsed[name] = !collapsed[name]; }

  // ── Live ticker ────────────────────────────────────────────────────────────

  let tick = $state(0);

  $effect(() => {
    const hasRunning = jobsStore.jobs.some(j => j.status.type === 'running');
    if (!hasRunning) return;
    const id = setInterval(() => { tick++; }, 1000);
    return () => clearInterval(id);
  });

  // ── Helpers ────────────────────────────────────────────────────────────────

  function elapsed(job: JobInfo): string {
    void tick; // re-evaluate every second while jobs are running
    const secs = Math.floor(Date.now() / 1000) - job.started_at;
    if (secs < 60) return `${secs}s`;
    return `${Math.floor(secs / 60)}m ${secs % 60}s`;
  }

  async function openOutput(job: JobInfo) {
    await jobsStore.loadOutput(job.id);
    jobsStore.setActiveJob(job.id);
    uiStore.setJobsOverlayOpen(false);
    uiStore.setActiveBottomSection('jobs' as any);
  }
</script>

<button type="button" aria-label="Close overlay" class="overlay-backdrop" onclick={() => uiStore.setJobsOverlayOpen(false)}></button>

<div class="overlay-panel jobs-overlay" role="dialog" aria-label="Background Jobs">
  <div class="overlay-header">
    <span class="overlay-title">Background Jobs</span>
    <div class="header-actions">
      <Toggle
        size="sm"
        label={hiddenTotal > 0 ? `Show hidden (${hiddenTotal})` : 'Show hidden'}
        checked={jobsStore.showHidden}
        onchange={(v) => jobsStore.setShowHidden(v)}
      />
      {#if jobsStore.finishedCount > 0}
        <button class="clear-btn" onclick={() => jobsStore.clearFinished()} use:tooltip={'Clear finished jobs'} aria-label="Clear finished jobs">
          <Trash2 size={13} />
        </button>
      {/if}
      <button class="mac-close-btn" onclick={() => uiStore.setJobsOverlayOpen(false)} use:tooltip={'Close'} aria-label="Close"></button>
    </div>
  </div>

  {#if visibleJobs.length === 0}
    {#if hiddenTotal > 0}
      <div class="empty-state">
        {hiddenTotal} hidden — toggle <em>Show hidden</em> to reveal
      </div>
    {:else}
      <div class="empty-state">No jobs yet</div>
    {/if}
  {:else}
    <div class="job-list">
      {#each groups as group (group.name ?? '__uncategorized__')}
        {#if hasCategories && group.name !== null}
          {@const isCollapsed = !!collapsed[group.name]}
          {@const hasRunning  = group.runningCount > 0}

          <div class="category-wrap">
            <button
              class="category-header"
              class:is-running={hasRunning}
              onclick={() => toggleGroup(group.name!)}
            >
              <span class="category-icon">
                {#if group.name.toLowerCase() === 'services'}
                  <Server size={13} />
                {:else if group.name.toLowerCase() === 'builds'}
                  <Hammer size={13} />
                {:else}
                  <Tag size={13} />
                {/if}
              </span>

              <span class="category-name">{group.name}</span>

              {#if hasRunning}
                <span class="run-badge">
                  <Loader size={9} class="spin-icon" />
                  {group.runningCount}
                </span>
              {:else}
                <span class="count-badge">{group.jobs.length}</span>
              {/if}

              <span class="chevron">
                {#if isCollapsed}<ChevronRight size={12} />{:else}<ChevronDown size={12} />{/if}
              </span>
            </button>

            {#if !isCollapsed}
              <div
                class="category-body"
                transition:slide={{ duration: animStore.dBase }}
              >
                {#each group.jobs as job (job.id)}
                  <JobRow {job} elapsed={elapsed(job)} indent onOpenOutput={openOutput} />
                {/each}
              </div>
            {/if}
          </div>

        {:else}
          {#each group.jobs as job (job.id)}
            <JobRow {job} elapsed={elapsed(job)} onOpenOutput={openOutput} />
          {/each}
        {/if}
      {/each}
    </div>
  {/if}
</div>

<style>
  .jobs-overlay {
    width: 320px;
    max-height: 460px;
    /* The overlay itself must be the flex+overflow container that caps the
       list: without display:flex/column + overflow:hidden here the panel
       just grows to fit its content and .job-list's flex:1/min-height:0
       never has a bounded parent to scroll inside. */
    display: flex;
    flex-direction: column;
    overflow: hidden;
    background: var(--bg-base);
    border-color: var(--border);
    box-shadow: 0 8px 32px rgba(0,0,0,0.7);
  }

  /* Header stays fixed; only the list scrolls. */
  .overlay-header { flex-shrink: 0; }

  .header-actions {
    display: flex;
    align-items: center;
    gap: 4px;
  }

  .clear-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 22px;
    height: 22px;
    padding: 0;
    border: none;
    background: transparent;
    color: var(--text-muted);
    border-radius: var(--radius-sm);
    cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .clear-btn:hover { background: var(--bg-elevated); color: var(--text-primary); }

  .job-list {
    overflow-y: auto;
    flex: 1 1 auto;
    /* min-height:0 is the flex-child half of the recipe: it lets this child
       shrink below its content height so the scrollbar appears instead of the
       list pushing the (now flex, capped) .jobs-overlay past its max-height. */
    min-height: 0;
    padding: 4px 4px 6px;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  /* ── Category ──────────────────────────────────────────────────────────── */

  .category-wrap {
    display: flex;
    flex-direction: column;
    background: var(--bg-elevated);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    overflow: hidden;
    margin-top: 4px;
  }
  .category-wrap:first-child { margin-top: 0; }

  .category-header {
    display: flex;
    align-items: center;
    gap: 6px;
    width: 100%;
    padding: 6px 8px;
    background: none;
    border: none;
    cursor: pointer;
    color: var(--text-muted);
    font-size: 11px;
    font-family: var(--font-ui-sans);
    font-weight: 600;
    letter-spacing: 0.03em;
    border-radius: 0;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .category-header:hover { background: var(--bg-hover); color: var(--text-secondary); }

  .category-header.is-running { color: var(--accent); }
  .category-header.is-running:hover { color: var(--accent); background: color-mix(in srgb, var(--accent) 8%, transparent); }

  .category-icon { display: flex; align-items: center; flex-shrink: 0; }
  .category-name { flex: 1; text-align: left; text-transform: uppercase; font-size: 10px; letter-spacing: 0.07em; }

  .run-badge {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    font-size: 10px;
    font-weight: 600;
    padding: 1px 6px;
    background: color-mix(in srgb, var(--accent) 15%, transparent);
    color: var(--accent);
    border-radius: var(--radius-md);
  }

  .count-badge {
    font-size: 10px;
    font-weight: 500;
    color: var(--text-disabled);
    padding: 0 2px;
  }

  .chevron { display: flex; align-items: center; color: var(--text-muted); flex-shrink: 0; }

  .category-body {
    display: flex;
    flex-direction: column;
    overflow: hidden;
    border-top: 1px solid var(--border);
  }

  /* ── Job rows: markup + row styles live in JobRow.svelte ───────────────────
     The icon colour/spin globals below stay here because the category
     run-badge (Loader) also uses `.spin-icon`. */

  :global(.icon-ok)   { color: var(--success); }
  :global(.icon-err)  { color: var(--error);   }
  :global(.icon-muted){ color: var(--text-muted); }
  :global(.accent)    { color: var(--accent);  }
  :global(.spin-icon) { animation: spin 1s linear infinite; }
</style>
