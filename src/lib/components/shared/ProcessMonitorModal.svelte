<script lang="ts">
  /**
   * Process Monitor — what Arbor is costing this machine, in one screen for the whole suite.
   *
   * ## One screen, not one per product
   *
   * Arbor is not one process: it is this shell, its WebView, a backend per product, whatever those
   * backends started — a language server, a JVM under test, a `cargo` — and whatever *those*
   * started. A monitor per product would answer a fraction of the question each and never the one
   * actually being asked, which is "what is making this machine slow". So it lives in `shared/`,
   * is mounted by `GlobalOverlays` in every window, and is reached from every product's command
   * palette.
   *
   * Each row carries the icon of the product it belongs to, inherited from the backend that
   * started it: a `rust-analyzer` two levels under `bennu-be` is Bennu's, and reads as Bennu's.
   *
   * ## It reports; it does not cap
   *
   * There is no portable way to hold a native process under a memory ceiling — Windows has job
   * objects, Linux has cgroups, macOS has nothing equivalent — so there is no "maximum RAM" switch
   * here, because it would work on one platform and silently do nothing on the other two. What is
   * real, and is offered: the measurement, a line you draw yourself, and Restart on the one kind of
   * process that can be restarted without losing anything — a language server, which rebuilds its
   * state from the files.
   *
   * The CPU budgets that *are* real limits already exist and are per-product: Bennu's indexing and
   * validation thread caps, in its own settings.
   */
  import { onDestroy } from 'svelte';
  import { Activity, ChevronDown, ChevronRight, Cpu, MemoryStick, RotateCcw, TriangleAlert } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import ModalFooter from '$lib/components/shared/ModalFooter.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import NumberStepper from '$lib/components/shared/ui/NumberStepper.svelte';
  import ProductIcon from '$lib/components/shared/internal/ProductIcon.svelte';
  import ArborLogo from '$lib/components/shared/internal/ArborLogo.svelte';
  import { productIdentity } from '$lib/utils/products';
  import { tooltip } from '$lib/actions/tooltip';
  import { processMonitorStore, type WatchedRow } from '$lib/stores/process-monitor.svelte';
  import { lspRestart, lspStatus } from '$lib/ipc/bennu/lsp';
  import { processMemory, type MemoryItem, type MemoryReport } from '$lib/ipc/processes';
  import { toastStore } from '$lib/feedback/stores/toasts.svelte';

  let { onClose }: { onClose: () => void } = $props();

  const store = processMonitorStore;

  // Sampling runs only while this is open — a monitor that polled in the background would be a
  // process doing work to report that processes are doing work.
  void store.start();
  onDestroy(() => store.stop());

  const report = $derived(store.report);
  const rows = $derived(store.rows);
  const warnings = $derived(store.warnings);

  /**
   * What a row is called on screen. A backend is shown by the product it serves rather than by its
   * binary — `Bennu`, not `bennu-be` — and the shell as Arbor, the application you are looking at.
   */
  function displayName(row: WatchedRow): string {
    if (row.kind === 'shell') return 'Arbor';
    if (row.kind === 'backend' && row.product) return productIdentity(row.product)?.name ?? row.name;
    return row.name;
  }

  /** The line under the name: what the process is to Arbor, and whose it is. */
  function describe(row: WatchedRow): string {
    const owner = row.product ? productIdentity(row.product)?.name ?? row.product : null;
    switch (row.kind) {
      case 'shell': return 'Frontend';
      case 'backend': return `Backend · ${row.name}`;
      case 'helper': return 'Frontend · WebView';
      case 'language-server': return owner ? `Started by ${owner}` : 'Language server';
      default: return owner ? `Started by ${owner}` : 'Started by Arbor';
    }
  }

  /**
   * Three tables, because the three answer three different questions.
   *
   * One list mixed them, and a `java` under test or a terminal's `zsh` then sat between Arbor's own
   * backends as if it were one — so "what is Arbor costing" could only be answered by reading every
   * row and deciding. Arbor itself is what you would look at to judge the application; language
   * servers are the usual reason a machine is slow and the only rows with an action; everything
   * else is what you started through Arbor, and it ends when you stop it.
   */
  const groups = $derived([
    {
      id: 'arbor', title: 'Arbor',
      hint: 'The application itself — its frontend and one backend per product.',
      rows: rows.filter((r) => r.kind === 'shell' || r.kind === 'backend' || r.kind === 'helper'),
      empty: '',
    },
    {
      id: 'lsp', title: 'Language servers',
      hint: 'Started by a product for the projects open in it. They rebuild their state from the files, so Restart loses nothing.',
      rows: rows.filter((r) => r.kind === 'language-server'),
      empty: 'No language server is running.',
    },
    {
      id: 'other', title: 'Everything else',
      hint: 'What was started through Arbor — runs, builds, tests, a terminal’s shell. It ends when you stop it.',
      rows: rows.filter((r) => r.kind === 'child'),
      empty: 'Nothing else is running.',
    },
  ]);

  function sumCpu(list: WatchedRow[]): number { return list.reduce((n, r) => n + r.cpu, 0); }
  function sumMemory(list: WatchedRow[]): number { return list.reduce((n, r) => n + r.memory, 0); }

  function mb(bytes: number): string {
    const m = bytes / (1024 * 1024);
    if (m >= 1024) return `${(m / 1024).toFixed(2)} GB`;
    // Below a megabyte in kilobytes: a breakdown has small lines, and "0 MB" on one is not true.
    if (m < 1) return `${Math.max(1, Math.round(bytes / 1024))} KB`;
    return `${Math.round(m)} MB`;
  }

  // ── What a backend is holding ──────────────────────────────────────────────
  //
  // Opened per backend row, and loaded then — never on the sampling tick. A backend answers by
  // walking its own structures, which is fine when somebody asks and wasteful once a second.

  type Breakdown =
    | { state: 'loading' }
    | { state: 'none' }
    | { state: 'error'; message: string }
    | { state: 'ready'; report: MemoryReport };

  let breakdowns = $state<Record<number, Breakdown>>({});

  async function loadBreakdown(row: WatchedRow) {
    if (!row.product) return;
    const pid = row.pid;
    breakdowns = { ...breakdowns, [pid]: { state: 'loading' } };
    try {
      const report = await processMemory(row.product);
      if (!(pid in breakdowns)) return; // closed while it was being asked
      breakdowns = { ...breakdowns, [pid]: report ? { state: 'ready', report } : { state: 'none' } };
    } catch (e) {
      if (!(pid in breakdowns)) return;
      breakdowns = { ...breakdowns, [pid]: { state: 'error', message: e instanceof Error ? e.message : String(e) } };
    }
  }

  function toggleBreakdown(row: WatchedRow) {
    if (row.pid in breakdowns) {
      const next = { ...breakdowns };
      delete next[row.pid];
      breakdowns = next;
    } else {
      void loadBreakdown(row);
    }
  }

  /** A project by its folder name; what the backend holds once, by what it is. */
  function scopeLabel(scope: string): string {
    return scope === 'process' ? 'Whole backend' : (scope.split('/').filter(Boolean).pop() ?? scope);
  }

  /** The items per project, largest first, the backend-wide group last. */
  function byScope(items: MemoryItem[]): { scope: string; bytes: number; items: MemoryItem[] }[] {
    const groups = new Map<string, MemoryItem[]>();
    for (const item of items) groups.set(item.scope, [...(groups.get(item.scope) ?? []), item]);
    return [...groups.entries()]
      .map(([scope, list]) => ({
        scope,
        bytes: list.filter((i) => !i.mapped).reduce((n, i) => n + (i.bytes ?? 0), 0),
        items: [...list].sort((a, b) => (b.bytes ?? -1) - (a.bytes ?? -1)),
      }))
      .sort((a, b) => (a.scope === 'process' ? 1 : b.scope === 'process' ? -1 : b.bytes - a.bytes));
  }

  /** What the items account for, mapped files left out — they are not memory the backend keeps. */
  function accountedFor(report: MemoryReport): number {
    return report.items.filter((i) => !i.mapped).reduce((n, i) => n + (i.bytes ?? 0), 0);
  }

  function uptime(seconds: number): string {
    if (seconds < 60) return `${seconds}s`;
    if (seconds < 3600) return `${Math.floor(seconds / 60)}m`;
    return `${Math.floor(seconds / 3600)}h ${Math.floor((seconds % 3600) / 60)}m`;
  }

  /**
   * How much of the whole machine the total is — the sentence a per-core figure does not say on
   * its own. 380% on a four-core machine is 95% of everything; on a sixteen-core one it is a
   * quarter, and those are not the same news.
   */
  const machineShare = $derived.by(() => {
    if (!report || !report.cores) return null;
    return Math.min(100, (report.total_cpu / report.cores));
  });

  /**
   * Restart a language server.
   *
   * Routed by **pid**: this screen measures processes and knows nothing about servers, so it asks
   * Bennu which of its servers that process is and restarts that one. The call goes to `bennu-be`
   * whichever window is open, which is the point of putting the screen in `shared/`.
   */
  let restarting = $state<number | null>(null);
  async function restartServer(row: WatchedRow) {
    restarting = row.pid;
    try {
      // Through the product's own IPC wrappers, not a hand-built call: the wire wants its fields
      // under `args`, and the first version of this built the payload flat — a Restart that could
      // only ever fail to deserialize.
      const servers = await lspStatus();
      const match = servers.find((s) => s.pid === row.pid);
      if (!match) {
        toastStore.show(`${row.name} is no longer running`, 'warning');
        return;
      }
      await lspRestart(match.root, match.language);
      toastStore.show(`${match.name} restarted`, 'success');
      await store.refresh();
    } catch (e) {
      toastStore.show(
        `Couldn't restart ${row.name} — ${e instanceof Error ? e.message : String(e)}`,
        'error',
      );
    } finally {
      restarting = null;
    }
  }
</script>

<Modal {onClose} width="820px" height="640px" ariaLabel="Process Monitor">
  {#snippet header()}
    <ModalHeader {onClose}>
      <Activity size={14} />
      <span class="modal-title">Process Monitor</span>
      {#if report}
        <span class="hdr-sub">{rows.length} processes</span>
      {/if}
    </ModalHeader>
  {/snippet}

  {#if store.error}
    <EmptyState message={`Couldn't read the process table — ${store.error}`} />
  {:else if !report}
    <EmptyState message="Measuring…" />
  {:else}
    <div class="totals">
      <div class="total">
        <Cpu size={14} />
        <span class="t-value">
          {report.cpu_sampled ? `${Math.round(report.total_cpu)}%` : '—'}
        </span>
        <span class="t-label">
          CPU, of one core
          {#if machineShare !== null && report.cpu_sampled}
            · {machineShare.toFixed(0)}% of {report.cores} cores
          {/if}
        </span>
      </div>
      <div class="total">
        <MemoryStick size={14} />
        <span class="t-value">{mb(report.total_memory)}</span>
        <span class="t-label">
          memory
          {#if report.machine_memory}
            · {((report.total_memory / report.machine_memory) * 100).toFixed(0)}% of this machine
          {/if}
        </span>
      </div>
    </div>

    {#if warnings.length}
      <div class="set-warn">
        <TriangleAlert size={13} />
        <span>
          {warnings.length === 1 ? '1 process is' : `${warnings.length} processes are`} over the line
          you drew — {warnings.map(displayName).join(', ')}.
        </span>
      </div>
    {/if}

    {#snippet table(group: (typeof groups)[number])}
      <section class="group" aria-labelledby={`pg-${group.id}`}>
        <header class="g-head">
          <h3 id={`pg-${group.id}`}>{group.title}</h3>
          <span class="g-count">{group.rows.length}</span>
          {#if group.rows.length}
            <span class="g-sum">
              {report?.cpu_sampled ? `${Math.round(sumCpu(group.rows))}%` : '—'} · {mb(sumMemory(group.rows))}
            </span>
          {/if}
        </header>
        <p class="g-hint">{group.hint}</p>
        {#if group.rows.length === 0}
          <p class="g-empty">{group.empty}</p>
        {:else}
          <div class="table" role="table" aria-label={group.title}>
            <div class="row head" role="row">
              <span class="c-what" role="columnheader">Process</span>
              <span class="c-num" role="columnheader">CPU</span>
              <span class="c-num" role="columnheader">Memory</span>
              <span class="c-up" role="columnheader">Up</span>
              <span class="c-act" role="columnheader"><span class="sr-only">Actions</span></span>
            </div>
            {#each group.rows as row (row.pid)}
              <div class="row" class:warn={row.heavy || row.busy} role="row">
                <span class="c-what" role="cell">
                  {#if row.kind === 'backend' && row.product}
                    <button
                      class="disclose"
                      type="button"
                      aria-expanded={row.pid in breakdowns}
                      aria-label={`What ${displayName(row)} is holding`}
                      onclick={() => toggleBreakdown(row)}
                    >
                      {#if row.pid in breakdowns}<ChevronDown size={12} />{:else}<ChevronRight size={12} />{/if}
                    </button>
                  {:else if group.id === 'arbor'}
                    <span class="disclose-gap" aria-hidden="true"></span>
                  {/if}
                  <span class="icon">
                    {#if row.kind === 'shell' || row.kind === 'helper'}
                      <ArborLogo size={16} />
                    {:else if row.product}
                      <ProductIcon id={row.product} size={16} />
                    {:else}
                      <Activity size={14} />
                    {/if}
                  </span>
                  <span class="names">
                    <span class="name">{displayName(row)}</span>
                    <span class="sub">{describe(row)} · pid {row.pid}</span>
                  </span>
                </span>
                <span class="c-num" class:hot={row.busy} role="cell">
                  {report?.cpu_sampled ? `${row.cpu.toFixed(0)}%` : '—'}
                </span>
                <span class="c-num" class:hot={row.heavy} role="cell">{mb(row.memory)}</span>
                <span class="c-up" role="cell">{uptime(row.uptime_s)}</span>
                <span class="c-act" role="cell">
                  {#if row.kind === 'language-server'}
                    <!-- The one process that can be restarted without losing anything: a language
                         server rebuilds its state from the files. A backend holds the session, so
                         it is not offered here. -->
                    <Button
                      variant="ghost"
                      size="sm"
                      disabled={restarting === row.pid}
                      onclick={() => void restartServer(row)}
                    >
                      {#snippet iconStart()}<RotateCcw size={12} />{/snippet}
                      {restarting === row.pid ? 'Restarting…' : 'Restart'}
                    </Button>
                  {/if}
                </span>
              </div>
              {#if row.pid in breakdowns}
                {@const b = breakdowns[row.pid]}
                <div class="breakdown" role="row">
                  <div class="bd-cell" role="cell">
                    {#if b.state === 'loading'}
                      <p class="bd-note">Asking {displayName(row)} what it is holding…</p>
                    {:else if b.state === 'none'}
                      <p class="bd-note">{displayName(row)} does not report what it holds.</p>
                    {:else if b.state === 'error'}
                      <p class="bd-note bd-error">{b.message}</p>
                    {:else}
                      <div class="bd-summary">
                        <span>Measured <strong>{mb(row.memory)}</strong></span>
                        <span>Accounted for <strong>≈ {mb(accountedFor(b.report))}</strong></span>
                        <button class="bd-refresh" type="button" onclick={() => void loadBreakdown(row)}>
                          <RotateCcw size={11} /> Refresh
                        </button>
                      </div>
                      {#each byScope(b.report.items) as scope (scope.scope)}
                        <div class="bd-scope">
                          <div class="bd-scope-head">
                            <span class="bd-scope-name">{scopeLabel(scope.scope)}</span>
                            {#if scope.bytes}<span class="bd-scope-sum">≈ {mb(scope.bytes)}</span>{/if}
                          </div>
                          {#each scope.items as item, i (i)}
                            <div class="bd-item">
                              <span class="bd-label">
                                {item.label}
                                {#if item.mapped}
                                  <span class="bd-tag" use:tooltip={'Memory-mapped file: in the measured total, but the system can take the pages back whenever it needs them.'}>mapped</span>
                                {/if}
                              </span>
                              <span class="bd-count">{item.count !== null ? item.count.toLocaleString() : ''}</span>
                              <span class="bd-bytes">
                                {item.bytes === null ? 'counted' : `${item.exact ? '' : '≈ '}${mb(item.bytes)}`}
                              </span>
                            </div>
                          {/each}
                        </div>
                      {/each}
                      <p class="bd-note">
                        Estimates the backend makes by walking its own structures. A line that says
                        <em>counted</em> was too deep to size cheaply. What the lines do not cover —
                        the allocator's own overhead, and structures nobody sizes yet — is the gap to
                        the measured total.
                      </p>
                    {/if}
                  </div>
                </div>
              {/if}
            {/each}
          </div>
        {/if}
      </section>
    {/snippet}

    {#each groups as group (group.id)}
      {@render table(group)}
    {/each}
  {/if}

  {#snippet footer()}
    <ModalFooter align="between">
      <div class="limits">
        <span class="l-label" use:tooltip={'Warn when one process holds more than this. 0 turns the warning off. It is a warning and not a cap: no operating system lets a native process be held under a memory ceiling in a way that works everywhere.'}>
          Warn above
        </span>
        <NumberStepper
          value={store.config.warn_mb}
          min={0}
          max={65536}
          step={256}
          narrow
          onchange={(v) => void store.setConfig({ ...store.config, warn_mb: v })}
          ariaLabel="Memory warning threshold, in megabytes"
        />
        <span class="l-unit">MB</span>
        <span class="l-label" use:tooltip={'Warn when one process holds more than this share of a single core across two consecutive samples. One sample is noise: every language server pins a core while it indexes.'}>
          and above
        </span>
        <NumberStepper
          value={Math.round(store.config.warn_cpu)}
          min={0}
          max={400}
          step={10}
          narrow
          onchange={(v) => void store.setConfig({ ...store.config, warn_cpu: v })}
          ariaLabel="CPU warning threshold, in percent of one core"
        />
        <span class="l-unit">% CPU</span>
      </div>
      <Button variant="primary" size="sm" onclick={onClose}>Done</Button>
    </ModalFooter>
  {/snippet}
</Modal>

<style>
  .modal-title { font-size: var(--font-size-md); font-weight: 600; color: var(--text-primary); }
  .hdr-sub { font-size: var(--font-size-xs); color: var(--text-muted); }

  .totals { display: flex; gap: 10px; margin-bottom: 12px; }
  .total {
    flex: 1; display: flex; align-items: baseline; gap: 8px; flex-wrap: wrap;
    padding: 10px 14px;
    background: var(--bg-elevated); border: 1px solid var(--border); border-radius: var(--radius-md);
  }
  .total :global(svg) { color: var(--accent); align-self: center; flex-shrink: 0; }
  .t-value {
    font-size: var(--font-size-lg); font-weight: 600; color: var(--text-primary);
    font-variant-numeric: tabular-nums;
  }
  .t-label { font-size: var(--font-size-xs); color: var(--text-muted); }

  .set-warn {
    display: flex; align-items: center; gap: 7px; margin-bottom: 12px;
    padding: 7px 10px; font-size: var(--font-size-sm); line-height: 1.4;
    color: var(--warning); background: color-mix(in srgb, var(--warning) 12%, transparent);
    border: 1px solid color-mix(in srgb, var(--warning) 30%, transparent);
    border-radius: var(--radius-md);
  }
  .set-warn :global(svg) { flex-shrink: 0; }

  .group { margin-bottom: 16px; }
  .g-head { display: flex; align-items: baseline; gap: 8px; margin: 0 2px 2px; }
  .g-head h3 {
    margin: 0; font-size: var(--font-size-sm); font-weight: 600; color: var(--text-primary);
  }
  .g-count {
    font-size: var(--font-size-2xs); font-weight: 600; color: var(--text-muted);
    background: var(--bg-overlay); border: 1px solid var(--border-subtle);
    border-radius: 999px; padding: 0 6px; line-height: 15px;
  }
  .g-sum {
    margin-left: auto; font-size: var(--font-size-xs); color: var(--text-secondary);
    font-variant-numeric: tabular-nums;
  }
  .g-hint { margin: 0 2px 6px; font-size: var(--font-size-xs); color: var(--text-muted); line-height: 1.45; }
  .g-empty {
    margin: 0 2px; padding: 8px 12px; font-size: var(--font-size-sm); color: var(--text-muted);
    font-style: italic; border: 1px dashed var(--border-subtle); border-radius: var(--radius-md);
  }

  .disclose, .disclose-gap { display: inline-flex; flex-shrink: 0; width: 16px; justify-content: center; }
  .disclose {
    padding: 2px 0; background: transparent; border: none; cursor: pointer;
    color: var(--text-muted); border-radius: var(--radius-sm);
  }
  .disclose:hover { color: var(--text-primary); background: var(--bg-hover); }

  .breakdown { border-top: 1px solid var(--border-subtle); background: var(--bg-base); }
  .bd-cell { padding: 8px 12px 10px 37px; display: flex; flex-direction: column; gap: 8px; }
  .bd-summary {
    display: flex; align-items: center; gap: 14px;
    font-size: var(--font-size-xs); color: var(--text-muted); font-variant-numeric: tabular-nums;
  }
  .bd-summary strong { color: var(--text-primary); font-weight: 600; }
  .bd-refresh {
    margin-left: auto; display: inline-flex; align-items: center; gap: 4px;
    padding: 2px 6px; background: transparent; border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm); color: var(--text-secondary); cursor: pointer;
    font-size: var(--font-size-2xs);
  }
  .bd-refresh:hover { color: var(--text-primary); background: var(--bg-hover); }
  .bd-scope { display: flex; flex-direction: column; }
  .bd-scope-head {
    display: flex; align-items: baseline; gap: 8px; padding: 2px 0 3px;
    border-bottom: 1px solid var(--border-subtle);
  }
  .bd-scope-name { font-size: var(--font-size-xs); font-weight: 600; color: var(--text-primary); }
  .bd-scope-sum {
    margin-left: auto; font-size: var(--font-size-xs); color: var(--accent);
    font-variant-numeric: tabular-nums;
  }
  .bd-item {
    display: grid; grid-template-columns: 1fr 90px 90px; gap: 8px; align-items: baseline;
    padding: 3px 0; font-size: var(--font-size-xs);
  }
  .bd-label { color: var(--text-secondary); min-width: 0; }
  .bd-tag {
    margin-left: 6px; padding: 0 5px; font-size: var(--font-size-2xs); color: var(--info);
    border: 1px solid color-mix(in srgb, var(--info) 35%, transparent); border-radius: 999px;
    cursor: help;
  }
  .bd-count, .bd-bytes { text-align: right; font-variant-numeric: tabular-nums; color: var(--text-muted); }
  .bd-bytes { color: var(--text-primary); }
  .bd-note { margin: 0; font-size: var(--font-size-2xs); color: var(--text-muted); line-height: 1.45; }
  .bd-error { color: var(--error); }

  .table {
    border: 1px solid var(--border); border-radius: var(--radius-md); overflow: hidden;
    background: var(--bg-elevated);
  }
  .row {
    display: grid;
    grid-template-columns: 1fr 70px 90px 60px 96px;
    align-items: center; gap: 8px;
    padding: 5px 12px;
    border-top: 1px solid var(--border-subtle);
    font-size: var(--font-size-sm);
  }
  .row:first-child { border-top: none; }
  .row.head {
    background: var(--bg-overlay);
    font-size: var(--font-size-2xs); font-weight: 600; color: var(--text-muted);
    text-transform: uppercase; letter-spacing: 0.5px;
    padding: 8px 12px;
  }
  .row.warn { background: color-mix(in srgb, var(--warning) 8%, transparent); }

  .c-what { display: flex; align-items: center; gap: 9px; min-width: 0; }
  .icon { display: inline-flex; flex-shrink: 0; width: 16px; justify-content: center; }
  .icon :global(svg) { color: var(--text-muted); }
  .names { display: flex; flex-direction: column; min-width: 0; }
  .name {
    color: var(--text-primary); overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
  .sub { font-size: var(--font-size-2xs); color: var(--text-muted); }

  /* Digits line up in a column, which is the whole reason to put numbers in one. */
  .c-num, .c-up {
    text-align: right; font-variant-numeric: tabular-nums; color: var(--text-secondary);
  }
  .c-num.hot { color: var(--warning); font-weight: 600; }
  .c-up { color: var(--text-muted); font-size: var(--font-size-xs); }
  .c-act { display: flex; justify-content: flex-end; }

  .limits { display: flex; align-items: center; gap: 6px; min-width: 0; flex-wrap: wrap; }
  .l-label { font-size: var(--font-size-xs); color: var(--text-muted); cursor: help; }
  .l-unit { font-size: var(--font-size-xs); color: var(--text-muted); }

  .sr-only {
    position: absolute; width: 1px; height: 1px; overflow: hidden;
    clip: rect(0 0 0 0); white-space: nowrap;
  }
</style>
