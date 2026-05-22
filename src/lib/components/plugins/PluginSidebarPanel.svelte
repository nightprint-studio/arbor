<script lang="ts">
  /**
   * Renders the body of a plugin-registered sidebar/bottom panel.
   *
   * The plugin registers a panel via `arbor.ui.add_sidebar({id, ...})` and
   * responds to the `panel:open:<id>` hook by pushing content through
   * `arbor.ui.set_panel_content(id, {title, nodes, actions?})`.
   *
   * This is a pragmatic form-DSL renderer — it handles the node shapes
   * common to dashboard / launcher use-cases (labels, buttons, lists,
   * sections). Full PluginFormModal parity (tree_layout, pipeline_editor,
   * wizard, etc.) is deliberately out of scope here: a sidebar should stay
   * lightweight. Plugins that need rich editing should still open modals.
   *
   * Opening a panel is strictly non-blocking: we fire `panel:open:<id>` and
   * derive UI state from the store cache. No awaits, no loading lock — if
   * the plugin never responds, the panel shows "waiting for content" and
   * the rest of the app stays fully interactive.
   *
   * ── Refactor notes (god-objects Phase 3) ──────────────────────────────
   * This file is the *dispatcher*: it owns state, listeners, drag glue and
   * the recursive `renderNode` snippet. Each node type delegates to a
   * focused sub-renderer under `sidebar-nodes/`:
   *   · SidebarNodeLayout   — label/heading/divider/text_display/paragraph/
   *                           button/row/section/container + unknown
   *   · SidebarNodeCard     — card_item, list
   *   · SidebarNodeField    — field, color_field, vec_field, entity_ref
   *   · SidebarNodeViz      — sparkline, chart, state_graph
   *   · SidebarNodeConsole  — console_input, code
   *
   * All styles live in `sidebar-nodes/sidebar-node-styles.css` (imported
   * once below); sub-renderers don't ship styles of their own.
   */
  import { untrack, onMount } from 'svelte';
  import { contributionStore } from '$lib/stores/contribution.svelte';
  import { firePluginAction } from '$lib/ipc/plugin';
  import { PANEL_CONTENT_POINT, findPanelContent } from '$lib/contributions/panel-content';
  import { setupTauriListeners } from '$lib/utils/tauri-listeners';
  import { copyToClipboard } from '$lib/utils/clipboard';
  import PluginIcon from './PluginIcon.svelte';
  import BottomPanelHeader from '$lib/components/shared/ui/BottomPanelHeader.svelte';

  import type { SidebarNodeCtx } from './sidebar-nodes/ctx';
  import { nodeType, nodeKey, fieldKey } from './sidebar-nodes/helpers';
  import SidebarNodeLayout  from './sidebar-nodes/SidebarNodeLayout.svelte';
  import SidebarNodeCard    from './sidebar-nodes/SidebarNodeCard.svelte';
  import SidebarNodeField   from './sidebar-nodes/SidebarNodeField.svelte';
  import SidebarNodeViz     from './sidebar-nodes/SidebarNodeViz.svelte';
  import SidebarNodeConsole from './sidebar-nodes/SidebarNodeConsole.svelte';

  import './sidebar-nodes/sidebar-node-styles.css';

  interface Props {
    pluginName: string;
    panelId:    string;
    /**
     * When mounted as a bottom-docked panel, render a `BottomPanelHeader`
     * (with the close X) instead of the normal sidebar `panel-header`,
     * so we don't double-stack header bars under the AppShell wrapper.
     */
    bottomMode?: boolean;
  }
  let { pluginName, panelId, bottomMode = false }: Props = $props();

  // Reactive view of the cached content. NEVER written from this component.
  const content = $derived(
    findPanelContent(contributionStore.forPoint(PANEL_CONTENT_POINT), pluginName, panelId)
  );
  const title   = $derived(content?.title ?? '');
  const nodes   = $derived((content?.nodes ?? []) as any[]);
  const actions = $derived((content?.actions ?? []) as any[]);

  function firePanelOpen() {
    firePluginAction(pluginName, `panel:open:${panelId}`, '{}').catch(() => {});
  }

  $effect(() => {
    void pluginName; void panelId;
    untrack(() => { firePanelOpen(); });
  });

  // Plugin reload wipes the runtime; without this we'd sit on "Waiting for
  // content…" forever after a reload.
  onMount(() => setupTauriListeners([
    { event: 'arbor://plugins-reloaded', handler: () => firePanelOpen() },
  ]));

  function fireAction(action: string | undefined, payload: Record<string, unknown> = {}) {
    if (!action) return;
    firePluginAction(pluginName, action, JSON.stringify(payload)).catch(() => {});
  }

  // ── Collapsible-section state ────────────────────────────────────────
  let collapsed = $state(new Map<string, boolean>());

  function isSectionCollapsed(n: any, key: string): boolean {
    if (collapsed.has(key)) return !!collapsed.get(key);
    return n.default_open === false;
  }
  function toggleSection(key: string, current: boolean) {
    const next = new Map(collapsed);
    next.set(key, !current);
    collapsed = next;
  }

  // ── Per-code-block copy flash ────────────────────────────────────────
  let copiedKey = $state<string | null>(null);
  async function copyCode(key: string, text: string) {
    if (await copyToClipboard(text)) {
      copiedKey = key;
      setTimeout(() => { if (copiedKey === key) copiedKey = null; }, 1200);
    }
  }

  // ── Editable `field` nodes ───────────────────────────────────────────
  // Local draft mirror; the contribution-store value remains the source of
  // truth (re-derived per render). Resets whenever the panel content
  // identity changes — without this stale drafts shadow the new values.
  let fieldDraft = $state(new Map<string, unknown>());

  function fieldValue(key: string, fallback: unknown): unknown {
    return fieldDraft.has(key) ? fieldDraft.get(key) : fallback;
  }
  function setFieldDraft(key: string, value: unknown) {
    const next = new Map(fieldDraft);
    next.set(key, value);
    fieldDraft = next;
  }
  function commitField(n: any, key: string, value: unknown) {
    setFieldDraft(key, value);
    if (!n.action) return;
    fireAction(n.action, { ...(n.payload ?? {}), value });
  }

  $effect(() => {
    void content?.title;
    untrack(() => { fieldDraft = new Map(); });
  });

  // ── Composite editors (color / vec) — fan a gesture out into per-channel
  //    or per-axis mutates (one BRP mutate per scalar field). ───────────
  function commitVecAxis(n: any, axis: string, value: number) {
    if (!n.action) return;
    const basePayload = n.payload ?? {};
    const subpath = (n.subpaths && n.subpaths[axis]) ?? ('.' + axis);
    const base = basePayload.base_path ?? '';
    fireAction(n.action, { ...basePayload, path: base + subpath, value });
  }
  function commitColorChannel(n: any, channel: string, value: number) {
    if (!n.action) return;
    const basePayload = n.payload ?? {};
    const subpath = (n.channels && n.channels[channel]) ?? ('.' + channel);
    const base = basePayload.base_path ?? '';
    fireAction(n.action, { ...basePayload, path: base + subpath, value });
  }

  // ── Vec drag state ───────────────────────────────────────────────────
  // Mousedown on an axis label arms drag; mousemove translates pixels into
  // value delta until mouseup. Shift = fine, Ctrl = coarse.
  type VecDragState = {
    nodeId: string;
    axis: string;
    startX: number;
    startValue: number;
    node: any;
  };
  let vecDrag: VecDragState | null = null;
  function vecSensitivity(e: MouseEvent | KeyboardEvent): number {
    if (e.shiftKey) return 0.001;
    if (e.ctrlKey || e.metaKey) return 0.1;
    return 0.01;
  }
  function startVecDrag(node: any, axis: string, startValue: number, e: MouseEvent) {
    e.preventDefault();
    vecDrag = {
      nodeId: fieldKey(node, 0),
      axis,
      startX: e.clientX,
      startValue: Number.isFinite(startValue) ? startValue : 0,
      node,
    };
    document.body.style.cursor = 'ew-resize';
    window.addEventListener('mousemove', onVecDragMove);
    window.addEventListener('mouseup',   endVecDrag);
  }
  function onVecDragMove(e: MouseEvent) {
    if (!vecDrag) return;
    const factor = vecSensitivity(e);
    const delta = (e.clientX - vecDrag.startX) * factor;
    const next = vecDrag.startValue + delta;
    setFieldDraft(vecDrag.nodeId + '::' + vecDrag.axis, next);
    commitVecAxis(vecDrag.node, vecDrag.axis, next);
  }
  function endVecDrag() {
    vecDrag = null;
    document.body.style.cursor = '';
    window.removeEventListener('mousemove', onVecDragMove);
    window.removeEventListener('mouseup',   endVecDrag);
  }
  function vecAxisValue(node: any, axis: string): number {
    const key = fieldKey(node, 0) + '::' + axis;
    if (fieldDraft.has(key)) {
      const v = fieldDraft.get(key);
      return typeof v === 'number' ? v : 0;
    }
    const v = node.value?.[axis];
    return typeof v === 'number' ? v : 0;
  }
  function resetVecAxis(node: any, axis: string) {
    const def = node.defaults?.[axis] ?? 0;
    commitVecAxis(node, axis, def);
    setFieldDraft(fieldKey(node, 0) + '::' + axis, def);
  }

  // ── Console input state ──────────────────────────────────────────────
  let consoleDraft       = $state(new Map<string, string>());
  let consoleHistoryIdx  = $state(new Map<string, number>());
  let suggestVisible     = $state(new Map<string, boolean>());
  let suggestActive      = $state(new Map<string, number>());

  function consoleValue(key: string): string {
    return consoleDraft.has(key) ? (consoleDraft.get(key) ?? '') : '';
  }
  function setConsoleValue(key: string, v: string) {
    const m = new Map(consoleDraft); m.set(key, v); consoleDraft = m;
  }
  function setHistoryIdx(key: string, idx: number) {
    const m = new Map(consoleHistoryIdx); m.set(key, idx); consoleHistoryIdx = m;
  }
  function setSuggestVisible(key: string, v: boolean) {
    const m = new Map(suggestVisible); m.set(key, v); suggestVisible = m;
  }
  function setSuggestActive(key: string, v: number) {
    const m = new Map(suggestActive); m.set(key, v); suggestActive = m;
  }
  function consoleMatches(n: any, text: string): string[] {
    const sug = Array.isArray(n.suggestions) ? (n.suggestions as string[]) : [];
    if (!text) return [];
    const lc = text.toLowerCase();
    const matches: string[] = [];
    for (const s of sug) {
      if (typeof s !== 'string') continue;
      if (s.toLowerCase().startsWith(lc) && s !== text) matches.push(s);
      if (matches.length >= 20) break;
    }
    return matches;
  }
  function acceptSuggestion(n: any, key: string, value: string) {
    setConsoleValue(key, value);
    setSuggestVisible(key, false);
    setSuggestActive(key, 0);
  }
  function submitConsole(n: any) {
    const key = fieldKey(n, 0);
    const text = consoleValue(key).trim();
    if (!text) return;
    if (n.action) fireAction(n.action, { ...(n.payload ?? {}), text });
    setConsoleValue(key, '');
    setHistoryIdx(key, -1);
    setSuggestVisible(key, false);
  }
  function navHistory(n: any, key: string, dir: -1 | 1) {
    const hist = Array.isArray(n.history) ? (n.history as string[]) : [];
    if (hist.length === 0) return;
    const cur = consoleHistoryIdx.get(key) ?? -1;
    let next = cur + dir;
    if (next < -1) next = -1;
    if (next >= hist.length) next = hist.length - 1;
    setHistoryIdx(key, next);
    setConsoleValue(key, next === -1 ? '' : (hist[next] ?? ''));
  }
  function onConsoleKey(n: any, key: string, ev: KeyboardEvent) {
    const matches = consoleMatches(n, consoleValue(key));
    const showing = (suggestVisible.get(key) ?? false) && matches.length > 0;
    if (ev.key === 'Enter') {
      if (showing) {
        const idx = suggestActive.get(key) ?? 0;
        const pick = matches[Math.max(0, Math.min(idx, matches.length - 1))];
        if (pick != null) { ev.preventDefault(); acceptSuggestion(n, key, pick); return; }
      }
      ev.preventDefault();
      submitConsole(n);
    } else if (ev.key === 'Tab' && matches.length > 0) {
      ev.preventDefault();
      acceptSuggestion(n, key, matches[0]);
    } else if (ev.key === 'ArrowUp') {
      if (showing) {
        ev.preventDefault();
        const cur = suggestActive.get(key) ?? 0;
        setSuggestActive(key, Math.max(0, cur - 1));
      } else {
        ev.preventDefault();
        navHistory(n, key, 1);
      }
    } else if (ev.key === 'ArrowDown') {
      if (showing) {
        ev.preventDefault();
        const cur = suggestActive.get(key) ?? 0;
        setSuggestActive(key, Math.min(matches.length - 1, cur + 1));
      } else {
        ev.preventDefault();
        navHistory(n, key, -1);
      }
    } else if (ev.key === 'Escape') {
      if (showing) { ev.preventDefault(); setSuggestVisible(key, false); }
    }
  }

  // ── Shared rendering context handed to every sub-renderer ──────────────
  // Getters keep proxy reactivity through the component boundary —
  // mutations in dispatcher's $state propagate to children automatically.
  const ctx: SidebarNodeCtx = {
    get pluginName()         { return pluginName; },

    get collapsed()          { return collapsed; },
    get fieldDraft()         { return fieldDraft; },
    get consoleDraft()       { return consoleDraft; },
    get consoleHistoryIdx()  { return consoleHistoryIdx; },
    get suggestVisible()     { return suggestVisible; },
    get suggestActive()      { return suggestActive; },
    get copiedKey()          { return copiedKey; },

    nodeType, nodeKey, fieldKey, fieldValue,
    isSectionCollapsed, toggleSection,
    copyCode,

    fireAction,
    commitField,
    commitColorChannel,
    commitVecAxis,
    startVecDrag,
    resetVecAxis,
    vecAxisValue,
    setFieldDraft,

    consoleValue,
    setConsoleValue,
    setSuggestVisible,
    setSuggestActive,
    consoleMatches,
    acceptSuggestion,
    submitConsole,
    onConsoleKey,
  };

  // ── Dispatch by node type ─────────────────────────────────────────────
  // Layout owns the `unknown` fallback so anything unrecognised lands
  // there.
  const CARD_TYPES    = new Set(['card_item', 'list']);
  const FIELD_TYPES   = new Set(['field', 'color_field', 'vec_field', 'entity_ref']);
  const VIZ_TYPES     = new Set(['sparkline', 'chart', 'state_graph']);
  const CONSOLE_TYPES = new Set(['console_input', 'code']);
</script>

<div class="plugin-panel">
  {#if bottomMode}
    <BottomPanelHeader title={title || pluginName} />
  {:else if title}
    <div class="panel-header">
      <span class="panel-title">{title}</span>
    </div>
  {/if}

  {#snippet renderNode(node: any, i: number)}
    {@const t = node?.type}
    {#if CARD_TYPES.has(t)}
      <SidebarNodeCard {node} {ctx} />
    {:else if FIELD_TYPES.has(t)}
      <SidebarNodeField {node} index={i} {ctx} />
    {:else if VIZ_TYPES.has(t)}
      <SidebarNodeViz {node} {ctx} />
    {:else if CONSOLE_TYPES.has(t)}
      <SidebarNodeConsole {node} index={i} {ctx} />
    {:else}
      <SidebarNodeLayout {node} index={i} {ctx} {renderNode} />
    {/if}
  {/snippet}

  <div class="panel-body">
    {#if !content}
      <div class="panel-empty">Waiting for content…</div>
    {:else if Array.isArray(nodes) && nodes.length === 0}
      <div class="panel-empty">No content.</div>
    {:else}
      {#each nodes as node, i (nodeKey(node, i))}
        {@render renderNode(node, i)}
      {/each}
    {/if}
  </div>

  {#if Array.isArray(actions) && actions.length > 0}
    <div class="panel-footer">
      {#each actions as a, i (`${a.action ?? a.label ?? 'action'}:${i}`)}
        <button class="node-button footer-button" onclick={() => fireAction(a.action, {})}>
          {#if a.icon}
            <PluginIcon name={a.icon} size={13} class="node-icon" />
          {/if}
          <span>{a.label ?? 'Action'}</span>
        </button>
      {/each}
    </div>
  {/if}
</div>
