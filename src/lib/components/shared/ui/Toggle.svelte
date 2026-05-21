<script lang="ts">
  type Size = 'sm' | 'md' | 'lg';

  interface Props {
    checked: boolean;
    disabled?: boolean;
    size?: Size;
    /** Inline label rendered to the right of the switch. */
    label?: string;
    /** Secondary description rendered under the label. */
    description?: string;
    /** Where to place the label relative to the track. */
    labelPosition?: 'before' | 'after';
    ariaLabel?: string;
    onchange?: (value: boolean) => void;
  }

  let {
    checked = $bindable(),
    disabled = false,
    size = 'md',
    label,
    description,
    labelPosition = 'after',
    ariaLabel,
    onchange,
  }: Props = $props();
</script>

<label class="toggle sz-{size} pos-{labelPosition}" class:disabled>
  {#if label && labelPosition === 'before'}
    <span class="toggle-text">
      <span class="toggle-label">{label}</span>
      {#if description}<span class="toggle-desc">{description}</span>{/if}
    </span>
  {/if}

  <input
    type="checkbox"
    bind:checked
    {disabled}
    aria-label={ariaLabel ?? label}
    onchange={() => onchange?.(checked)}
  />
  <span class="toggle-track" aria-hidden="true">
    <span class="toggle-thumb"></span>
  </span>

  {#if label && labelPosition === 'after'}
    <span class="toggle-text">
      <span class="toggle-label">{label}</span>
      {#if description}<span class="toggle-desc">{description}</span>{/if}
    </span>
  {/if}
</label>

<style>
  .toggle {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    cursor: pointer;
    flex-shrink: 0;
  }
  .toggle.disabled { opacity: 0.4; cursor: not-allowed; }

  /* Hide native input but keep it accessible. */
  .toggle input {
    position: absolute;
    width: 1px;
    height: 1px;
    margin: -1px;
    border: 0;
    padding: 0;
    overflow: hidden;
    clip: rect(0 0 0 0);
  }

  .toggle-track {
    position: relative;
    flex-shrink: 0;
    background: var(--bg-overlay);
    border: 1px solid var(--border);
    border-radius: 999px;
    transition: background var(--transition-fast), border-color var(--transition-fast);
  }
  .toggle input:focus-visible ~ .toggle-track {
    box-shadow: 0 0 0 3px var(--accent-subtle);
  }
  .toggle input:checked ~ .toggle-track {
    background: var(--accent);
    border-color: var(--accent);
  }

  .toggle-thumb {
    position: absolute;
    top: 50%;
    left: 2px;
    background: var(--text-muted);
    border-radius: 50%;
    transform: translateY(-50%);
    transition: transform var(--transition-fast), background var(--transition-fast);
  }
  .toggle input:checked ~ .toggle-track .toggle-thumb {
    background: var(--text-on-accent);
  }

  /* ---- Sizes ---- */
  .sz-sm .toggle-track { width: 26px; height: 14px; }
  .sz-sm .toggle-thumb { width: 8px;  height: 8px; }
  .sz-sm input:checked ~ .toggle-track .toggle-thumb { transform: translate(12px, -50%); }

  .sz-md .toggle-track { width: 32px; height: 18px; }
  .sz-md .toggle-thumb { width: 12px; height: 12px; }
  .sz-md input:checked ~ .toggle-track .toggle-thumb { transform: translate(14px, -50%); }

  .sz-lg .toggle-track { width: 40px; height: 22px; }
  .sz-lg .toggle-thumb { width: 16px; height: 16px; }
  .sz-lg input:checked ~ .toggle-track .toggle-thumb { transform: translate(18px, -50%); }

  /* ---- Label ---- */
  .toggle-text {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }
  .toggle-label { font-size: var(--font-size-sm); color: var(--text-primary); }
  .toggle-desc  { font-size: var(--font-size-xs); color: var(--text-muted); }

  .pos-before .toggle-text { text-align: right; }
</style>
