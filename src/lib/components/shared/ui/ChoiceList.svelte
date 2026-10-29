<script lang="ts">
  /**
   * A list of choices, one per row — pick one, or tick several — with an "Other…" row that opens a field
   * for what the list did not think of.
   *
   * The shape a question takes when its answers are a handful of sentences rather than a word: each row
   * is a whole target (label above, what choosing it means below), the chosen ones take the accent, and
   * the arrow keys and Space work because the rows are real radio buttons and checkboxes underneath.
   * `RadioGroup`'s card appearance is the single-choice cousin without the multi mode or the free row.
   */
  export interface Choice {
    value: string;
    label: string;
    /** What choosing it means, in a line. */
    description?: string;
  }

  import Input from './Input.svelte';

  const OTHER = '__other__';

  let {
    options,
    multi = false,
    value = $bindable<string[]>([]),
    other = $bindable(''),
    allowOther = true,
    otherLabel = 'Other…',
    otherPlaceholder = 'Your answer',
    name,
    disabled = false,
  }: {
    options: Choice[];
    /** Tick several instead of picking one. */
    multi?: boolean;
    /** The chosen values. The free row is the reserved value `__other__` while it is chosen, and what is
     *  typed into it is `other`. */
    value?: string[];
    other?: string;
    allowOther?: boolean;
    otherLabel?: string;
    otherPlaceholder?: string;
    /** Groups the radios of one list (single mode). Unique per list on a page. */
    name: string;
    disabled?: boolean;
  } = $props();

  const otherOn = $derived(value.includes(OTHER));

  function toggle(v: string) {
    if (disabled) return;
    if (multi) value = value.includes(v) ? value.filter((x) => x !== v) : [...value, v];
    else value = [v];
  }
</script>

<div class="list" role={multi ? 'group' : 'radiogroup'}>
  {#each options as o (o.value)}
    {@const on = value.includes(o.value)}
    <label class="row" class:on class:disabled>
      <input
        class="native"
        type={multi ? 'checkbox' : 'radio'}
        {name}
        checked={on}
        {disabled}
        onchange={() => toggle(o.value)}
      />
      <span class="mark" class:square={multi} aria-hidden="true"></span>
      <span class="text">
        <span class="label">{o.label}</span>
        {#if o.description}<span class="why">{o.description}</span>{/if}
      </span>
    </label>
  {/each}

  {#if allowOther}
    <div class="row other" class:on={otherOn} class:disabled>
      <label class="pick">
        <input
          class="native"
          type={multi ? 'checkbox' : 'radio'}
          {name}
          checked={otherOn}
          {disabled}
          onchange={() => toggle(OTHER)}
        />
        <span class="mark" class:square={multi} aria-hidden="true"></span>
        <span class="text"><span class="label">{otherLabel}</span></span>
      </label>
      {#if otherOn}
        <div class="field"><Input bind:value={other} placeholder={otherPlaceholder} /></div>
      {/if}
    </div>
  {/if}
</div>

<style>
  .list { display: flex; flex-direction: column; gap: 5px; }

  .row {
    display: flex; align-items: flex-start; gap: 10px;
    padding: 8px 10px;
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    background: var(--bg-base);
    cursor: pointer;
    transition: border-color var(--transition-fast), background var(--transition-fast);
  }
  .row:hover:not(.disabled) { background: var(--bg-hover); }
  .row.on {
    border-color: var(--accent);
    background: color-mix(in srgb, var(--accent) 9%, var(--bg-base));
  }
  .row.disabled { opacity: 0.55; cursor: not-allowed; }
  .row:has(.native:focus-visible) { box-shadow: 0 0 0 3px var(--accent-subtle); }

  .row.other { flex-direction: column; gap: 7px; }
  .pick { display: flex; align-items: flex-start; gap: 10px; cursor: pointer; }

  /* The real input, kept for the keyboard and for assistive tech, and hidden for the eye. */
  .native { position: absolute; width: 1px; height: 1px; margin: -1px; padding: 0; border: 0; overflow: hidden; clip: rect(0 0 0 0); }

  .mark {
    position: relative; flex-shrink: 0; margin-top: 2px;
    width: 15px; height: 15px; border-radius: 50%;
    border: 1.5px solid var(--border);
    background: var(--bg-input, var(--bg-base));
    transition: border-color var(--transition-fast), background var(--transition-fast);
  }
  .mark.square { border-radius: 4px; }
  .row.on .mark { border-color: var(--accent); background: var(--accent); }
  .row.on .mark::after {
    content: ''; position: absolute; inset: 0; margin: auto;
    width: 5px; height: 5px; border-radius: 50%; background: var(--text-on-accent);
  }
  .row.on .mark.square::after {
    width: 8px; height: 4px; margin: 3px 0 0 2.5px; border-radius: 0; background: transparent;
    border-left: 1.8px solid var(--text-on-accent); border-bottom: 1.8px solid var(--text-on-accent);
    transform: rotate(-45deg);
  }

  .text { display: flex; flex-direction: column; gap: 1px; min-width: 0; }
  .label { font-size: var(--font-size-sm); color: var(--text-primary); line-height: 1.4; }
  .why { font-size: var(--font-size-xs); color: var(--text-muted); line-height: 1.45; }
  .field { padding-left: 25px; }
</style>
