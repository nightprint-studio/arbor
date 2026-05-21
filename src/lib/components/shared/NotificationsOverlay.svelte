<script lang="ts">
  import { Trash2, Info } from 'lucide-svelte';
  import { flip } from 'svelte/animate';
  import { notificationsStore } from '$lib/stores/notifications.svelte';
  import { uiStore } from '$lib/stores/ui.svelte';
  import { animStore } from '$lib/stores/animations.svelte';
  import NotificationItem from './NotificationItem.svelte';
  import { tooltip } from '$lib/actions/tooltip';
</script>

<!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
<div class="overlay-backdrop" onclick={() => uiStore.setNotificationsOverlayOpen(false)}></div>

<div class="overlay-panel notif-overlay" role="dialog" aria-label="Notifications">
  <div class="overlay-header">
    <span class="overlay-title">Notifications</span>
    <div class="header-actions">
      {#if notificationsStore.count > 0}
        <button class="clear-btn" onclick={() => notificationsStore.clearAll()} use:tooltip={'Clear all notifications'}>
          <Trash2 size={13} />
          <span>Clear all</span>
        </button>
      {/if}
      <button class="mac-close-btn" onclick={() => uiStore.setNotificationsOverlayOpen(false)} use:tooltip={'Close'} aria-label="Close"></button>
    </div>
  </div>

  {#if notificationsStore.count === 0}
    <div class="empty-state">
      <Info size={22} />
      <span>No notifications</span>
    </div>
  {:else}
    <div class="notif-list">
      {#each notificationsStore.notifications as n (n.id)}
        <!-- `animate:flip` lets the items below the dismissed one slide
             smoothly up to close the gap, instead of jumping when the
             removed row's slot finally disappears. -->
        <div animate:flip={{ duration: animStore.dPanel }}>
          <NotificationItem notif={n} />
        </div>
      {/each}
    </div>
  {/if}
</div>

<style>
  .notif-overlay {
    width: 320px;
    max-height: 460px;
    background: var(--bg-base);
    border-color: var(--border);
    box-shadow: 0 8px 32px rgba(0,0,0,0.7);
  }

  .header-actions {
    display: flex;
    align-items: center;
    gap: 4px;
  }

  .clear-btn {
    display: flex;
    align-items: center;
    gap: 4px;
    height: 22px;
    padding: 0 6px;
    border: none;
    background: transparent;
    color: var(--text-muted);
    border-radius: var(--radius-sm);
    cursor: pointer;
    font-size: 11px;
    font-family: var(--font-ui-sans);
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .clear-btn:hover { background: var(--bg-elevated); color: var(--text-primary); }

  .empty-state {
    display: flex; flex-direction: column;
    align-items: center; justify-content: center;
    gap: 8px;
    padding: 36px 16px;
    color: var(--text-disabled);
    font-size: var(--font-size-xs);
  }

  .notif-list {
    overflow-y: auto;
    /* Items animate in/out via `fly` (x: 360px) — without explicit
       overflow-x:hidden, the spec promotes overflow-x to `auto` when
       overflow-y is set, briefly flashing a horizontal scrollbar
       during the transition. */
    overflow-x: hidden;
    flex: 1;
    padding: 6px;
    display: flex;
    flex-direction: column;
    gap: 5px;
  }
</style>
