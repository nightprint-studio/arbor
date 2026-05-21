<script lang="ts">
  /**
   * Graceful "supported provider but no credentials" state. Mirrors the
   * convention used by the CI/CD panel — points the user at Settings →
   * Authentication where the OAuth / token flow lives.
   */
  import { ShieldAlert } from 'lucide-svelte';

  interface Props {
    /** Display name of the detected provider (e.g. "GitLab", "GitHub"). */
    providerLabel: string;
  }

  let { providerLabel }: Props = $props();
</script>

<div class="state-view">
  <ShieldAlert size={28} class="state-icon" />
  <p class="state-title">{providerLabel} detected</p>
  <p class="state-hint">
    Connect your {providerLabel} account in
    <strong>Settings → Authentication</strong> to view security findings here.
  </p>
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
    color: var(--severity-medium);
    opacity: 0.85;
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
