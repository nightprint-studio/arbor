<script lang="ts">
  /**
   * A checkbox, including the **indeterminate** state a group needs.
   *
   * Distinct from `Toggle`, which is a switch: a switch turns one thing on or off, a checkbox
   * selects items — and a checkbox that stands for a group of items has a third state, "some of
   * them", which a switch has no way to say. `indeterminate` is not a value (the box is still
   * either checked or not); it is a display state set on the DOM node, so it has to be applied
   * imperatively rather than as an attribute.
   */
  interface Props {
    checked: boolean;
    /** Show the "some, not all" bar instead of a tick. Ignored while `checked` is true. */
    indeterminate?: boolean;
    disabled?: boolean;
    /** Inline label rendered to the right of the box. */
    label?: string;
    ariaLabel?: string;
    /** Fired with the new value on user interaction. */
    onchange?: (value: boolean) => void;
  }

  let {
    checked = $bindable(),
    indeterminate = false,
    disabled = false,
    label,
    ariaLabel,
    onchange,
  }: Props = $props();

  let el = $state<HTMLInputElement | null>(null);

  // `indeterminate` exists only as a DOM property — there is no attribute for it, so it must be
  // written to the node and rewritten whenever it (or `checked`) moves.
  $effect(() => {
    if (el) el.indeterminate = !checked && indeterminate;
  });
</script>

<label class="cb" class:disabled>
  <input
    bind:this={el}
    type="checkbox"
    bind:checked
    {disabled}
    aria-label={ariaLabel ?? label}
    onchange={() => onchange?.(checked)}
  />
  {#if label}<span class="cb-label">{label}</span>{/if}
</label>

<style>
  .cb {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    cursor: pointer;
    user-select: none;
  }
  .cb.disabled {
    cursor: not-allowed;
    opacity: 0.5;
  }

  input {
    appearance: none;
    margin: 0;
    width: 14px;
    height: 14px;
    flex: none;
    border: 1px solid var(--border-strong);
    border-radius: 3px;
    background: var(--bg-base);
    display: grid;
    place-content: center;
    cursor: inherit;
    transition: background 0.12s ease, border-color 0.12s ease;
  }
  input:hover:not(:disabled) {
    border-color: var(--accent);
  }
  input:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
  }
  input:checked,
  input:indeterminate {
    background: var(--accent);
    border-color: var(--accent);
  }

  /* The tick and the bar are drawn, not glyphs: a font-dependent character would sit differently
     in every theme and at every zoom. */
  input:checked::after {
    content: '';
    width: 3px;
    height: 7px;
    border: solid var(--accent-fg, #fff);
    border-width: 0 2px 2px 0;
    transform: translateY(-1px) rotate(45deg);
  }
  input:indeterminate::after {
    content: '';
    width: 8px;
    height: 2px;
    border-radius: 1px;
    background: var(--accent-fg, #fff);
  }

  .cb-label {
    font-size: 12px;
    color: var(--text-secondary);
  }
</style>
