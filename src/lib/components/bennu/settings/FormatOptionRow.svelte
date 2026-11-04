<script lang="ts">
  /**
   * One formatter option as a settings row — the control follows the option's kind, so the page
   * renders the whole catalogue without a hand-written row.
   *
   * Beside the control: a badge when a layer other than the user's own style decides the value
   * (`.editorconfig`, detection, the project), and a reset button when the scope being edited has a
   * value of its own to undo.
   */
  import { RotateCcw } from 'lucide-svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import FormRow from '$lib/components/shared/ui/FormRow.svelte';
  import IconButton from '$lib/components/shared/ui/IconButton.svelte';
  import NumberStepper from '$lib/components/shared/ui/NumberStepper.svelte';
  import RadioGroup from '$lib/components/shared/ui/RadioGroup.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import Toggle from '$lib/components/shared/ui/Toggle.svelte';
  import TokenListInput from '$lib/components/shared/ui/TokenListInput.svelte';
  import { describeValue, type FormatOption } from '../format-options';
  import type { FormatScope } from '$lib/stores/bennu/format-style.svelte';
  import type { FormatLayer } from '$lib/types/bennu-format';

  interface Props {
    option: FormatOption;
    value: unknown;
    /** Greyed and out of the tab order: the option this one depends on is off. */
    inert?: boolean;
    /** Which layer decides the effective value. */
    layer: FormatLayer;
    /** What the effective value is — the badge's tooltip names it when it is not the one shown. */
    effectiveValue: unknown;
    scope: FormatScope;
    /** The scope being edited has a value of its own here. */
    modified: boolean;
    onchange: (value: unknown) => void;
    onreset: () => void;
  }

  let { option, value, inert = false, layer, effectiveValue, scope, modified, onchange, onreset }: Props = $props();

  /** Segments read better than a dropdown while there are only a few, short choices. */
  const SEGMENT_MAX = 3;

  const IMPORT_GROUP_SUGGESTIONS = [
    { value: '*', detail: 'Everything else' },
    { value: 'static', detail: 'The static block, when static imports are mixed in' },
    { value: 'java' }, { value: 'javax' }, { value: 'jakarta' }, { value: 'org' }, { value: 'com' },
  ];

  const BADGES: Partial<Record<FormatLayer, { label: string; tone: 'accent' | 'info' | 'warning' }>> = {
    project: { label: 'project', tone: 'accent' },
    editorconfig: { label: 'editorconfig', tone: 'info' },
    detected: { label: 'detected', tone: 'warning' },
  };
  const SOURCE_TEXT: Partial<Record<FormatLayer, string>> = {
    project: 'this project’s style',
    editorconfig: 'the project’s .editorconfig',
    detected: 'what the project’s code already does',
  };

  const badge = $derived(BADGES[layer]);
  const badgeTip = $derived.by(() => {
    const source = SOURCE_TEXT[layer];
    if (!source) return '';
    return scope === 'project'
      ? `Value comes from ${source}`
      : `In the open project ${source} sets this to ${describeValue(option.key, effectiveValue)}`;
  });

  const stacked = $derived(option.kind === 'stringlist');
  const segmented = $derived(option.kind === 'enum' && option.choices.length <= SEGMENT_MAX);
</script>

<FormRow label={option.label} description={option.description} {stacked}>
  <div class="fo-ctl" class:inert aria-disabled={inert} {inert}>
    {#if badge}
      <Badge variant="tone" tone={badge.tone} size="sm" label={badge.label} tooltip={badgeTip} />
    {/if}
    {#if modified}
      <IconButton tooltip={scope === 'project' ? 'Use the style below the project' : 'Back to the default'} size={22} onclick={onreset}>
        <RotateCcw size={12} />
      </IconButton>
    {/if}

    {#if option.kind === 'bool'}
      <Toggle checked={value as boolean} onchange={(v) => onchange(v)} ariaLabel={option.label} />
    {:else if option.kind === 'int'}
      <NumberStepper value={value as number} min={option.min} max={option.max} step={option.step ?? 1}
                     suffix={option.suffix} narrow onchange={(v) => onchange(v)} ariaLabel={option.label} />
    {:else if option.kind === 'enum' && segmented}
      <RadioGroup value={value as string} options={option.choices} size="sm" nowrap
                  onchange={(v) => onchange(v)} />
    {:else if option.kind === 'enum'}
      <Select value={value as string} options={option.choices} size="sm" narrow
              onchange={(v) => onchange(v)} ariaLabel={option.label} />
    {:else if option.kind === 'stringlist'}
      <div class="fo-list">
        <TokenListInput values={value as string[]} onchange={(next) => onchange(next)}
                        placeholder={option.placeholder} suggestions={IMPORT_GROUP_SUGGESTIONS}
                        emptyMessage="No groups: every import is one block." ariaLabel={option.label} />
      </div>
    {/if}
  </div>
</FormRow>

<style>
  .fo-ctl { display: flex; align-items: center; gap: 6px; }
  .fo-ctl.inert { opacity: 0.45; }
  .fo-list { flex: 1; min-width: 0; }
  /* A stacked row puts the actions above the full-width list, not beside it. */
  .fo-ctl:has(.fo-list) { flex-wrap: wrap; }
  .fo-list { flex-basis: 100%; }
</style>
