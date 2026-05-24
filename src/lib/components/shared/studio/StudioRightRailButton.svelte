<!--
  StudioRightRailButton — icon button for the modal's right activity
  rail. Standard `ab-btn` chrome with active-pressed state, tooltip,
  optional count badge (formatted `99+` over 99) and optional warning
  dot (used by RON's bindings rail to flag broken refs).

  Each Studio modal renders ~5 of these; this widget collapses the
  ~12-line snippet to one tag.
-->
<script lang="ts">
  import { BookOpen } from 'lucide-svelte';
  import { tooltip } from '$lib/actions/tooltip';

  interface Props {
    /** Lucide-svelte component (or any icon component accepting `size`).
     *  Typed as `typeof BookOpen` (the codebase convention) so every Lucide
     *  icon's full prop surface — `size`, `color`, `strokeWidth`, `class`, …
     *  — is preserved. Svelte 5's `Component<{ size? }>` would be too narrow
     *  and reject every concrete icon at the call site. */
    icon:    typeof BookOpen;
    active:  boolean;
    tooltip: string;
    label:   string;
    onClick: () => void;
    /** Count badge shown bottom-right when > 0. */
    count?:  number;
    /** Warning dot bottom-right (used for broken-ref / error counts). */
    dot?:    boolean;
    /** Tone for `dot` — defaults to error (red). Use `success` for the
     *  "loaded / configured" indicator (e.g. schema chip), `warning` for
     *  the broken-refs flag in the bindings rail. */
    dotTone?: 'success' | 'warning' | 'error';
    iconSize?: number;
  }

  let {
    icon: Icon,
    active,
    tooltip: tip,
    label,
    onClick,
    count,
    dot = false,
    dotTone = 'error',
    iconSize = 20,
  }: Props = $props();

  const displayCount = $derived(
    count !== undefined && count > 0
      ? (count >= 100 ? '99+' : String(count))
      : null,
  );
</script>

<button
  type="button"
  class="ab-btn"
  class:ab-active={active}
  onclick={onClick}
  use:tooltip={tip}
  aria-label={label}
  aria-pressed={active}
>
  <Icon size={iconSize} />
  {#if displayCount}
    <span class="rail-count" aria-hidden="true">{displayCount}</span>
  {/if}
  {#if dot}
    <span class="rail-dot rail-dot-{dotTone}" aria-hidden="true"></span>
  {/if}
</button>

<style>
  .rail-count {
    position: absolute;
    bottom: 2px;
    right: 2px;
    background: var(--accent);
    color: var(--bg-base);
    font-size: 9px;
    font-weight: 700;
    line-height: 1;
    padding: 1px 3px;
    border-radius: 6px;
    min-width: 12px;
    text-align: center;
  }
  .rail-dot {
    position: absolute;
    top: 4px;
    right: 4px;
    width: 6px;
    height: 6px;
    border-radius: 50%;
  }
  .rail-dot-error   { background: var(--error); }
  .rail-dot-warning { background: var(--warning, #e5c07b); }
  .rail-dot-success { background: var(--success, #98c379); }
</style>
