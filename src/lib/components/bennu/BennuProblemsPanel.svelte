<script lang="ts">
  /**
   * Problems (bottom dock / right tool window) — a TREE grouped **by severity first**:
   *   • **Errors** / **Warnings** (/ Info / Hints) are the top-level nodes.
   *   • Under each, problems are sub-grouped by their source — a **JDK** node (no JDK / fallback
   *     JDK), an **Encoding** node (files not valid in the declared encoding), and one node per
   *     **file** carrying diagnostics.
   *
   * A file's rows can come from any of **four** sources, which is the point: the active buffer's
   * live validation, the whole-project Java validation, a project mojibake scan, and — the one
   * that was missing until it was noticed — whatever the **language servers** are publishing.
   * That last one is what makes this panel say anything at all on a Rust, TypeScript or Svelte
   * project: `bennu_lsp_problems` existed on both sides of the wire and nothing ever called it,
   * so Problems was, in practice, a Java panel.
   *   • Each node (severity and file) is collapsible; the leaf rows click to jump.
   *
   * Because grouping is severity-first, one file can appear under both Errors and Warnings — each
   * with only its rows of that severity. The header count is every leaf row.
   */
  import { tooltip } from '$lib/actions/tooltip';
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
  import type { Diagnostic } from '$lib/types/bennu';

  // The bottom dock owns the header (tab switcher), so hide PanelShell's own.
  let { hideHeader = false }: { hideHeader?: boolean } = $props();

  const activePath = $derived(projectStore.activeFilePath);

  // ── Active-file diagnostics — the editor's LIVE buffer validation, shared via the store, so this
  // section updates as you type / fix a problem (no manual "Validate project" re-run). Only shown
  // when the store's diagnostics are for the current file (a stale push for an old file is ignored).
  const diags = $derived(
    activePath
    && bennuDiagnosticsStore.activeFile
    && norm(bennuDiagnosticsStore.activeFile) === norm(activePath)
      ? bennuDiagnosticsStore.activeFileDiagnostics
      : [],
  );

  type Severity = Diagnostic['severity'];
  /** One leaf problem, tagged with the SOURCE group (a file, or the JDK/Encoding pseudo-groups) it
   *  belongs to — grouping is done afterwards, severity-first. */
  interface Item {
    id: string;
    severity: Severity;
    groupKey: string;            // file path, or 'jdk' / 'encoding'
    groupLabel: string;
    groupIcon: typeof Coffee;
    label: string;
    detail?: string;
    title?: string;
    copy?: string;
    onClick?: () => void;
  }
  interface FileGroup {
    key: string;
    label: string;
    icon: typeof Coffee;
    rows: Item[];
  }
  interface SevGroup {
    severity: Severity;
    label: string;
    count: number;
    files: FileGroup[];
  }

  const SEV_ORDER: Severity[] = ['error', 'warning', 'info', 'hint'];
  const SEV_LABEL: Record<string, string> = {
    error: 'Errors', warning: 'Warnings', info: 'Info', hint: 'Hints',
  };

  function baseName(path: string): string {
    return path.split(/[\\/]/).pop() ?? path;
  }

  /** Normalise a path to forward slashes for comparison (BE emits `/`, the FE may hold `\`). */
  function norm(path: string): string {
    return path.replace(/\\/g, '/');
  }

  // Map a UTF-8 byte offset to a 1-based line for the jump (cheap; the editor re-centres).
  function lineOfOffset(offset: number): number {
    const src = projectStore.activeSource;
    let line = 1;
    for (let i = 0; i < offset && i < src.length; i++) if (src[i] === '\n') line++;
    return line;
  }

  const jdk = $derived(bennuDiagnosticsStore.jdk);

  // Flat list of every leaf problem, each tagged with its source group. Grouping into the
  // severity-first tree happens in `tree` below.
  const items = $derived.by<Item[]>(() => {
    const out: Item[] = [];

    // JDK
    if (bennuDiagnosticsStore.jdkMissing) {
      out.push({
        id: 'jdk-missing', severity: 'error',
        groupKey: 'jdk', groupLabel: 'JDK', groupIcon: Coffee,
        label: 'No JDK found',
        detail: 'Completion and navigation can’t resolve the standard library — set a JDK path in Settings.',
        onClick: () => bennuUiStore.openSettings(),
      });
    } else if (bennuDiagnosticsStore.jdkFallback && jdk) {
      out.push({
        id: 'jdk-fallback', severity: 'warning',
        groupKey: 'jdk', groupLabel: 'JDK', groupIcon: Coffee,
        label: `Project targets Java ${jdk.requested_major ?? '?'}, using Java ${jdk.resolved_major ?? '?'}`,
        detail: jdk.resolved_home ?? 'a different JDK is resolving the standard library — install the matching JDK, or set a path in Settings',
        title: jdk.resolved_home ?? undefined,
        onClick: () => bennuUiStore.openSettings(),
      });
    }

    // Encoding
    for (const e of bennuDiagnosticsStore.encodingIssues) {
      out.push({
        id: `enc:${e.file}`, severity: 'warning',
        groupKey: 'encoding', groupLabel: 'Encoding', groupIcon: FileWarning,
        label: baseName(e.file),
        detail: `not valid ${e.declared_encoding} — recovered as ${e.decoded_as}`,
        title: e.file,
        copy: e.file,
        onClick: () => void projectStore.openFile(e.file),
      });
    }

    // Active-file diagnostics (live buffer — authoritative for the open file).
    if (activePath && diags.length) {
      const label = baseName(activePath);
      diags.forEach((d, i) => {
        out.push({
          id: `d:${i}`, severity: d.severity,
          groupKey: `file:${norm(activePath)}`, groupLabel: label, groupIcon: FileCode2,
          label: d.message,
          detail: `@${d.start}`,
          copy: d.message,
          onClick: () => bennuUiStore.requestGoto(lineOfOffset(d.start)),
        });
      });
    }

    // Whole-project validation results. The active file is skipped — its live rows above are more
    // up-to-date (reflect unsaved edits).
    const activeNorm = activePath ? norm(activePath) : null;
    for (const fd of bennuDiagnosticsStore.projectDiagnostics) {
      if (norm(fd.file) === activeNorm) continue;
      const label = baseName(fd.file);
      fd.diagnostics.forEach((d, i) => {
        out.push({
          id: `proj:${fd.file}:${i}`, severity: d.severity,
          groupKey: `file:${norm(fd.file)}`, groupLabel: label, groupIcon: FileCode2,
          label: d.message,
          title: fd.file,
          copy: d.message,
          onClick: () => void projectStore.openFile(fd.file).then(() => bennuUiStore.requestGotoOffset(d.start)),
        });
      });
    }

    // What the LANGUAGE SERVERS report. Its own pass and not merged into the list above, because
    // the two lists have different lifecycles and a project can legitimately have both — a
    // polyglot repository whose Java half was validated and whose Rust half is being checked.
    // The active file is skipped for the same reason: its live rows are ahead of anything a
    // server published before the last keystroke.
    for (const fd of bennuDiagnosticsStore.serverDiagnostics) {
      if (norm(fd.file) === activeNorm) continue;
      const label = baseName(fd.file);
      fd.diagnostics.forEach((d, i) => {
        out.push({
          id: `lsp:${fd.file}:${i}`, severity: d.severity,
          groupKey: `file:${norm(fd.file)}`, groupLabel: label, groupIcon: FileCode2,
          label: d.message,
          title: fd.file,
          copy: d.message,
          onClick: () => void projectStore.openFile(fd.file).then(() => bennuUiStore.requestGotoOffset(d.start)),
        });
      });
    }

    // Project mojibake scan hits (explicitly added from the scan modal). Grouped by file like the
    // validation rows; NOT skipped for the active file (its mojibake hits aren't in the live buffer
    // diagnostics, which come from validation, not the mojibake scan).
    for (const fd of bennuDiagnosticsStore.mojibakeDiagnostics) {
      const label = baseName(fd.file);
      fd.diagnostics.forEach((d, i) => {
        out.push({
          id: `moji:${fd.file}:${i}`, severity: d.severity,
          groupKey: `file:${norm(fd.file)}`, groupLabel: label, groupIcon: FileCode2,
          label: d.message,
          title: fd.file,
          copy: d.message,
          onClick: () => void projectStore.openFile(fd.file).then(() => bennuUiStore.requestGotoOffset(d.start)),
        });
      });
    }

    return out;
  });

  // Severity-first tree: severity → file/source group → rows. Insertion order of files is preserved
  // (JDK/Encoding first if present, then files as the stores list them).
  const tree = $derived.by<SevGroup[]>(() => {
    const bySev = new Map<Severity, Map<string, FileGroup>>();
    for (const it of items) {
      let files = bySev.get(it.severity);
      if (!files) { files = new Map(); bySev.set(it.severity, files); }
      let fg = files.get(it.groupKey);
      if (!fg) { fg = { key: it.groupKey, label: it.groupLabel, icon: it.groupIcon, rows: [] }; files.set(it.groupKey, fg); }
      fg.rows.push(it);
    }
    const out: SevGroup[] = [];
    for (const sev of SEV_ORDER) {
      const files = bySev.get(sev);
      if (!files) continue;
      let count = 0;
      for (const fg of files.values()) count += fg.rows.length;
      out.push({ severity: sev, label: SEV_LABEL[sev] ?? sev, count, files: [...files.values()] });
    }
    return out;
  });

  const total = $derived(items.length);

  // Collapsed nodes (severity + file), keyed by id. Default expanded.
  const collapsed = new SvelteSet<string>();
  function toggle(id: string) { if (collapsed.has(id)) collapsed.delete(id); else collapsed.add(id); }

  function sevIcon(sev: Severity) {
    return sev === 'error' ? CircleAlert : sev === 'warning' ? AlertTriangle : Info;
  }

  function copyText(text: string) {
    void navigator.clipboard?.writeText(text).catch(() => { /* denied — ignore */ });
  }

  function onRowContextMenu(row: Item, e: MouseEvent) {
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
      {#each tree as sev (sev.severity)}
        {@const SevIc = sevIcon(sev.severity)}
        {@const sevId = `sev:${sev.severity}`}
        {@const sevCollapsed = collapsed.has(sevId)}
        <button
          class="pb-sec"
          type="button"
          role="treeitem"
          aria-expanded={!sevCollapsed}
          onclick={() => toggle(sevId)}
        >
          <span class="pb-chev">{#if sevCollapsed}<ChevronRight size={13} />{:else}<ChevronDown size={13} />{/if}</span>
          <span class="pb-sec-icon sev-{sev.severity}"><SevIc size={13} /></span>
          <span class="pb-sec-label">{sev.label}</span>
          <span class="pb-sec-count">{sev.count}</span>
        </button>
        {#if !sevCollapsed}
          {#each sev.files as fg (fg.key)}
            {@const FileIc = fg.icon}
            {@const fileId = `${sevId}/${fg.key}`}
            {@const fileCollapsed = collapsed.has(fileId)}
            <button
              class="pb-file"
              type="button"
              role="treeitem"
              aria-expanded={!fileCollapsed}
              onclick={() => toggle(fileId)}
            >
              <span class="pb-chev">{#if fileCollapsed}<ChevronRight size={12} />{:else}<ChevronDown size={12} />{/if}</span>
              <span class="pb-file-icon"><FileIc size={12} /></span>
              <span class="pb-file-label">{fg.label}</span>
              <span class="pb-sec-count">{fg.rows.length}</span>
            </button>
            {#if !fileCollapsed}
              {#each fg.rows as row (row.id)}
                {@const RowIc = sevIcon(row.severity)}
                <button
                  class="pb-row"
                  type="button"
                  role="treeitem"
                  onclick={() => row.onClick?.()}
                  oncontextmenu={(e) => onRowContextMenu(row, e)}
                  use:tooltip={row.title}
                >
                  <span class="pb-icon sev-{row.severity}"><RowIc size={12} /></span>
                  <span class="pb-msg">{row.label}</span>
                  {#if row.detail}<span class="pb-detail">{row.detail}</span>{/if}
                </button>
              {/each}
            {/if}
          {/each}
        {/if}
      {/each}
    </div>
  {/if}
</PanelShell>

<style>
  .pb-clean {
    display: flex; align-items: center; gap: 6px;
    padding: 14px 16px; color: var(--success); font-size: var(--font-size-sm);
  }
  .pb-tree { padding: 4px 0; }

  .pb-sec {
    display: flex; align-items: center; gap: 6px;
    width: 100%; text-align: left; box-sizing: border-box;
    padding: 5px 12px 5px 6px; font-size: var(--font-size-sm); cursor: pointer;
    background: transparent; border: none; font-family: var(--font-ui-sans);
    color: var(--text-primary); font-weight: 600;
  }
  .pb-sec:hover { background: var(--bg-hover); }
  .pb-chev { display: flex; flex-shrink: 0; color: var(--text-muted); }
  .pb-sec-icon { display: flex; flex-shrink: 0; }
  .pb-sec-label { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .pb-sec-count {
    flex-shrink: 0; font-size: var(--font-size-2xs); font-weight: 700; color: var(--text-muted);
    background: var(--bg-overlay); border-radius: var(--radius-sm); padding: 0 6px;
    font-variant-numeric: tabular-nums;
  }

  /* Second level: the file / source group under a severity. Lighter weight than the severity node,
     indented one step. */
  .pb-file {
    display: flex; align-items: center; gap: 6px;
    width: 100%; text-align: left; box-sizing: border-box;
    padding: 4px 12px 4px 24px; font-size: var(--font-size-sm); cursor: pointer;
    background: transparent; border: none; font-family: var(--font-ui-sans);
    color: var(--text-primary); font-weight: 500;
  }
  .pb-file:hover { background: var(--bg-hover); }
  .pb-file-icon { display: flex; flex-shrink: 0; color: var(--text-muted); }
  .pb-file-label { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }

  .pb-row {
    display: flex; align-items: center; gap: 8px;
    width: 100%; text-align: left; box-sizing: border-box;
    padding: 4px 12px 4px 48px; font-size: var(--font-size-sm); cursor: pointer;
    background: transparent; border: none; font-family: var(--font-ui-sans);
    transition: background var(--transition-fast);
  }
  .pb-row:hover { background: var(--bg-hover); }
  .pb-icon { display: flex; flex-shrink: 0; }
  .sev-error { color: var(--error); }
  .sev-warning { color: var(--warning); }
  .sev-info, .sev-hint { color: var(--info); }
  .pb-msg { flex-shrink: 0; max-width: 55%; color: var(--text-secondary); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .pb-detail { flex: 1; min-width: 0; font-size: var(--font-size-2xs); color: var(--text-muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
</style>
