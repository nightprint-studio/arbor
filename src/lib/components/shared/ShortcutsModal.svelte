<!--
  Keyboard Shortcuts — the single home for keybindings. A searchable AND
  editable reference, opened from the gear menu, the Command Palette, or the
  open_shortcuts shortcut (Shift+F1). There is intentionally no separate
  read-only cheat-sheet vs. editor (IntelliJ/VSCode model): you search and
  rebind in the same place.

  Data-driven from a single source of truth: built-in rows come straight from
  DEFAULT_KEYBINDINGS, resolved live through keybindingsStore (so a rebind here
  reflects everywhere instantly); plugin rows come from the `arbor:keybinding`
  contribution point and are read-only (owned by the plugin).

  The deep DocsPanel "Keyboard Shortcuts" page remains the prose reference and
  additionally documents contextual, non-rebindable keys (graph navigation,
  diff viewer, file picker) — reachable via the footer "Full reference" link.
-->
<script lang="ts">
  import {
    Keyboard, BookOpen, RotateCcw, Info,
    Navigation, LayoutDashboard, PanelLeft, GitBranch, Terminal, Puzzle,
  } from 'lucide-svelte';
  import Modal       from './Modal.svelte';
  import ModalHeader from './ModalHeader.svelte';
  import ModalFooter from './ModalFooter.svelte';
  import SearchBar   from './ui/SearchBar.svelte';
  import Button      from './ui/Button.svelte';
  import Kbd         from './internal/Kbd.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { uiStore }           from '$lib/stores/ui.svelte';
  import { keybindingsStore }  from '$lib/stores/keybindings.svelte';
  import { contributionStore } from '$lib/stores/corvus/contribution.svelte';
  import { pluginStore }       from '$lib/stores/plugin.svelte';
  import {
    DEFAULT_KEYBINDINGS, GROUP_ORDER, formatBinding, type Keybinding,
  } from '$lib/utils/keybindings';

  interface Props { onClose: () => void; }
  let { onClose }: Props = $props();

  type Row = {
    /** Built-in action id, or `<plugin>:<action>` for plugin rows. */
    id:          string;
    description: string;
    binding:     Keybinding;
    /** Formatted combo ("Ctrl+Shift+F") — used for substring filtering. */
    combo:       string;
    /** Set on plugin rows so the source plugin can be shown + searched. */
    plugin?:     string;
  };
  type Group = { label: string; rows: Row[] };

  let query = $state('');

  // Built-in bindings grouped by their `group`, resolved live so a rebind
  // flows through without a reload.
  const builtinGroups = $derived.by<Group[]>(() => {
    const byGroup = new Map<string, Row[]>();
    for (const [id, def] of Object.entries(DEFAULT_KEYBINDINGS)) {
      const b = keybindingsStore.getBinding(id);
      if (!b || !b.key) continue;
      const label = b.group || def.group || 'Other';
      const rows  = byGroup.get(label) ?? [];
      rows.push({ id, description: def.description, binding: b, combo: formatBinding(b) });
      byGroup.set(label, rows);
    }
    // Canonical groups first (GROUP_ORDER), then any stragglers alphabetically.
    const known = GROUP_ORDER as readonly string[];
    const order = [
      ...known.filter(g => byGroup.has(g)),
      ...[...byGroup.keys()].filter(g => !known.includes(g)).sort(),
    ];
    return order.map(label => ({
      label,
      rows: byGroup.get(label)!.sort((a, b) => a.description.localeCompare(b.description)),
    }));
  });

  // Plugin-registered shortcuts (enabled plugins only), as a trailing group.
  const pluginGroup = $derived.by<Group | null>(() => {
    const rows = contributionStore.forPoint('arbor:keybinding')
      .filter(c => !pluginStore.disabledPlugins.has(c.plugin_name))
      .map((c): Row => {
        const p = c.payload as { key?: string; ctrl?: boolean; shift?: boolean; alt?: boolean; action?: string; description?: string };
        const binding: Keybinding = {
          key: p.key ?? '', ctrl: !!p.ctrl, shift: !!p.shift, alt: !!p.alt,
          description: p.description ?? '', group: 'Plugins',
        };
        return {
          id:          c.plugin_name + ':' + (p.action ?? ''),
          description: p.description || p.action || '(plugin action)',
          binding,
          combo:       binding.key ? formatBinding(binding) : '',
          plugin:      c.plugin_name,
        };
      })
      .filter(r => !!r.binding.key)
      .sort((a, b) => a.plugin!.localeCompare(b.plugin!) || a.description.localeCompare(b.description));
    return rows.length ? { label: 'Plugins', rows } : null;
  });

  const allGroups = $derived<Group[]>(pluginGroup ? [...builtinGroups, pluginGroup] : builtinGroups);

  // Substring filter across description, combo, plugin name and group label.
  const filtered = $derived.by<Group[]>(() => {
    const q = query.trim().toLowerCase();
    if (!q) return allGroups;
    return allGroups
      .map(g => ({
        label: g.label,
        rows: g.rows.filter(r =>
          r.description.toLowerCase().includes(q) ||
          r.combo.toLowerCase().includes(q) ||
          (r.plugin?.toLowerCase().includes(q) ?? false) ||
          g.label.toLowerCase().includes(q),
        ),
      }))
      .filter(g => g.rows.length > 0);
  });

  const totalShown = $derived(filtered.reduce((n, g) => n + g.rows.length, 0));

  // ── Rebinding (built-in rows only) ────────────────────────────────────────
  // Captures the next key combination at the capture phase so it never leaks
  // into the search field or the global shortcut handler. Mirrors the logic
  // the Settings → Keybindings editor used before it was folded into here.
  let recordingAction = $state<string | null>(null);
  let captureHandler: ((e: KeyboardEvent) => void) | null = null;

  function startRecording(action: string) {
    stopRecording();
    recordingAction = action;
    captureHandler  = onCaptureKey;
    window.addEventListener('keydown', captureHandler, { capture: true });
  }

  function onCaptureKey(e: KeyboardEvent) {
    if (['Control', 'Shift', 'Alt', 'Meta', 'CapsLock'].includes(e.key)) return;
    e.preventDefault();
    e.stopImmediatePropagation();

    const action = recordingAction!;
    if (e.key === 'Escape') { stopRecording(); return; }

    const binding: Keybinding = {
      key:         e.key,
      description: DEFAULT_KEYBINDINGS[action]?.description ?? '',
      group:       DEFAULT_KEYBINDINGS[action]?.group ?? '',
    };
    if (e.ctrlKey || e.metaKey) binding.ctrl  = true;
    if (e.shiftKey)             binding.shift = true;
    if (e.altKey)               binding.alt   = true;

    keybindingsStore.setBinding(action, binding);
    stopRecording();
  }

  function stopRecording() {
    if (captureHandler) {
      window.removeEventListener('keydown', captureHandler, true);
      captureHandler = null;
    }
    recordingAction = null;
  }

  $effect(() => () => stopRecording());

  function openFullDocs() { onClose(); uiStore.setPanel('docs'); }

  // Per-group glyph — gives each card a scannable identity instead of a bare
  // label. Falls back to the generic keyboard glyph for unknown groups
  // (e.g. future categories or oddly-named plugin groups).
  function groupIcon(label: string): typeof Keyboard {
    switch (label) {
      case 'Navigation':        return Navigation;
      case 'Panels':            return LayoutDashboard;
      case 'Sidebar Sections':  return PanelLeft;
      case 'Git':               return GitBranch;
      case 'Terminal':          return Terminal;
      case 'Plugins':           return Puzzle;
      default:                  return Keyboard;
    }
  }
</script>

<Modal {onClose} width="840px" height="660px" padBody={false} ariaLabel="Keyboard shortcuts">
  {#snippet header()}
    <ModalHeader {onClose}>
      <Keyboard size={14} />
      <span class="modal-title">Keyboard Shortcuts</span>
    </ModalHeader>
  {/snippet}

  <div class="sc-body">
    <div class="sc-toolbar">
      <SearchBar
        bind:query
        showRegex={false}
        showCounter={false}
        placeholder="Filter shortcuts…"
        ariaLabel="Filter keyboard shortcuts"
        autofocus
      />
      <div class="sc-hint">
        <Info size={12} />
        <span>Click a shortcut to rebind it. <kbd class="sc-kbd-inline">Escape</kbd> closes panels and isn't rebindable.</span>
      </div>
    </div>

    <div class="sc-scroll">
      {#if totalShown === 0}
        <div class="sc-empty">
          <Keyboard size={28} strokeWidth={1.5} />
          <p>No shortcut matches “{query.trim()}”.</p>
        </div>
      {:else}
        <div class="sc-grid">
          {#each filtered as group (group.label)}
            {@const GroupIcon = groupIcon(group.label)}
            <section class="sc-card">
              <div class="sc-card-head">
                <span class="sc-card-icon"><GroupIcon size={13} /></span>
                <h3 class="sc-card-title">{group.label}</h3>
                <span class="sc-card-count">{group.rows.length}</span>
              </div>
              <ul class="sc-rows">
                {#each group.rows as row (row.id)}
                  {@const customized = !row.plugin && keybindingsStore.isCustomized(row.id)}
                  {@const isRecording = recordingAction === row.id}
                  <li class="sc-row" class:recording={isRecording}>
                    <span class="sc-desc">
                      {row.description}
                      {#if row.plugin}<span class="sc-plugin">{row.plugin}</span>{/if}
                    </span>

                    <div class="sc-keys">
                      <span class="sc-keys-inner">
                        {#if row.plugin}
                          <!-- Plugin bindings are owned by the plugin — read-only. -->
                          <span class="sc-readonly" use:tooltip={`Registered by plugin: ${row.plugin}`}>
                            <Kbd binding={row.binding} size="sm" />
                          </span>
                        {:else}
                          <button
                            class="sc-chip"
                            class:is-recording={isRecording}
                            onclick={() => isRecording ? stopRecording() : startRecording(row.id)}
                            use:tooltip={isRecording ? 'Press a key combination — Escape to cancel' : 'Click to rebind'}
                          >
                            {#if isRecording}
                              <span class="sc-recording-label">Press a shortcut…</span>
                            {:else}
                              <Kbd action={row.id} size="sm" tone={customized ? 'accent' : 'default'} />
                            {/if}
                          </button>
                          <span class="sc-reset-cell">
                            {#if customized}
                              <button
                                class="sc-reset"
                                onclick={() => keybindingsStore.resetBinding(row.id)}
                                use:tooltip={`Reset to default: ${formatBinding(DEFAULT_KEYBINDINGS[row.id])}`}
                                aria-label="Reset to default"
                              >
                                <RotateCcw size={11} />
                              </button>
                            {/if}
                          </span>
                        {/if}
                      </span>
                    </div>
                  </li>
                {/each}
              </ul>
            </section>
          {/each}
        </div>
      {/if}
    </div>
  </div>

  {#snippet footer()}
    <ModalFooter align="between">
      <span class="sc-footer-note">Changes are saved automatically and apply everywhere instantly.</span>
      <div class="sc-actions">
        <Button variant="ghost" size="sm" onclick={() => keybindingsStore.resetAll()}
                tooltip={'Reset every shortcut to its default'}>
          {#snippet iconStart()}<RotateCcw size={13} />{/snippet}
          Reset all
        </Button>
        <Button variant="ghost" size="sm" onclick={openFullDocs}>
          {#snippet iconStart()}<BookOpen size={13} />{/snippet}
          Full reference
        </Button>
      </div>
    </ModalFooter>
  {/snippet}
</Modal>

<style>
  .sc-body {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
  }

  /* Sticky search + hint — stays put while the grid scrolls beneath it. */
  .sc-toolbar {
    flex-shrink: 0;
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 12px 16px;
    border-bottom: 1px solid var(--border-subtle);
  }
  .sc-hint {
    display: flex;
    align-items: center;
    gap: 6px;
    font-family: var(--font-ui-sans);
    font-size: var(--font-size-xs);
    color: var(--text-muted);
  }
  .sc-kbd-inline {
    font-family: var(--font-code);
    font-size: 10px;
    background: var(--bg-overlay);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    padding: 0 4px;
    color: var(--text-secondary);
  }

  .sc-scroll {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 16px;
    scrollbar-width: thin;
    scrollbar-color: var(--scrollbar-thumb) transparent;
  }
  .sc-scroll::-webkit-scrollbar       { width: var(--scrollbar-width); }
  .sc-scroll::-webkit-scrollbar-track { background: transparent; }
  .sc-scroll::-webkit-scrollbar-thumb {
    background: var(--scrollbar-thumb);
    border-radius: var(--scrollbar-radius);
  }
  .sc-scroll::-webkit-scrollbar-thumb:hover { background: var(--scrollbar-thumb-hover); }

  /* Cards size to ~330px and wrap, aligned to top so a short card never
     stretches to match a tall neighbour. */
  .sc-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(330px, 1fr));
    gap: 12px;
    align-items: start;
  }

  .sc-card {
    background: var(--bg-base);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    padding: 10px 12px 12px;
  }

  .sc-card-head {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 0 2px 8px;
    margin-bottom: 5px;
    border-bottom: 1px solid var(--border-subtle);
  }
  .sc-card-icon {
    display: inline-flex;
    color: var(--accent);
    flex-shrink: 0;
  }
  .sc-card-title {
    flex: 1;
    margin: 0;
    font-family: var(--font-ui-sans);
    font-size: 12px;
    font-weight: 600;
    letter-spacing: 0.01em;
    color: var(--text-primary);
  }
  .sc-card-count {
    flex-shrink: 0;
    font-family: var(--font-ui-sans);
    font-size: 10px;
    font-variant-numeric: tabular-nums;
    color: var(--text-muted);
    background: var(--bg-overlay);
    border-radius: 999px;
    padding: 1px 7px;
  }

  /* CSS table so the shortcut column lines up natively on a single left edge
     across the whole card — no subgrid / modern-CSS dependency, rock-solid
     column alignment everywhere. The description cell is greedy (width:100%)
     so the shortcut cell shrinks to its content and every combo starts at the
     same x. */
  .sc-rows {
    list-style: none;
    margin: 0;
    padding: 0;
    display: table;
    width: 100%;
    border-collapse: collapse;
  }

  .sc-row { display: table-row; }
  .sc-row:hover .sc-desc,
  .sc-row:hover .sc-keys { background: var(--bg-hover); }
  .sc-row.recording .sc-desc,
  .sc-row.recording .sc-keys { background: var(--accent-subtle); }

  .sc-desc {
    display: table-cell;
    width: 100%;
    vertical-align: middle;
    padding: 4px 14px 4px 6px;
    font-family: var(--font-ui-sans);
    font-size: var(--font-size-xs);
    color: var(--text-secondary);
    line-height: 1.35;
  }
  /* The plugin chip sits inline after the description text. */
  .sc-desc :global(.sc-plugin) { margin-left: 6px; }

  .sc-keys {
    display: table-cell;
    vertical-align: middle;
    white-space: nowrap;
    padding: 3px 6px 3px 0;
  }
  /* Inner flex keeps the chip + reset spacing; the cell itself owns alignment. */
  .sc-keys-inner {
    display: inline-flex;
    align-items: center;
    gap: 4px;
  }

  /* Plugin attribution chip on plugin-contributed rows. */
  .sc-plugin {
    flex-shrink: 0;
    font-size: 9px;
    font-weight: 600;
    letter-spacing: 0.03em;
    text-transform: uppercase;
    color: var(--text-muted);
    background: var(--bg-overlay);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    padding: 1px 5px;
  }

  .sc-readonly { display: inline-flex; opacity: 0.8; }

  .sc-chip {
    display: inline-flex;
    align-items: center;
    justify-content: flex-start;
    min-width: 72px;
    padding: 2px 5px;
    background: transparent;
    border: none;
    border-radius: var(--radius-sm);
    cursor: pointer;
    transition: background var(--transition-fast);
  }
  .sc-chip:hover:not(.is-recording) { background: var(--bg-overlay); }
  .sc-chip.is-recording { cursor: default; }

  .sc-recording-label {
    font-family: var(--font-ui-sans);
    font-size: 11px;
    font-style: italic;
    color: var(--accent);
    white-space: nowrap;
    animation: sc-pulse 1.1s ease-in-out infinite;
  }
  @keyframes sc-pulse {
    0%, 100% { opacity: 1; }
    50%      { opacity: 0.5; }
  }

  .sc-reset-cell {
    width: 20px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
  }
  .sc-reset {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 18px;
    height: 18px;
    background: transparent;
    border: none;
    border-radius: var(--radius-sm);
    cursor: pointer;
    color: var(--text-disabled);
    transition: color var(--transition-fast), background var(--transition-fast);
  }
  .sc-reset:hover {
    color: var(--warning);
    background: color-mix(in srgb, var(--warning) 12%, transparent);
  }

  .sc-empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 10px;
    padding: 64px 16px;
    color: var(--text-disabled);
  }
  .sc-empty p {
    margin: 0;
    font-family: var(--font-ui-sans);
    font-size: var(--font-size-sm);
  }

  .sc-footer-note {
    font-family: var(--font-ui-sans);
    font-size: var(--font-size-xs);
    color: var(--text-muted);
  }
  .sc-actions { display: flex; gap: 6px; }
</style>
