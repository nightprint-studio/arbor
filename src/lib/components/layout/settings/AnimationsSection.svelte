<script lang="ts">
  import { Zap } from 'lucide-svelte';
  import { animStore, type AnimSpeed } from '$lib/stores/animations.svelte';
  import SectionHeader from '$lib/components/shared/ui/SectionHeader.svelte';
  import FormRow from '$lib/components/shared/ui/FormRow.svelte';
  import Toggle from '$lib/components/shared/ui/Toggle.svelte';
  import RadioGroup from '$lib/components/shared/ui/RadioGroup.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';

  // Local state — mirrors store; $effects sync both directions.
  let enabled = $state(animStore.enabled);
  let speed   = $state<AnimSpeed>(animStore.speed);

  $effect(() => { animStore.setEnabled(enabled); });
  $effect(() => { animStore.setSpeed(speed); });

  const speeds: { value: AnimSpeed; label: string; description: string }[] = [
    { value: 'fast',   label: 'Snappy',  description: 'Faster, minimal transitions' },
    { value: 'normal', label: 'Normal',  description: 'Balanced — default'          },
    { value: 'slow',   label: 'Relaxed', description: 'Slower, more fluid motion'   },
  ];

  // Bump to replay the CSS animation on the preview chip.
  let previewKey = $state(0);
  function replay() { previewKey++; }
</script>

<SectionHeader title="Animations" description="Control the speed and behaviour of UI transitions and motion effects." />

<div class="card">
  <!-- Enable toggle -->
  <FormRow label="Enable animations" description="Toggles all transitions and motion effects globally">
    <Toggle bind:checked={enabled} />
  </FormRow>

  <!-- Speed -->
  {#if enabled}
    <FormRow label="Speed" description="How fast transitions play across the UI">
      <RadioGroup
        bind:value={speed}
        options={speeds}
        appearance="segment"
        size="md"
        onchange={() => replay()}
      />
    </FormRow>

    <!-- Preview -->
    <FormRow label="Preview" description="See the current speed live">
      <div class="preview-area">
        {#key previewKey}
          <span class="preview-chip">
            <Zap size={12} />
            Animation preview
          </span>
        {/key}
        <Button variant="ghost" size="sm" onclick={replay}>Replay</Button>
      </div>
    </FormRow>
  {/if}
</div>

<style>
  /* Preview row */
  .preview-area {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .preview-chip {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 4px 12px;
    background: var(--accent-subtle);
    border: 1px solid var(--accent);
    border-radius: var(--radius-md);
    color: var(--accent);
    font-size: var(--font-size-sm);
    animation: previewPop var(--anim-dur-base, 150ms) var(--anim-easing-spring, cubic-bezier(0.16,1,0.3,1)) both;
  }

  @keyframes previewPop {
    from { opacity: 0; transform: scale(0.8) translateY(4px); }
    to   { opacity: 1; transform: scale(1)   translateY(0);   }
  }
</style>
