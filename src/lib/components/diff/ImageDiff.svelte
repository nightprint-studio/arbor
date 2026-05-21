<script lang="ts">
  import type { DiffFile } from '$lib/types/git';

  let { file }: { file: DiffFile } = $props();
  let mode = $state<'side-by-side' | 'onion'>('side-by-side');
  let sliderVal = $state(50);
</script>

<div class="image-diff">
  <div class="controls">
    <button class="mode-btn" class:active={mode === 'side-by-side'} onclick={() => mode = 'side-by-side'}>Side by side</button>
    <button class="mode-btn" class:active={mode === 'onion'} onclick={() => mode = 'onion'}>Onion skin</button>
  </div>

  {#if mode === 'side-by-side'}
    <div class="side-by-side">
      <div class="side">
        <div class="label">Before</div>
        {#if file.image_old}
          <img src="data:image/*;base64,{file.image_old}" alt="before" />
        {:else}
          <div class="no-image">No file</div>
        {/if}
      </div>
      <div class="side">
        <div class="label">After</div>
        {#if file.image_new}
          <img src="data:image/*;base64,{file.image_new}" alt="after" />
        {:else}
          <div class="no-image">No file</div>
        {/if}
      </div>
    </div>
  {:else}
    <!-- Onion skin / slider -->
    <div class="onion-wrap">
      {#if file.image_old}
        <img class="onion-img" src="data:image/*;base64,{file.image_old}" alt="before" style="opacity: {1 - sliderVal / 100}" />
      {/if}
      {#if file.image_new}
        <img class="onion-img onion-top" src="data:image/*;base64,{file.image_new}" alt="after" style="opacity: {sliderVal / 100}" />
      {/if}
    </div>
    <div class="slider-wrap">
      <span class="text-muted text-xs">Before</span>
      <input type="range" min="0" max="100" bind:value={sliderVal} style="flex:1" />
      <span class="text-muted text-xs">After</span>
    </div>
  {/if}
</div>

<style>
  .image-diff { display: flex; flex-direction: column; gap: 12px; padding: 16px; height: 100%; overflow: auto; }
  .controls { display: flex; gap: 4px; }
  .mode-btn { padding: 3px 10px; background: transparent; border: 1px solid var(--border); border-radius: var(--radius-sm); cursor: pointer; color: var(--text-muted); font-family: var(--font-ui-sans); font-size: var(--font-size-xs); }
  .mode-btn.active { background: var(--accent-subtle); color: var(--accent); border-color: var(--accent); }
  .side-by-side { display: flex; gap: 16px; }
  .side { flex: 1; display: flex; flex-direction: column; gap: 6px; }
  .label { font-size: var(--font-size-xs); color: var(--text-muted); }
  img { max-width: 100%; border: 1px solid var(--border); border-radius: var(--radius-sm); }
  .no-image { color: var(--text-disabled); font-size: var(--font-size-sm); padding: 24px; text-align: center; border: 1px dashed var(--border); border-radius: var(--radius-md); }
  .onion-wrap { position: relative; display: inline-block; }
  .onion-img { display: block; max-width: 100%; }
  .onion-top { position: absolute; top: 0; left: 0; }
  .slider-wrap { display: flex; align-items: center; gap: 10px; }
</style>
