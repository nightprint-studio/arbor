<script lang="ts">
  /**
   * Problems (bottom dock / right tool window) — a small TREE grouped into sections:
   *   • **JDK** — a warning when no JDK is installed (completion/nav can't resolve the
   *     standard library) or when a fallback JDK stands in for the level the project targets.
   *   • **Encoding** — the source files whose bytes weren't valid in the project's declared
   *     encoding (recovered + indexed, but flagged); click one to open it.
   *   • **<active file>** — the diagnostics for the open file (from `bennu_diagnostics`);
   *     click one to jump to its line.
   *
   * Each section is collapsible. The count in the header is every row across sections.
   */
  import {
    AlertTriangle, CircleAlert, Info, CircleCheckBig, ChevronRight, ChevronDown,
    Coffee, FileWarning, FileCode2,
  } from 'lucide-svelte';
  import { SvelteSet } from 'svelte/reactivity';
  import PanelShell from '$lib/components/shared/ui/PanelShell.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import { bennuUiStore } from '$lib/stores/bennu/ui.svelte';
  import { bennuDiagnosticsStore } from '$lib/stores/bennu/diagnostics.svelte';
  import { bennuContextMenuStore } from '$lib/stores/bennu/contextmenu.svelte';
  import type { MenuItem } from '$lib/components/shared/ContextMenu.svelte';
  import { diagnostics as ipcDiagnostics } from '$lib/ipc/bennu';
  import type { Diagnostic } from '$lib/types/bennu';

  // The bottom dock owns the header (tab switcher), so hide PanelShell's own.
  let { hideHeader = false }: { hideHeader?: boolean } = $props();

  const activePath = $derived(projectStore.activeFilePath);

  // ── Active-file diagnostics (existing wiring) ────────────────────────────────
  let diags = $state<Diagnostic[]>([]);
  $effect(() => {
    const path = activePath;
    if (!path) { diags = []; return; }
    let cancelled = false;
    void ipcDiagnostics(path)
      .then((ds) => { if (!cancelled) diags = ds; })
      .catch(() => { if (!cancelled) diags = []; });
    return () => { cancelled = true; };
  });

  type Severity = Diagnostic['severity'];
  interface Row {
    id: string;
    label: string;
    detail?: string;
    title?: string;
    severity: Severity;
    onClick?: () => void;
    copy?: string;
  }
  interface Section {
    id: string;
    label: string;
    icon: typeof Coffee;
    severity: Severity;
    rows: Row[];
  }

  function baseName(path: string): string {
    return path.split(/[\\/]/).pop() ?? path;
  }

  // Map a UTF-8 byte offset to a 1-based line for the jump (cheap; the editor re-centres).
  function lineOfOffset(offset: number): number {
    const src = projectStore.activeSource;
    let line = 1;
    for (let i = 0; i < offset && i < src.length; i++) if (src[i] === '\n') line++;
    return line;
  }

  const jdk = $derived(bennuDiagnosticsStore.jdk);

  const sections = $derived.by<Section[]>(() => {
    const out: Section[] = [];

    // JDK
    if (bennuDiagnosticsStore.jdkMissing) {
      out.push({
        id: 'jdk', label: 'JDK', icon: Coffee, severity: 'error',
        rows: [{
          id: 'jdk-missing', severity: 'error',
          label: 'No JDK found',
          detail: 'Completion and navigation can’t resolve the standard library — set a JDK path in Settings.',
          onClick: () => bennuUiStore.openSettings(),
        }],
      });
    } else if (bennuDiagnosticsStore.jdkFallback && jdk) {
      out.push({
        id: 'jdk', label: 'JDK', icon: Coffee, severity: 'warning',
        rows: [{
          id: 'jdk-fallback', severity: 'warning',
          label: `Project targets Java ${jdk.requested_major ?? '?'}, using Java ${jdk.resolved_major ?? '?'}`,
          detail: jdk.resolved_home ?? 'a different JDK is resolving the standard library — install the matching JDK, or set a path in Settings',
          title: jdk.resolved_home ?? undefined,
          onClick: () => bennuUiStore.openSettings(),
        }],
      });
    }

    // Encoding
    const enc = bennuDiagnosticsStore.encodingIssues;
    if (enc.length) {
      out.push({
        id: 'encoding', label: 'Encoding', icon: FileWarning, severity: 'warning',
        rows: enc.map((e) => ({
          id: `enc:${e.file}`, severity: 'warning',
          label: baseName(e.file),
          detail: `not valid ${e.declared_encoding} — recovered as ${e.decoded_as}`,
          title: e.file,
          copy: e.file,
          onClick: () => void projectStore.openFile(e.file),
        })),
      });
    }

    // Active-file diagnostics
    if (activePath && diags.length) {
      out.push({
        id: 'file', label: baseName(activePath), icon: FileCode2,
        severity: diags.some((d) => d.severity === 'error') ? 'error' : 'warning',
        rows: diags.map((d, i) => ({
          id: `d:${i}`, severity: d.severity,
          label: d.message,
          detail: `@${d.start}`,
          copy: d.message,
          onClick: () => bennuUiStore.requestGoto(lineOfOffset(d.start)),
        })),
      });
    }

    return out;
  });

  const total = $derived(sections.reduce((n, s) => n + s.rows.length, 0));

  // Collapsed sections (local). Default expanded.
  const collapsed = new SvelteSet<string>();
  function toggle(id: string) { if (collapsed.has(id)) collapsed.delete(id); else collapsed.add(id); }

  function sevIcon(sev: Severity) {
    return sev === 'error' ? CircleAlert : sev === 'warning' ? AlertTriangle : Info;
  }

  function copyText(text: string) {
    void navigator.clipboard?.writeText(text).catch(() => { /* denied — ignore */ });
  }

  function onRowContextMenu(row: Row, e: MouseEvent) {
    e.preventDefault();
    const items: MenuItem[] = [];
    if (row.onClick) items.push({ id: 'open', label: 'Open', icon: ChevronRight });
    if (row.copy) items.push({ id: 'copy', label: 'Copy', icon: FileCode2 });
    if (!items.length) return;
    bennuContextMenuStore.show(e.clientX, e.clientY, items, (id) => {
      if (id === 'open') row.onClick?.();
      else if (id === 'copy' && row.copy) copyText(row.copy);
    });
  }
</script>

<PanelShell title="Problems" count={total} {hideHeader}>
  {#snippet icon()}<AlertTriangle size={13} />{/snippet}

  {#if !projectStore.project}
    <EmptyState message="Open a project to see its problems." />
  {:else if total === 0}
    <div class="pb-clean"><CircleCheckBig size={14} /> No problems detected</div>
  {:else}
    <div class="pb-tree" role="tree" aria-label="Problems">
      {#each sections as sec (sec.id)}
        {@const SecIcon = sec.icon}
        {@const isCollapsed = collapsed.has(sec.id)}
        <button
          class="pb-sec"
          type="button"
          role="treeitem"
          aria-expanded={!isCollapsed}
          onclick={() => toggle(sec.id)}
        >
          <span class="pb-chev">{#if isCollapsed}<ChevronRight size={13} />{:else}<ChevronDown size={13} />{/if}</span>
          <span class="pb-sec-icon sev-{sec.severity}"><SecIcon size={13} /></span>
          <span class="pb-sec-label">{sec.label}</span>
          <span class="pb-sec-count">{sec.rows.length}</span>
        </button>
        {#if !isCollapsed}
          {#each sec.rows as row (row.id)}
            {@const RowIc = sevIcon(row.severity)}
            <button
              class="pb-row"
              type="button"
              role="treeitem"
              onclick={() => row.onClick?.()}
              oncontextmenu={(e) => onRowContextMenu(row, e)}
              title={row.title}
            >
              <span class="pb-icon sev-{row.severity}"><RowIc size={12} /></span>
              <span class="pb-msg">{row.label}</span>
              {#if row.detail}<span class="pb-detail">{row.detail}</span>{/if}
            </button>
          {/each}
        {/if}
      {/each}
    </div>
  {/if}
</PanelShell>

<style>
  .pb-clean {
    display: flex; align-items: center; gap: 6px;
    padding: 14px 16px; color: var(--success); font-size: 12px;
  }
  .pb-tree { padding: 4px 0; }

  .pb-sec {
    display: flex; align-items: center; gap: 6px;
    width: 100%; text-align: left; box-sizing: border-box;
    padding: 5px 12px 5px 6px; font-size: 12px; cursor: pointer;
    background: transparent; border: none; font-family: var(--font-ui-sans);
    color: var(--text-primary); font-weight: 600;
  }
  .pb-sec:hover { background: var(--bg-hover); }
  .pb-chev { display: flex; flex-shrink: 0; color: var(--text-muted); }
  .pb-sec-icon { display: flex; flex-shrink: 0; }
  .pb-sec-label { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .pb-sec-count {
    flex-shrink: 0; font-size: 10px; font-weight: 700; color: var(--text-muted);
    background: var(--bg-overlay); border-radius: var(--radius-sm); padding: 0 6px;
    font-variant-numeric: tabular-nums;
  }

  .pb-row {
    display: flex; align-items: center; gap: 8px;
    width: 100%; text-align: left; box-sizing: border-box;
    padding: 4px 12px 4px 30px; font-size: 12px; cursor: pointer;
    background: transparent; border: none; font-family: var(--font-ui-sans);
    transition: background var(--transition-fast);
  }
  .pb-row:hover { background: var(--bg-hover); }
  .pb-icon { display: flex; flex-shrink: 0; }
  .sev-error { color: var(--error); }
  .sev-warning { color: var(--warning); }
  .sev-info, .sev-hint { color: var(--info); }
  .pb-msg { flex-shrink: 0; max-width: 55%; color: var(--text-secondary); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .pb-detail { flex: 1; min-width: 0; font-size: 10.5px; color: var(--text-muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
</style>
