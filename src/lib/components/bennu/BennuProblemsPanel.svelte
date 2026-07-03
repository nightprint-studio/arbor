<script lang="ts">
  /**
   * Problems (right tool window) — diagnostics for the active file, from
   * `bennu_diagnostics` (Phase 0 backend returns [], so this shows the clean
   * state). The wiring is live: it lights up for free once the backend emits real
   * diagnostics. Clicking a row jumps the editor to the diagnostic's line.
   */
  import { AlertTriangle, CircleAlert, Info, CircleCheckBig, ArrowRight, Copy } from 'lucide-svelte';
  import PanelShell from '$lib/components/shared/ui/PanelShell.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import { bennuUiStore } from '$lib/stores/bennu/ui.svelte';
  import { bennuContextMenuStore } from '$lib/stores/bennu/contextmenu.svelte';
  import type { MenuItem } from '$lib/components/shared/ContextMenu.svelte';
  import { diagnostics as ipcDiagnostics } from '$lib/ipc/bennu';
  import type { Diagnostic } from '$lib/types/bennu';

  // The bottom dock owns the header (tab switcher), so hide PanelShell's own.
  let { hideHeader = false }: { hideHeader?: boolean } = $props();

  const activePath = $derived(projectStore.activeFilePath);

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

  // Map a UTF-8 byte offset to a 1-based line for the jump. Cheap approximation:
  // count newlines up to the offset in the active source (bytes ≈ chars for ASCII;
  // fine for a jump target — the editor re-centres on the line).
  function lineOfOffset(offset: number): number {
    const src = projectStore.activeSource;
    let line = 1;
    for (let i = 0; i < offset && i < src.length; i++) if (src[i] === '\n') line++;
    return line;
  }

  const errorCount = $derived(diags.filter((d) => d.severity === 'error').length);
  const warnCount  = $derived(diags.filter((d) => d.severity === 'warning').length);

  function iconFor(sev: Diagnostic['severity']) {
    return sev === 'error' ? CircleAlert : sev === 'warning' ? AlertTriangle : Info;
  }

  function copyText(text: string) {
    // Best-effort — clipboard can be denied (permission / focus); swallow.
    void navigator.clipboard?.writeText(text).catch(() => { /* clipboard denied — ignore */ });
  }

  function onRowContextMenu(d: Diagnostic, e: MouseEvent) {
    e.preventDefault();
    const items: MenuItem[] = [
      { id: 'goto', label: 'Go to', icon: ArrowRight },
      { id: 'copy-msg', label: 'Copy message', icon: Copy },
    ];
    bennuContextMenuStore.show(e.clientX, e.clientY, items, (id) => {
      switch (id) {
        case 'goto':     bennuUiStore.requestGoto(lineOfOffset(d.start)); break;
        case 'copy-msg': copyText(d.message); break;
      }
    });
  }
</script>

<PanelShell title="Problems" count={diags.length} {hideHeader}>
  {#snippet icon()}<AlertTriangle size={13} />{/snippet}

  {#if !activePath}
    <EmptyState message="Open a file to see its problems." />
  {:else if diags.length === 0}
    <div class="pb-clean"><CircleCheckBig size={14} /> No problems detected</div>
  {:else}
    <div class="pb-meta">{errorCount} errors · {warnCount} warnings</div>
    <div class="pb-list">
      {#each diags as d, i (i)}
        {@const Ic = iconFor(d.severity)}
        <button
          class="pb-row"
          onclick={() => bennuUiStore.requestGoto(lineOfOffset(d.start))}
          oncontextmenu={(e) => onRowContextMenu(d, e)}
        >
          <span class="pb-icon sev-{d.severity}"><Ic size={13} /></span>
          <span class="pb-msg">{d.message}</span>
          <span class="pb-loc">@{d.start}</span>
        </button>
      {/each}
    </div>
  {/if}
</PanelShell>

<style>
  .pb-clean {
    display: flex; align-items: center; gap: 6px;
    padding: 14px 16px; color: var(--success); font-size: 12px;
  }
  .pb-meta {
    padding: 6px 12px; font-size: 10.5px; color: var(--text-muted);
    border-bottom: 1px solid var(--border-subtle);
  }
  .pb-list { padding: 4px 0; }
  .pb-row {
    display: flex; align-items: center; gap: 8px;
    width: 100%; text-align: left;
    padding: 5px 12px; font-size: 12px; cursor: pointer;
    background: transparent; border: none; font-family: var(--font-ui-sans);
    transition: background var(--transition-fast);
  }
  .pb-row:hover { background: var(--bg-hover); }
  .pb-icon { display: flex; flex-shrink: 0; }
  .sev-error { color: var(--error); }
  .sev-warning { color: var(--warning); }
  .sev-info, .sev-hint { color: var(--info); }
  .pb-msg { flex: 1; min-width: 0; color: var(--text-secondary); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .pb-loc { font-family: var(--font-code); font-size: 10.5px; color: var(--text-muted); flex-shrink: 0; }
</style>
