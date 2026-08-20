<script module lang="ts">
  import type { IconComponent } from '$lib/types/icon';

  /** One row in the editor: a rail button, as something you can drag and hide. */
  export interface RailEditorRow {
    id: string;
    label: string;
    /** A lucide component, or a short string (an emoji glyph) to render as text. */
    icon?: IconComponent | string;
    visible: boolean;
    /** Always visible and never draggable — see the note on `mandatory` below. */
    mandatory: boolean;
  }

  /** A cluster of the bar: the top group, the bottom group. */
  export interface RailEditorSection {
    id: string;
    /** Uppercase heading. */
    label: string;
    /** The small grey line beside it — what this cluster of the bar actually drives. */
    hint?: string;
    items: RailEditorRow[];
  }

  /** One bar. Rendered as a tab when there is more than one. */
  export interface RailEditorTab {
    id: string;
    label: string;
    /** A sentence above the sections, for whatever is peculiar to this bar. */
    hint?: string;
    sections: RailEditorSection[];
  }
</script>

<script lang="ts">
  /**
   * Rearranging an icon rail — the dialog, for any product that has one.
   *
   * ## What it does and does not know
   *
   * It knows about rows, sections and bars. It does not know what a row *is*: which of them
   * come from plugins, which are gated on a capability, how a label is derived, where the
   * result is persisted. All of that differs per product and all of it is the caller's, which
   * is what lets Corvus's four bars-with-plugins and Bennu's four bars-of-tool-windows be the
   * same dialog rather than two that drift apart a row at a time.
   *
   * ## The snapshot is deliberate
   *
   * `tabs` is read **once**, on mount, into a working copy. It is not a live binding, and that
   * is not laziness: in Corvus the rows are derived from the plugin contribution store, which
   * mutates on every `arbor://contributions-changed` event — a scheduler tick from any
   * installed plugin. A reactive seed re-ran a few hundred milliseconds later and overwrote
   * whatever the user had just toggled, which made the dialog impossible to land an edit in.
   * Nothing is written anywhere until Save.
   *
   * ## `mandatory`
   *
   * A rail every button of which can be hidden has a state with nothing left to click, and
   * therefore no way back. So a product marks the buttons that are its own way back; they
   * render locked rather than merely disabled, because a control that looks available and
   * does nothing is worse than one that says why.
   */
  import { GripVertical, Eye, EyeOff, Lock, Zap, RotateCcw } from 'lucide-svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { onMount } from 'svelte';

  let {
    tabs,
    title = 'Customize Activity Bar',
    onSave,
    onResetTab,
    onClose,
  }: {
    /** The bars to edit. Snapshotted on mount — see the note above. */
    tabs: RailEditorTab[];
    title?: string;
    /** Persist. The dialog closes when it resolves. */
    onSave: (tabs: RailEditorTab[]) => Promise<void> | void;
    /** The default arrangement of one bar — everything visible, natural order. */
    onResetTab?: (tabId: string) => RailEditorSection[];
    onClose: () => void;
  } = $props();

  let working = $state<RailEditorTab[]>([]);
  let activeId = $state('');

  onMount(() => {
    working = tabs.map((t) => ({
      ...t,
      sections: t.sections.map((s) => ({ ...s, items: s.items.map((i) => ({ ...i })) })),
    }));
    activeId = working[0]?.id ?? '';
  });

  const active = $derived(working.find((t) => t.id === activeId) ?? null);

  // ── Drag and drop ──────────────────────────────────────────────────────────
  // The same mouse-event pattern the rest of the app reorders with (TitleBar, BranchTree):
  // a 4px threshold before anything moves, so a click on the handle is still a click.

  let dragState = $state<{ section: string; fromIndex: number; insertBefore: number } | null>(null);
  // `$state`: `bind:this` into a plain object writes a property Svelte is not tracking, which
  // it warns about — and rightly, because the write would be invisible to anything deriving
  // from it. Nothing derives from it today (the drag reads it imperatively, long after the
  // bind has landed), but a silent non-reactive binding is the wrong thing to leave lying
  // around in a widget other products build on.
  let listEls = $state<Record<string, HTMLElement | undefined>>({});

  function startDrag(e: MouseEvent, sectionId: string, fromIndex: number) {
    if (e.button !== 0) return;
    const startY = e.clientY;
    let engaged = false;

    function onMove(ev: MouseEvent) {
      if (!engaged) {
        if (Math.abs(ev.clientY - startY) < 4) return;
        engaged = true;
        dragState = { section: sectionId, fromIndex, insertBefore: fromIndex };
        document.body.style.cursor = 'grabbing';
      }
      if (!dragState) return;
      dragState = { ...dragState, insertBefore: insertIndexAt(ev.clientY, listEls[sectionId]) };
    }

    function onUp() {
      window.removeEventListener('mousemove', onMove);
      window.removeEventListener('mouseup', onUp);
      document.body.style.cursor = '';
      const drag = dragState;
      dragState = null;
      if (!engaged || !drag) return;
      const to = drag.insertBefore <= drag.fromIndex ? drag.insertBefore : drag.insertBefore - 1;
      if (to === drag.fromIndex) return;
      mutate(sectionId, (items) => {
        const next = [...items];
        const [moved] = next.splice(drag.fromIndex, 1);
        next.splice(to, 0, moved);
        return next;
      });
    }

    window.addEventListener('mousemove', onMove);
    window.addEventListener('mouseup', onUp);
  }

  function insertIndexAt(y: number, listEl: HTMLElement | undefined): number {
    if (!listEl) return 0;
    const rows = listEl.querySelectorAll<HTMLElement>('[data-drag-idx]');
    for (let i = 0; i < rows.length; i++) {
      const r = rows[i].getBoundingClientRect();
      if (y < r.top + r.height / 2) return i;
    }
    return rows.length;
  }

  /** Replace one section's items in the working copy. */
  function mutate(sectionId: string, f: (items: RailEditorRow[]) => RailEditorRow[]) {
    working = working.map((tab) =>
      tab.id !== activeId
        ? tab
        : {
            ...tab,
            sections: tab.sections.map((s) =>
              s.id === sectionId ? { ...s, items: f(s.items) } : s,
            ),
          },
    );
  }

  function toggleVisibility(sectionId: string, idx: number) {
    mutate(sectionId, (items) =>
      items.map((it, i) => (i === idx && !it.mandatory ? { ...it, visible: !it.visible } : it)),
    );
  }

  function resetActive() {
    if (!onResetTab || !active) return;
    const sections = onResetTab(active.id);
    working = working.map((tab) => (tab.id === activeId ? { ...tab, sections } : tab));
  }

  let saving = $state(false);
  async function save() {
    saving = true;
    try {
      await onSave(working);
      onClose();
    } finally {
      saving = false;
    }
  }

  function isGlyph(icon: unknown): icon is string {
    return typeof icon === 'string';
  }
</script>

<Modal {onClose} width="420px" height="80vh" padBody={false} ariaLabel={title}>
  {#snippet header()}
    <ModalHeader {title} {onClose} />
  {/snippet}

  <div class="cr-body">
    {#if working.length > 1}
      <div class="tabs" role="tablist">
        {#each working as tab (tab.id)}
          <button
            class="tab-btn"
            class:tab-active={tab.id === activeId}
            role="tab"
            aria-selected={tab.id === activeId}
            onclick={() => (activeId = tab.id)}
          >{tab.label}</button>
        {/each}
      </div>
    {/if}

    <div class="cr-content">
      <p class="hint">
        {#if active?.hint}{active.hint}<br>{/if}
        Drag to reorder · click the eye to show/hide ·
        <Lock size={10} class="hint-lock" /> locked items are always visible.
      </p>

      {#each active?.sections ?? [] as section (section.id)}
        <div class="section">
          <div class="section-label">
            <span>{section.label}</span>
            {#if section.hint}<span class="section-label-hint">{section.hint}</span>{/if}
          </div>
          <div class="item-list" bind:this={listEls[section.id]}>
            {#if section.items.length === 0}
              <p class="section-empty">Nothing on this part of the bar for this project.</p>
            {/if}
            {#each section.items as item, i (item.id)}
              {#if dragState?.section === section.id && dragState.insertBefore === i}
                <div class="drop-indicator" aria-hidden="true"></div>
              {/if}
              <div
                class="item"
                class:item-hidden={!item.visible}
                class:item-dragging={dragState?.section === section.id && dragState.fromIndex === i}
                data-drag-idx={i}
              >
                <button
                  class="drag-handle"
                  class:drag-locked={item.mandatory}
                  onmousedown={(e) => !item.mandatory && startDrag(e, section.id, i)}
                  disabled={item.mandatory}
                  use:tooltip={item.mandatory ? 'Locked — cannot be reordered' : 'Drag to reorder'}
                  aria-label={item.mandatory ? 'Locked' : 'Drag to reorder'}
                >
                  <GripVertical size={14} />
                </button>

                <span class="item-icon">
                  {#if isGlyph(item.icon)}
                    <span class="glyph-icon">{item.icon}</span>
                  {:else if item.icon}
                    {@const IconComp = item.icon}
                    <IconComp size={16} />
                  {:else}
                    <Zap size={16} />
                  {/if}
                </span>

                <span class="item-label">{item.label}</span>

                {#if item.mandatory}
                  <span class="lock-icon" use:tooltip={'Always visible'}><Lock size={13} /></span>
                {:else}
                  <button
                    class="vis-btn"
                    class:vis-hidden={!item.visible}
                    onclick={() => toggleVisibility(section.id, i)}
                    use:tooltip={item.visible ? 'Hide' : 'Show'}
                    aria-label={item.visible ? 'Hide item' : 'Show item'}
                  >
                    {#if item.visible}<Eye size={14} />{:else}<EyeOff size={14} />{/if}
                  </button>
                {/if}
              </div>
            {/each}
            {#if dragState?.section === section.id && dragState.insertBefore === section.items.length}
              <div class="drop-indicator" aria-hidden="true"></div>
            {/if}
          </div>
        </div>
      {/each}
    </div>
  </div>

  {#snippet footer()}
    {#if onResetTab}
      <Button variant="ghost" onclick={resetActive} title={`Restore the default order on ${active?.label ?? 'this bar'}`}>
        {#snippet iconStart()}<RotateCcw size={14} />{/snippet}
        Restore defaults
      </Button>
    {/if}
    <span class="footer-spacer"></span>
    <Button variant="secondary" onclick={onClose}>Cancel</Button>
    <Button variant="primary" onclick={save} disabled={saving} loading={saving}>
      {saving ? 'Saving…' : 'Save'}
    </Button>
  {/snippet}
</Modal>

<style>
  .cr-body {
    height: 100%;
    display: flex;
    flex-direction: column;
    font-family: var(--font-ui-sans);
  }

  /* Tabs — equal halves, centered, accent underline on the active one. */
  .tabs {
    display: flex;
    align-items: stretch;
    width: 100%;
    background: transparent;
    border-bottom: 1px solid var(--border-subtle);
    flex-shrink: 0;
  }
  .tab-btn {
    flex: 1 1 0;
    min-width: 0;
    height: 36px;
    display: flex;
    align-items: center;
    justify-content: center;
    background: transparent;
    border: none;
    color: var(--text-secondary);
    font-family: var(--font-ui-sans);
    font-size: var(--font-size-sm);
    font-weight: 600;
    letter-spacing: 0.02em;
    cursor: pointer;
    position: relative;
    transition: color var(--transition-fast), background var(--transition-fast);
  }
  .tab-btn:hover:not(.tab-active) { background: var(--bg-hover); color: var(--text-primary); }
  .tab-btn.tab-active { color: var(--accent); }
  .tab-btn.tab-active::after {
    content: '';
    position: absolute;
    left: 20%;
    right: 20%;
    bottom: -1px;
    height: 2px;
    background: var(--accent);
    border-radius: 2px 2px 0 0;
  }

  /* The footer justifies to the end, so this eats the space between Restore and Cancel. */
  .footer-spacer { flex: 1; }

  .cr-content {
    flex: 1;
    overflow-y: auto;
    padding: 14px 16px;
    display: flex;
    flex-direction: column;
    gap: 18px;
  }

  .hint {
    font-size: var(--font-size-xs);
    color: var(--text-muted);
    margin: 0;
    line-height: 1.6;
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 4px;
  }
  :global(.hint-lock) {
    color: var(--text-disabled);
    vertical-align: -1px;
  }

  .section { display: flex; flex-direction: column; gap: 4px; }
  .section-label {
    display: flex;
    align-items: baseline;
    gap: 8px;
    font-size: var(--font-size-2xs);
    font-weight: 600;
    letter-spacing: 0.6px;
    text-transform: uppercase;
    color: var(--text-muted);
    padding: 0 2px;
    margin-bottom: 2px;
  }
  .section-label-hint {
    font-weight: 400;
    letter-spacing: 0;
    text-transform: none;
    color: var(--text-disabled);
    font-size: var(--font-size-2xs);
  }
  .section-empty {
    margin: 0;
    padding: 6px 4px;
    font-size: var(--font-size-xs);
    color: var(--text-disabled);
  }

  .item-list { display: flex; flex-direction: column; gap: 2px; }

  .item {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 8px 6px 4px;
    border-radius: var(--radius-sm);
    background: transparent;
    border: 1px solid var(--border-subtle);
    transition: background var(--transition-fast), border-color var(--transition-fast), opacity var(--transition-fast);
    user-select: none;
  }
  .item:hover { background: var(--bg-hover); border-color: var(--border); }
  .item.item-hidden { opacity: 0.45; }
  .item.item-dragging { opacity: 0.3; border-style: dashed; border-color: var(--border); }

  .drag-handle {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 20px;
    height: 20px;
    background: transparent;
    border: none;
    color: var(--text-disabled);
    cursor: grab;
    border-radius: var(--radius-sm);
    flex-shrink: 0;
    padding: 0;
    transition: color var(--transition-fast);
  }
  .drag-handle:hover:not(:disabled) { color: var(--text-muted); }
  .drag-handle:active:not(:disabled) { cursor: grabbing; }
  .drag-handle.drag-locked { opacity: 0.25; cursor: not-allowed; }
  .drag-handle.drag-locked:hover { color: var(--text-disabled); }

  .item-icon {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 20px;
    height: 20px;
    color: var(--text-secondary);
    flex-shrink: 0;
  }
  .glyph-icon { font-size: var(--font-size-lg); line-height: 1; }

  .item-label {
    flex: 1;
    font-size: var(--font-size-sm);
    color: var(--text-primary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    min-width: 0;
  }

  .vis-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 24px;
    height: 24px;
    background: transparent;
    border: none;
    border-radius: var(--radius-sm);
    color: var(--text-secondary);
    cursor: pointer;
    flex-shrink: 0;
    padding: 0;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .vis-btn:hover { background: var(--bg-hover); color: var(--text-primary); }
  .vis-btn.vis-hidden { color: var(--text-disabled); }
  .vis-btn.vis-hidden:hover { color: var(--text-muted); }

  .lock-icon {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 24px;
    height: 24px;
    color: var(--text-disabled);
    flex-shrink: 0;
  }

  .drop-indicator {
    height: 2px;
    background: var(--accent);
    border-radius: 1px;
    margin: 1px 0;
    pointer-events: none;
  }
</style>
