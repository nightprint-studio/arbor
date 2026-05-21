<script lang="ts">
  /**
   * Celebrative "no findings" placeholder shown when the dashboard returns
   * a summary with zero counts across every severity. Distinct from the
   * "no token" state — here we know we *can* see findings, there just
   * aren't any.
   */
  import { ShieldCheck } from 'lucide-svelte';
</script>

<div class="state-view">
  <ShieldCheck size={32} class="state-icon" />
  <p class="state-title">No findings</p>
  <p class="state-hint">Nothing flagged on this branch — all clear.</p>
</div>

<style>
  .state-view {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 10px;
    padding: 32px 24px;
    color: var(--text-muted);
    text-align: center;
    font-family: var(--font-ui-sans);
  }
  :global(.state-view .state-icon) {
    color: var(--success);
    opacity: 0.9;
  }
  .state-title {
    margin: 0;
    font-size: 13px;
    font-weight: 600;
    color: var(--text-primary);
  }
  .state-hint {
    margin: 0;
    font-size: 12px;
    line-height: 1.5;
    max-width: 240px;
  }
</style>
