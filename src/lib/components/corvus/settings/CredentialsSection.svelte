<script lang="ts">
  import { KeyRound, Plus, Trash2, Eye, EyeOff } from 'lucide-svelte';
  import {
    saveCredential, deleteCredential,
    saveDefaultCredential, deleteDefaultCredential,
  } from '$lib/ipc/auth';
  import { uiStore } from '$lib/stores/ui.svelte';
  import SectionHeader from '$lib/components/shared/ui/SectionHeader.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import FormRow from '$lib/components/shared/ui/FormRow.svelte';
  import { tooltip } from '$lib/actions/tooltip';

  const CRED_LIST_KEY = 'arbor:credentials';
  interface CredEntry { host: string; username: string; isDefault?: boolean }

  const PROVIDERS = [
    { value: 'github.com',    label: 'GitHub'       },
    { value: 'gitlab.com',    label: 'GitLab'       },
    { value: 'bitbucket.org', label: 'Bitbucket'    },
    { value: 'dev.azure.com', label: 'Azure DevOps' },
    { value: 'custom',        label: 'Custom…'      },
  ];

  let credList      = $state<CredEntry[]>(JSON.parse(localStorage.getItem(CRED_LIST_KEY) ?? '[]'));
  let newProvider   = $state('github.com');
  let newCustomHost = $state('');
  let newUsername   = $state('');
  let newPassword   = $state('');
  let showPassword  = $state(false);
  let credSaving    = $state(false);
  let credError     = $state('');

  const effectiveHost = $derived(newProvider === 'custom' ? newCustomHost.trim() : newProvider);

  function saveCredList(list: CredEntry[]) {
    localStorage.setItem(CRED_LIST_KEY, JSON.stringify(list));
  }

  async function addCredential() {
    if (!effectiveHost || !newUsername.trim() || !newPassword.trim()) {
      credError = 'All fields are required.';
      return;
    }
    credError  = '';
    credSaving = true;
    try {
      await saveCredential(effectiveHost, newUsername.trim(), newPassword.trim());
      await saveDefaultCredential(effectiveHost, newUsername.trim(), newPassword.trim());
      const entry: CredEntry = { host: effectiveHost, username: newUsername.trim(), isDefault: true };
      const updated = credList.map(c =>
        c.host === effectiveHost ? { ...c, isDefault: false } : c
      );
      credList = [...updated.filter(c => !(c.host === entry.host && c.username === entry.username)), entry];
      saveCredList(credList);
      newProvider = 'github.com'; newCustomHost = ''; newUsername = ''; newPassword = '';
      uiStore.showToast('Credential saved to OS keychain', 'success');
    } catch (err) {
      credError = String(err);
    } finally {
      credSaving = false;
    }
  }

  async function removeCredential(entry: CredEntry) {
    try {
      await deleteCredential(entry.host, entry.username);
    } catch { /* ignore if not found */ }
    try {
      await deleteDefaultCredential(entry.host);
    } catch { /* ignore if no default set */ }
    credList = credList.filter(c => !(c.host === entry.host && c.username === entry.username));
    saveCredList(credList);
    uiStore.showToast('Credential removed', 'info');
  }
</script>

<SectionHeader title="Git Credentials" description="Passwords and tokens are stored securely in the OS keychain." />

{#if credList.length > 0}
  <div class="card cred-list">
    {#each credList as cred (cred.host + cred.username)}
      <div class="card-row cred-row">
        <div class="cred-icon-wrap">
          <KeyRound size={14} />
        </div>
        <div class="cred-details">
          <span class="cred-host">{cred.host}</span>
          <span class="cred-username">{cred.username}</span>
          {#if cred.isDefault}
            <span class="cred-default-badge">default</span>
          {/if}
        </div>
        <button
          class="icon-btn danger"
          use:tooltip={'Remove credential'}
          onclick={() => removeCredential(cred)}
        >
          <Trash2 size={12} />
        </button>
      </div>
    {/each}
  </div>
{:else}
  <div class="empty-state">
    <KeyRound size={20} />
    <span>No credentials saved yet</span>
  </div>
{/if}

<div class="card cred-form">
  <div class="card-section-title"><Plus size={12} /> Add Credential</div>

  <FormRow label="Provider" description="Select a preset or choose Custom">
    {#snippet children()}
      <Select bind:value={newProvider} options={PROVIDERS} />
    {/snippet}
  </FormRow>

  {#if newProvider === 'custom'}
    <FormRow label="Host" description="e.g. git.company.com">
      {#snippet children()}
        <Input type="text" placeholder="git.company.com" bind:value={newCustomHost} />
      {/snippet}
    </FormRow>
  {/if}

  <FormRow label="Username" description="Your Git username">
    {#snippet children()}
      <Input type="text" placeholder="your-username" bind:value={newUsername} />
    {/snippet}
  </FormRow>

  <FormRow label="Token / Password" description="Personal Access Token or password">
    {#snippet children()}
      <div class="input-with-addon">
        <input
          class="text-input"
          type={showPassword ? 'text' : 'password'}
          placeholder="ghp_…"
          bind:value={newPassword}
        />
        <button
          class="addon-btn"
          use:tooltip={showPassword ? 'Hide' : 'Show'}
          onclick={() => showPassword = !showPassword}
        >
          {#if showPassword}<EyeOff size={12} />{:else}<Eye size={12} />{/if}
        </button>
      </div>
    {/snippet}
  </FormRow>

  {#if credError}
    <p class="form-error">{credError}</p>
  {/if}

  <div class="form-actions">
    <button class="btn-primary" onclick={addCredential} disabled={credSaving}>
      {credSaving ? 'Saving…' : 'Save to Keychain'}
    </button>
  </div>
</div>

<style>
  .cred-list :global(.card-row) { gap: 10px; }

  .cred-icon-wrap {
    width: 28px;
    height: 28px;
    border-radius: var(--radius-sm);
    background: var(--bg-overlay);
    border: 1px solid var(--border-subtle);
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--text-muted);
    flex-shrink: 0;
  }

  .cred-details {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 1px;
  }
  .cred-host {
    font-size: 12px;
    color: var(--text-primary);
    font-weight: 500;
  }
  .cred-username {
    font-size: 11px;
    color: var(--text-muted);
  }
  .cred-default-badge {
    display: inline-flex;
    align-items: center;
    font-size: 9px;
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--accent);
    background: color-mix(in srgb, var(--accent) 14%, transparent);
    border: 1px solid color-mix(in srgb, var(--accent) 35%, transparent);
    border-radius: var(--radius-sm);
    padding: 1px 5px;
    margin-top: 2px;
    width: fit-content;
  }
  .cred-row { transition: background var(--transition-fast); }
</style>
