<script lang="ts">
  import { tooltip } from '$lib/actions/tooltip';

  interface Option {
    value: string;
    label: string;
    description?: string;
    /** Lucide icon component (rendered next to the label). */
    icon?: any;
    disabled?: boolean;
  }

  type Appearance = 'segment' | 'radio' | 'card';
  type Size       = 'sm' | 'md';
  type Direction  = 'horizontal' | 'vertical';

  interface Props {
    value: string;
    options: Option[];
    name?: string;
    /** 'segment' = pill-style toggle bar | 'radio' = classic radios | 'card' = description+title cards */
    appearance?: Appearance;
    size?: Size;
    direction?: Direction;
    disabled?: boolean;
    /** Make the group fill its container (segment + card layouts). */
    block?: boolean;
    onchange?: (value: string) => void;
  }

  let {
    value = $bindable(),
    options,
    name = crypto.randomUUID(),
    appearance = 'segment',
    size       = 'md',
    direction  = 'horizontal',
    disabled   = false,
    block      = false,
    onchange,
  }: Props = $props();

  function select(v: string) {
    if (disabled) return;
    value = v;
    onchange?.(v);
  }
</script>

<div
  class="radio-group app-{appearance} sz-{size} dir-{direction}"
  class:disabled
  class:block
  role={appearance === 'segment' || appearance === 'card' ? 'radiogroup' : undefined}
>
  {#each options as opt}
    {#if appearance === 'segment'}
      <button
        type="button"
        class="seg-btn"
        class:selected={value === opt.value}
        disabled={disabled || opt.disabled}
        use:tooltip={opt.description ?? ''}
        aria-pressed={value === opt.value}
        onclick={() => select(opt.value)}
      >
        {#if opt.icon}
          {@const Icon = opt.icon}
          <Icon size={size === 'md' ? 13 : 11} />
        {/if}
        {opt.label}
      </button>
    {:else if appearance === 'card'}
      <button
        type="button"
        class="card-opt"
        class:selected={value === opt.value}
        disabled={disabled || opt.disabled}
        aria-pressed={value === opt.value}
        onclick={() => select(opt.value)}
      >
        {#if opt.icon}
          {@const Icon = opt.icon}
          <Icon size={size === 'md' ? 16 : 14} class="card-opt-icon" />
        {/if}
        <span class="card-opt-text">
          <span class="card-opt-title">{opt.label}</span>
          {#if opt.description}
            <span class="card-opt-desc">{opt.description}</span>
          {/if}
        </span>
      </button>
    {:else}
      <label class="radio-option" class:selected={value === opt.value}>
        <input
          type="radio"
          {name}
          value={opt.value}
          checked={value === opt.value}
          disabled={disabled || opt.disabled}
          onchange={() => select(opt.value)}
        />
        <span class="radio-label-block">
          <span class="radio-label-row">
            {#if opt.icon}
              {@const Icon = opt.icon}
              <Icon size={size === 'md' ? 13 : 11} />
            {/if}
            <span>{opt.label}</span>
          </span>
          {#if opt.description}
            <span class="radio-desc">{opt.description}</span>
          {/if}
        </span>
      </label>
    {/if}
  {/each}
</div>

<style>
  .radio-group { display: flex; gap: 4px; }
  .radio-group.disabled { opacity: 0.45; pointer-events: none; }

  .dir-vertical { flex-direction: column; align-items: stretch; }
  .dir-horizontal { flex-direction: row; align-items: center; flex-wrap: wrap; }
  .block { width: 100%; }
  .block.app-segment .seg-btn,
  .block.app-card    .card-opt { flex: 1; }

  /* ---- Segment ---- */
  .app-segment { gap: 2px; }
  .seg-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 5px;
    border-radius: var(--radius-md);
    border: 1px solid var(--border);
    background: var(--bg-input);
    color: var(--text-secondary);
    font-family: var(--font-ui-sans);
    cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast),
                border-color var(--transition-fast);
  }
  .app-segment.sz-sm .seg-btn { padding: 3px 9px;  font-size: var(--font-size-xs); }
  .app-segment.sz-md .seg-btn { padding: 4px 12px; font-size: var(--font-size-sm); }
  .seg-btn:hover:not(:disabled) { background: var(--bg-hover); color: var(--text-primary); }
  .seg-btn.selected {
    background: var(--accent-subtle);
    border-color: var(--accent);
    color: var(--accent);
    font-weight: 500;
  }
  .seg-btn:disabled { opacity: 0.45; cursor: not-allowed; }

  /* ---- Card ---- */
  .app-card { gap: 8px; }
  .card-opt {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    text-align: left;
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    color: var(--text-secondary);
    cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast),
                border-color var(--transition-fast);
  }
  .app-card.sz-sm .card-opt { padding: 6px 9px;  font-size: var(--font-size-xs); }
  .app-card.sz-md .card-opt { padding: 8px 12px; font-size: var(--font-size-sm); }
  .card-opt:hover:not(:disabled) { background: var(--bg-hover); color: var(--text-primary); border-color: var(--border-subtle); }
  .card-opt.selected {
    background: var(--accent-subtle);
    border-color: var(--accent);
    color: var(--text-primary);
  }
  .card-opt:disabled { opacity: 0.45; cursor: not-allowed; }
  :global(.card-opt-icon) { flex-shrink: 0; margin-top: 1px; color: var(--accent); }
  .card-opt-text { display: flex; flex-direction: column; gap: 2px; min-width: 0; }
  .card-opt-title { font-weight: 500; color: var(--text-primary); }
  .card-opt-desc  { font-size: var(--font-size-xs); color: var(--text-muted); white-space: normal; }

  /* ---- Radio ---- */
  .app-radio { gap: 8px; }
  .app-radio.dir-horizontal { gap: 14px; flex-wrap: wrap; align-items: center; }

  .radio-option {
    display: inline-flex;
    align-items: flex-start;
    gap: 6px;
    color: var(--text-secondary);
    cursor: pointer;
    transition: color var(--transition-fast);
  }
  .app-radio.sz-sm .radio-option { font-size: var(--font-size-xs); }
  .app-radio.sz-md .radio-option { font-size: var(--font-size-sm); }
  .radio-option.selected { color: var(--text-primary); }
  .radio-option input { accent-color: var(--accent); cursor: pointer; margin-top: 2px; }

  .radio-label-block { display: flex; flex-direction: column; gap: 2px; min-width: 0; }
  .radio-label-row   { display: inline-flex; align-items: center; gap: 5px; }
  .radio-desc        { font-size: var(--font-size-xs); color: var(--text-muted); }
</style>
