<script lang="ts">
  import { CheckCircle, AlertCircle, Loader, Eye, EyeOff } from 'lucide-svelte';
  import { issuesStore } from '$lib/stores/issues.svelte';
  import { uiStore } from '$lib/stores/ui.svelte';
  import { tooltip } from '$lib/actions/tooltip';

  // Load auth status if not already loaded
  $effect(() => {
    if (issuesStore.authStatus === null) issuesStore.loadAuthStatus();
  });

  let tokenInput  = $state('');
  let showToken   = $state(false);
  let saving      = $state(false);
  let tokenError  = $state('');

  async function saveToken() {
    if (!tokenInput.trim()) return;
    tokenError = ''; saving = true;
    try {
      await issuesStore.saveToken(tokenInput.trim());
      tokenInput = '';
      uiStore.showToast('Connected to Linear', 'success');
    } catch (e) {
      tokenError = String(e);
    } finally {
      saving = false;
    }
  }

  async function disconnect() {
    await issuesStore.logout();
    uiStore.showToast('Disconnected from Linear', 'info');
  }
</script>

<div class="integrations-section">
  <div class="section-title">Integrations</div>

  <!-- Linear -->
  <div class="provider-card">
    <div class="provider-header">
      <div class="provider-info">
        <span class="provider-name">Linear</span>
        <span class="provider-desc">Issue tracker integration</span>
      </div>
      {#if issuesStore.authLoading || issuesStore.authStatus === null}
        <Loader size={14} class="spin provider-status-icon" />
      {:else if issuesStore.authStatus.authenticated}
        <CheckCircle size={14} class="icon-success" />
        <span class="provider-connected">Connected</span>
      {:else}
        <AlertCircle size={14} class="icon-muted" />
        <span class="provider-disconnected">Not connected</span>
      {/if}
    </div>

    {#if issuesStore.authStatus?.authenticated && issuesStore.authStatus.user}
      <div class="connected-info">
        <div class="user-row">
          {#if issuesStore.authStatus.user.avatarUrl}
            <img class="user-avatar" src={issuesStore.authStatus.user.avatarUrl} alt="" />
          {:else}
            <span class="user-avatar-ph">{issuesStore.authStatus.user.displayName[0]}</span>
          {/if}
          <div>
            <div class="user-name">{issuesStore.authStatus.user.displayName}</div>
            {#if issuesStore.authStatus.user.email}
              <div class="user-email">{issuesStore.authStatus.user.email}</div>
            {/if}
          </div>
        </div>
        <button class="btn-danger-ghost" onclick={disconnect}>Disconnect</button>
      </div>
    {:else if issuesStore.authStatus && !issuesStore.authStatus.authenticated}
      <div class="token-form">
        <p class="token-hint">
          Generate a <strong>Personal API Key</strong> at
          <code>linear.app → Settings → API</code>.
        </p>
        <div class="token-input-wrap">
          <input
            class="token-input"
            type={showToken ? 'text' : 'password'}
            placeholder="lin_api_…"
            bind:value={tokenInput}
            onkeydown={(e) => e.key === 'Enter' && saveToken()}
          />
          <button class="token-toggle" onclick={() => (showToken = !showToken)} use:tooltip={'Toggle visibility'}>
            {#if showToken}<EyeOff size={13} />{:else}<Eye size={13} />{/if}
          </button>
          <button class="btn-primary" onclick={saveToken} disabled={saving || !tokenInput.trim()}>
            {#if saving}<Loader size={11} class="spin" />{/if}
            {saving ? 'Connecting…' : 'Connect'}
          </button>
        </div>
        {#if tokenError}
          <p class="token-error">{tokenError}</p>
        {/if}
      </div>
    {/if}
  </div>

  <!-- Jira (coming soon) -->
  <div class="provider-card provider-card-disabled">
    <div class="provider-header">
      <div class="provider-info">
        <span class="provider-name">Jira</span>
        <span class="provider-desc">Coming soon</span>
      </div>
      <span class="badge-soon">Soon</span>
    </div>
  </div>
</div>

<style>
  .integrations-section {
    padding: 20px 24px;
    display: flex; flex-direction: column; gap: 16px;
    font-family: var(--font-ui-sans);
  }
  .section-title {
    font-size: 11px; font-weight: 700; letter-spacing: 0.06em;
    text-transform: uppercase; color: var(--text-muted);
  }

  /* Provider card */
  .provider-card {
    background: var(--bg-overlay);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    padding: 14px 16px;
    display: flex; flex-direction: column; gap: 12px;
  }
  .provider-card-disabled { opacity: 0.5; pointer-events: none; }
  .provider-header {
    display: flex; align-items: center; gap: 8px;
  }
  .provider-info { flex: 1; display: flex; flex-direction: column; gap: 2px; }
  .provider-name { font-size: 13px; font-weight: 600; color: var(--text-primary); }
  .provider-desc { font-size: 11px; color: var(--text-muted); }
  .provider-connected { font-size: 11px; color: var(--success); font-weight: 500; }
  .provider-disconnected { font-size: 11px; color: var(--text-muted); }
  :global(.icon-success) { color: var(--success); }
  :global(.icon-muted)   { color: var(--text-muted); }
  :global(.provider-status-icon) { color: var(--text-muted); }
  .badge-soon {
    font-size: 10px; font-weight: 600; padding: 2px 7px;
    background: var(--bg-elevated); border: 1px solid var(--border-subtle);
    border-radius: 99px; color: var(--text-muted);
  }

  /* Connected info */
  .connected-info {
    display: flex; align-items: center; justify-content: space-between; gap: 12px;
  }
  .user-row { display: flex; align-items: center; gap: 8px; }
  .user-avatar { width: 28px; height: 28px; border-radius: 50%; object-fit: cover; }
  .user-avatar-ph {
    width: 28px; height: 28px; border-radius: 50%;
    background: var(--accent-subtle); color: var(--accent);
    font-size: 12px; font-weight: 700;
    display: flex; align-items: center; justify-content: center;
  }
  .user-name { font-size: 12px; font-weight: 500; color: var(--text-primary); }
  .user-email { font-size: 10px; color: var(--text-muted); }
  .btn-danger-ghost {
    padding: 5px 12px; font-size: 11px; font-weight: 500;
    background: transparent; border: 1px solid var(--border);
    border-radius: var(--radius-md); color: var(--text-muted);
    cursor: pointer; font-family: var(--font-ui-sans);
    transition: all var(--transition-fast);
    flex-shrink: 0;
  }
  .btn-danger-ghost:hover { color: var(--status-error, #f87171); border-color: var(--status-error, #f87171); }

  /* Token form */
  .token-form { display: flex; flex-direction: column; gap: 8px; }
  .token-hint { font-size: 11px; color: var(--text-muted); margin: 0; line-height: 1.5; }
  .token-hint strong { color: var(--text-secondary); }
  .token-hint code { font-family: var(--font-code); font-size: 10px; color: var(--accent); }
  .token-input-wrap { display: flex; gap: 6px; }
  .token-input {
    flex: 1; padding: 6px 8px; font-size: 11px;
    font-family: var(--font-code);
    background: var(--bg-elevated); border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md); color: var(--text-primary); outline: none;
    transition: border-color var(--transition-fast);
  }
  .token-input:focus { border-color: var(--accent); }
  .token-toggle {
    display: flex; align-items: center; justify-content: center;
    width: 30px; height: 30px; flex-shrink: 0;
    background: var(--bg-elevated); border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md); color: var(--text-muted); cursor: pointer;
    transition: all var(--transition-fast);
  }
  .token-toggle:hover { color: var(--text-secondary); border-color: var(--border); }
  .btn-primary {
    display: flex; align-items: center; gap: 5px;
    padding: 6px 14px; font-size: 11px; font-weight: 600;
    background: var(--accent); color: var(--text-on-accent);
    border: none; border-radius: var(--radius-md); cursor: pointer;
    font-family: var(--font-ui-sans); white-space: nowrap;
    transition: background var(--transition-fast);
  }
  .btn-primary:hover { background: var(--accent-hover); }
  .btn-primary:disabled { opacity: 0.5; cursor: default; }
  .token-error { font-size: 10px; color: var(--status-error, #f87171); margin: 0; }

  :global(.spin) { animation: spin 1s linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
</style>
