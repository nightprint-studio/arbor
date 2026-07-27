<script module lang="ts">
  import type { Snippet } from 'svelte';
  import type { DropdownItem } from './Dropdown.svelte';
  import type { TooltipInput } from '$lib/stores/tooltip.svelte';

  /**
   * One configurable button in the title bar's right cluster.
   *
   * Two flavours, picked by whether `menu` is set:
   *  • plain   → fires `onclick` (toggle a panel, open an overlay, …)
   *  • menu    → opens a `Dropdown` built from `menu` items
   */
  export interface TitleBarButton {
    /** Persistent "lit" state — accent colour even when the menu is closed. */
    active?: boolean;
    /** Click handler (plain mode — ignored when `menu` is set). */
    onclick?: () => void;
    /** Tooltip text (or full tooltip options); defaults to the slot's canonical label. */
    tooltip?: TooltipInput;
    /** Lucide component override; falls back to the slot's canonical icon. */
    icon?: any;
    /** When set, the button opens a dropdown built from these items. */
    menu?: DropdownItem[];
    /** CSS width for the dropdown panel (default `220px`). */
    menuWidth?: string;
    /** Upper cap on the dropdown panel height (lets long lists scroll). */
    menuMaxHeight?: number;
    /** Accessible label override (defaults to the canonical label). */
    ariaLabel?: string;
  }
</script>

<script lang="ts">
  /**
   * TitleBar — app-agnostic window chrome shell.
   *
   * Renders the standard top bar skeleton (drag region · brand · hamburger ·
   * leading content · spacer · right cluster · window controls) and leaves
   * every Arbor/merula-specific piece to the consumer:
   *
   *  • `logo`            — app mark snippet (omit → no brand slot)
   *  • `menu`/`hamburger`— structured `DropdownItem[]` OR a custom control
   *                        (omit both → no hamburger)
   *  • `onNativeMenu`    — macOS: publish `menu` to the system menu bar instead
   *                        of drawing a hamburger (the host owns the IPC)
   *  • `leading`         — free content after the hamburger (workspace/project
   *                        switcher, plugin-left items, …)
   *  • `trailing`        — free content at the head of the right cluster
   *                        (run controls, status pills, layout toggles, …)
   *  • `actions`         — free-form buttons just before the named buttons
   *  • `docs` / `commandPalette` / `settings` — optional built-in buttons; each
   *                        omitted slot renders nothing. `settings` may carry a
   *                        `menu` to open a dropdown instead of firing onclick.
   *  • `settingsContent` — escape hatch: a fully custom settings control
   *                        (used when the menu needs a shape a flat
   *                        `DropdownItem[]` can't express)
   *  • `windowControls`  — min/max/close slot (consumer passes its own)
   *
   * No domain imports live here — only the shared `Dropdown`, lucide glyphs and
   * the tooltip action (both already standard inside `shared/ui/`).
   */
  import Dropdown from './Dropdown.svelte';
  import { AlignJustify, BookOpen, Command, Settings } from 'lucide-svelte';
  import { isMac } from '$lib/utils/platform';
  // Title bar sits at the very top — tooltips fly downward so they're never
  // clipped by the window edge.
  import { tooltipBottom as tooltip } from '$lib/actions/tooltip';

  interface Props {
    /** App mark at the far left. Omit → no brand slot. */
    logo?: Snippet;
    logoTooltip?: string;
    /** Structured hamburger menu. Omit (and no `hamburger`) → no hamburger. */
    menu?: DropdownItem[];
    menuWidth?: string;
    menuTooltip?: string;
    /** Escape hatch: a fully custom hamburger control (wins over `menu`). */
    hamburger?: Snippet;
    /**
     * macOS only: hand `menu` to the OS menu bar instead of rendering a
     * hamburger for it. Called with the live items on every change — the host
     * decides how to publish them (see `utils/native-menu`), keeping this
     * widget free of any IPC. Ignored off macOS and when `hamburger` is set.
     */
    onNativeMenu?: (items: DropdownItem[]) => void;
    /**
     * Whether this bar may claim the app-wide menu (macOS). Default true — set
     * it to false for a bar that is mounted but not on screen, so a host with
     * several bars alive at once (a tabbed container) doesn't have its
     * background products overwrite the foreground one's menus.
     */
    nativeMenuEnabled?: boolean;
    /** Free content after the hamburger, before the draggable spacer. */
    leading?: Snippet;
    /** Free content centred in the bar, between two draggable spacers — for
     *  things that belong to the window rather than to what's on its left or
     *  right (the container's product tabs). */
    center?: Snippet;
    /** Free content at the head of the right cluster. */
    trailing?: Snippet;
    /** Free-form buttons just before the named buttons. */
    actions?: Snippet;
    /** Built-in documentation toggle button. */
    docs?: TitleBarButton;
    /** Built-in command-palette toggle button. */
    commandPalette?: TitleBarButton;
    /** Built-in settings button (plain or dropdown). */
    settings?: TitleBarButton;
    /** Escape hatch replacing the structured settings button. */
    settingsContent?: Snippet;
    /** Window controls slot — consumer passes its own min/max/close. */
    windowControls?: Snippet;
    class?: string;
  }

  let {
    logo, logoTooltip, menu, menuWidth = '280px', menuTooltip = 'Main menu',
    hamburger, onNativeMenu, nativeMenuEnabled = true, leading, center, trailing, actions,
    docs, commandPalette, settings, settingsContent,
    windowControls, class: cls = '',
  }: Props = $props();

  /** macOS with a native-menu host: the bar replaces the hamburger entirely.
   *  `nativeMenuEnabled` lets a host with SEVERAL bars mounted at once (a tabbed
   *  container) keep the background ones from fighting over the app-wide menu. */
  const nativeMenu = $derived(
    isMac && !hamburger && !!onNativeMenu && !!menu && nativeMenuEnabled,
  );

  // Re-publish on every change of the items — and of anything the host reads
  // while deriving them (keybindings, for the accelerators).
  $effect(() => {
    if (nativeMenu && menu) onNativeMenu!(menu);
  });
</script>

{#snippet namedButton(btn: TitleBarButton, fallbackIcon: any, fallbackLabel: string)}
  {@const Icon = btn.icon ?? fallbackIcon}
  {#if btn.menu}
    <Dropdown
      items={btn.menu}
      position="fixed"
      direction="down"
      width={btn.menuWidth ?? '220px'}
      maxHeight={btn.menuMaxHeight}
    >
      {#snippet trigger({ open, toggle })}
        <button
          class="tb-icon"
          class:active={btn.active || open}
          onclick={toggle}
          use:tooltip={btn.tooltip ?? fallbackLabel}
          aria-label={btn.ariaLabel ?? fallbackLabel}
          aria-haspopup="menu"
          aria-expanded={open}
        >
          <Icon size={18} />
        </button>
      {/snippet}
    </Dropdown>
  {:else}
    <button
      class="tb-icon"
      class:active={btn.active}
      onclick={btn.onclick}
      use:tooltip={btn.tooltip ?? fallbackLabel}
      aria-label={btn.ariaLabel ?? fallbackLabel}
      aria-pressed={btn.active}
    >
      <Icon size={18} />
    </button>
  {/if}
{/snippet}

<div class="tb {cls}" data-tauri-drag-region role="banner">
  <!-- macOS: a hairline that divides the native traffic-light gutter from the
       app chrome. Sits at the start, right after the reserved gutter. -->
  {#if isMac}<div class="tb-mac-sep" aria-hidden="true"></div>{/if}

  {#if logo}
    <div class="tb-nodrag tb-brand" use:tooltip={logoTooltip ?? ''}>
      {@render logo()}
    </div>
  {/if}

  {#if hamburger}
    <div class="tb-nodrag">{@render hamburger()}</div>
  {:else if menu && !nativeMenu}
    <div class="tb-nodrag">
      <Dropdown items={menu} position="fixed" direction="down" width={menuWidth}>
        {#snippet trigger({ open, toggle })}
          <button
            class="tb-hamburger"
            class:active={open}
            onclick={toggle}
            use:tooltip={menuTooltip}
            aria-label="Open main menu"
            aria-haspopup="menu"
            aria-expanded={open}
          >
            <AlignJustify size={20} strokeWidth={2} />
          </button>
        {/snippet}
      </Dropdown>
    </div>
  {/if}

  {#if leading}
    <div class="tb-nodrag tb-leading">{@render leading()}</div>
  {/if}

  <!-- Draggable region so the user can grab the empty middle. With a `center`
       slot the middle is split in two so the slot sits centred between the
       leading content and the right cluster. -->
  <div class="tb-spacer" data-tauri-drag-region></div>

  {#if center}
    <div class="tb-nodrag tb-center">{@render center()}</div>
    <div class="tb-spacer" data-tauri-drag-region></div>
  {/if}

  <div class="tb-right tb-nodrag" class:tb-right-mac={isMac}>
    {#if trailing}{@render trailing()}{/if}
    {#if actions}{@render actions()}{/if}
    {#if docs}{@render namedButton(docs, BookOpen, 'Documentation')}{/if}
    {#if commandPalette}{@render namedButton(commandPalette, Command, 'Command palette')}{/if}
    {#if settingsContent}
      {@render settingsContent()}
    {:else if settings}
      {@render namedButton(settings, Settings, 'Settings')}
    {/if}
  </div>

  {#if windowControls}
    <!-- Its own flex child rather than a tail of `.tb-right`: the Mac trio has
         to be able to jump to the LEADING edge (`.window-controls-slot` in
         app.css flips its `order`), which it can't do from inside the right
         cluster. On macOS the slot renders nothing — the native traffic lights
         sit top-left instead — so the divider is suppressed with it. -->
    <div class="window-controls-slot">
      {#if !isMac}<div class="tb-sep"></div>{/if}
      {@render windowControls()}
    </div>
  {/if}
</div>

<style>
  .tb {
    display: flex;
    align-items: center;
    height: var(--titlebar-h, 42px);
    background: var(--bg-elevated);
    flex-shrink: 0;
    overflow: visible;
    position: relative;
    z-index: 100;
    box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.04);
    transition: height var(--anim-dur-base) ease;
    /* Clear the macOS native traffic lights (0 off macOS). */
    padding-left: var(--mac-traffic-gutter, 0);
  }

  /* Each interactive cluster opts out of the window-drag region and lays its
     children out inline. (Plain flex, not `display: contents`, so the wrapper
     keeps a measurable box for the Dropdown anchor + its own padding.) */
  .tb-nodrag {
    -webkit-app-region: no-drag;
    display: flex;
    align-items: center;
  }

  .tb-brand {
    padding: 0 8px;
    flex-shrink: 0;
  }

  .tb-leading {
    gap: 4px;
    min-width: 0;
  }

  .tb-spacer {
    flex: 1;
    min-width: 40px;
    height: 100%;
  }

  /* Centred slot, anchored to the WINDOW rather than to the leftover space.
     Flex centring would put it in a different place in every product, because
     each one has a different amount of chrome to its left and right — and a
     strip of tabs that jumps sideways when you switch tab is the one thing it
     must never do. Absolute centring costs the two spacers around it (kept for
     the drag region) but keeps the tabs nailed to the same pixel everywhere. */
  .tb-center {
    position: absolute;
    left: 50%;
    transform: translateX(-50%);
    display: flex;
    align-items: center;
    height: 100%;
    max-width: 46%;
    z-index: 1;
  }

  .tb-right {
    height: 100%;
    flex-shrink: 0;
  }
  /* macOS: the native traffic lights replace our controls, so the settings gear
     is the last element — give it a touch more room from the rounded corner. */
  .tb-right-mac {
    padding-right: 8px;
  }

  /* macOS-only hairline separating the traffic-light gutter from app chrome. */
  .tb-mac-sep {
    width: 1px;
    height: 18px;
    background: var(--border);
    flex-shrink: 0;
    margin-right: 8px;
    -webkit-app-region: no-drag;
  }
  /* Fullscreen hides the traffic lights, so there's nothing to divide from. */
  :global([data-fullscreen="true"]) .tb-mac-sep { display: none; }

  /* ── Hamburger trigger ──────────────────────────────────────────────── */
  .tb-hamburger {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 42px;
    height: 42px;
    background: transparent;
    border: none;
    border-radius: var(--radius-sm);
    cursor: pointer;
    color: var(--text-secondary);
    transition: background var(--transition-fast), color var(--transition-fast);
    -webkit-app-region: no-drag;
  }
  .tb-hamburger:hover,
  .tb-hamburger.active {
    background: var(--bg-overlay);
    color: var(--text-primary);
  }

  /* ── Named / icon buttons (docs · palette · settings) ───────────────── */
  .tb-icon {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 34px;
    height: 34px;
    background: transparent;
    border: none;
    border-radius: var(--radius-sm);
    cursor: pointer;
    color: var(--text-secondary);
    transition: background var(--transition-fast), color var(--transition-fast);
    -webkit-app-region: no-drag;
  }
  .tb-icon:hover { background: var(--bg-hover); color: var(--text-primary); }
  .tb-icon.active { color: var(--accent); }

  .tb-sep {
    width: 1px;
    height: 18px;
    background: var(--border);
    flex-shrink: 0;
    margin: 0 4px;
  }

  /* Compact title bar — shrinks icon buttons to keep the chrome proportional
     when the host lowers `--titlebar-h`. Mirrors the previous Arbor rule. */
  :global([data-compact-title-bar="true"]) .tb-icon {
    width: 26px;
    height: 26px;
  }
  :global([data-compact-title-bar="true"]) .tb-sep {
    height: 14px;
  }
</style>
