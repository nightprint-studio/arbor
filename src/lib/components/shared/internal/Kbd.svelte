<script lang="ts">
  /**
   * Canonical shortcut/keybinding display widget.
   *
   * Resolution priority (first non-empty wins):
   *   1. `action`  — built-in action id, looked up live in keybindingsStore
   *                  (so user remaps in Settings → Keybindings flow through)
   *   2. `binding` — explicit Keybinding object (e.g. plugin-registered)
   *   3. `keys`    — array of chord parts ['Ctrl','K']
   *   4. `label`   — single string ("Ctrl+K"), split on '+'
   *   5. `children`— free-form snippet (the legacy escape hatch)
   *
   * When `action` or `binding` is supplied but resolves to nothing (action
   * not in DEFAULT_KEYBINDINGS, or empty key), the widget renders nothing
   * — callers can drop `<Kbd action="…" />` next to a label without
   *   guarding it.
   *
   * Variants:
   *   - "box"    (default) — boxed <kbd> badges, one per chord part.
   *                          Suitable for help/docs/settings/footer hints.
   *   - "inline"            — plain monospace text, IntelliJ-menu style:
   *                          right-aligned ash-grey, no border, no bg.
   *                          Suitable for menu rows and tooltips.
   */
  import type { Snippet } from 'svelte';
  import { keybindingsStore } from '$lib/stores/keybindings.svelte';
  import { formatBinding, type Keybinding } from '$lib/utils/keybindings';

  type Size    = 'sm' | 'md';
  type Tone    = 'default' | 'accent' | 'muted';
  type Variant = 'box' | 'inline';

  interface Props {
    action?:  string;
    binding?: Keybinding | null;
    label?:   string;
    keys?:    string[];
    size?:    Size;
    tone?:    Tone;
    variant?: Variant;
    children?: Snippet;
  }

  let {
    action, binding, label, keys,
    size = 'md', tone = 'default', variant = 'box',
    children,
  }: Props = $props();

  // Live resolution — re-runs when keybindingsStore.custom changes.
  const resolvedLabel = $derived.by(() => {
    if (action) {
      const b = keybindingsStore.getBinding(action);
      return b && b.key ? formatBinding(b) : null;
    }
    if (binding && binding.key) return formatBinding(binding);
    return label ?? null;
  });

  const parts = $derived(
    keys ?? (resolvedLabel ? resolvedLabel.split('+').map(s => s.trim()).filter(Boolean) : []),
  );

  // Render-nothing guard: only kicks in for the action/binding paths so
  // legacy callers passing an empty <Kbd> still render an empty box.
  const hasResolvable = $derived(!!action || !!binding);
  const empty         = $derived(hasResolvable && !children && parts.length === 0);
</script>

{#if !empty}
  {#if variant === 'inline'}
    <!-- IntelliJ-menu style: plain monospace muted text, no border, no bg. -->
    <span class="kbd-inline tone-{tone}">
      {#if children}{@render children()}{:else}{resolvedLabel ?? parts.join('+') ?? ''}{/if}
    </span>
  {:else if children}
    <kbd class="kbd sz-{size} tone-{tone}">{@render children()}</kbd>
  {:else if parts.length > 1}
    <span class="kbd-combo sz-{size}">
      {#each parts as p, i}
        {#if i > 0}<span class="kbd-plus" aria-hidden="true">+</span>{/if}
        <kbd class="kbd sz-{size} tone-{tone}">{p}</kbd>
      {/each}
    </span>
  {:else}
    <kbd class="kbd sz-{size} tone-{tone}">{parts[0] ?? label ?? ''}</kbd>
  {/if}
{/if}

<style>
  .kbd {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    font-family: var(--font-code);
    background: var(--bg-overlay);
    border: 1px solid var(--border);
    border-bottom-width: 2px;
    border-radius: var(--radius-sm);
    color: var(--text-primary);
    white-space: nowrap;
    transition: border-color var(--transition-fast), color var(--transition-fast),
                background var(--transition-fast);
  }

  .sz-sm { font-size: 10px; padding: 0 5px; min-width: 18px; height: 17px; }
  .sz-md { font-size: 11px; padding: 1px 7px; min-width: 20px; height: 19px; }

  .tone-accent {
    background: var(--accent-subtle);
    border-color: var(--accent);
    color: var(--accent);
  }
  .tone-muted {
    background: transparent;
    border-color: var(--border-subtle);
    color: var(--text-muted);
  }

  .kbd-combo {
    display: inline-flex;
    align-items: center;
    gap: 3px;
  }
  .kbd-plus {
    color: var(--text-muted);
    font-size: 10px;
    user-select: none;
  }

  /* Inline (IntelliJ-menu) variant — plain monospace, right-aligned by parent. */
  .kbd-inline {
    font-family: var(--font-code);
    font-size: 11px;
    color: var(--text-muted);
    white-space: nowrap;
    letter-spacing: 0.2px;
  }
  .kbd-inline.tone-accent { color: var(--accent); }
  .kbd-inline.tone-muted  { color: var(--text-disabled); }
</style>
