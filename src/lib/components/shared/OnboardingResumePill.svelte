<script lang="ts">
  /**
   * OnboardingResumePill — floating "Resume welcome tour" affordance.
   *
   * Mounted by AppShell when `onboardingStore.paused === true`, i.e. the
   * user clicked a tour link that opens a non-stacking surface (Settings
   * or Docs panel) which would otherwise be occluded by the tour's modal
   * backdrop. This pill replaces the modal until the user clicks Resume
   * (re-opens the tour at the same step) or dismisses it.
   *
   * Partner component of OnboardingModal.svelte — it has no other consumer
   * by design, but lives here next to its sibling rather than inlined in
   * AppShell so AppShell doesn't grow another concern.
   *
   * Positioning: bottom-right, fixed, sitting above the status bar but
   * below modals so a sub-modal (FilePicker, Clone, …) opened from the
   * Settings panel still draws on top of the pill.
   */
  import { Sparkles, X } from 'lucide-svelte';
  import { fly } from 'svelte/transition';
  import { cubicOut } from 'svelte/easing';
  import { animStore } from '$lib/stores/animations.svelte';
  import { onboardingStore } from '$lib/stores/onboarding.svelte';
  import { tooltip } from '$lib/actions/tooltip';
</script>

<div
  class="resume-pill"
  role="region"
  aria-label="Welcome tour paused"
  transition:fly={{ y: 16, duration: animStore.dPanel, easing: cubicOut }}
>
  <span class="icon"><Sparkles size={14} /></span>
  <button
    type="button"
    class="resume-btn"
    onclick={() => onboardingStore.resume()}
  >
    Resume welcome tour
  </button>
  <button
    type="button"
    class="dismiss-btn"
    aria-label="Dismiss"
    use:tooltip={'Dismiss'}
    onclick={() => onboardingStore.dismissPause()}
  >
    <X size={13} />
  </button>
</div>

<style>
  .resume-pill {
    position: fixed;
    right: 18px;
    bottom: 38px;          /* clears the status bar (28px) + breathing room */
    z-index: var(--z-toast, 90);
    display: inline-flex;
    align-items: center;
    gap: 8px;
    padding: 6px 6px 6px 12px;
    background: var(--bg-elevated);
    border: 1px solid color-mix(in srgb, var(--accent) 45%, var(--border));
    border-radius: var(--radius-pill, 999px);
    box-shadow: 0 12px 32px -10px rgba(0, 0, 0, 0.5);
    color: var(--text-primary);
    font-size: var(--font-size-sm);
  }
  .icon {
    color: var(--accent);
    display: inline-flex;
  }
  .resume-btn {
    background: none;
    border: none;
    padding: 2px 4px;
    color: inherit;
    font: inherit;
    font-weight: 500;
    cursor: pointer;
    border-radius: var(--radius-sm);
  }
  .resume-btn:hover { color: var(--accent); }
  .resume-btn:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }
  .dismiss-btn {
    background: none;
    border: none;
    padding: 4px;
    color: var(--text-muted);
    cursor: pointer;
    border-radius: 999px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    transition: background 120ms ease, color 120ms ease;
  }
  .dismiss-btn:hover {
    background: var(--bg-overlay);
    color: var(--text-primary);
  }
  .dismiss-btn:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }
</style>
