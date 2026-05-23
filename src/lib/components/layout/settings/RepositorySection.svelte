<script lang="ts">
  import { FolderGit2 } from 'lucide-svelte';
  import { tabsStore } from '$lib/stores/tabs.svelte';
  import { getRepoConfig, setRepoConfig } from '$lib/ipc/config';
  import type { RepoConfig } from '$lib/ipc/config';
  import { commitConfigStore } from '$lib/stores/commit_config.svelte';
  import { uiStore } from '$lib/stores/ui.svelte';
  import SectionHeader from '$lib/components/shared/ui/SectionHeader.svelte';
  import FormRow from '$lib/components/shared/ui/FormRow.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';

  const commitTemplate = $derived(commitConfigStore.templateGlobal);

  const tab = $derived(tabsStore.activeTab);
  let repoConfig        = $state<RepoConfig | null>(null);
  let repoConfigLoading = $state(false);
  let repoConfigSaving  = $state(false);
  let repoConfigError   = $state('');
  let repoConfigDirty   = $state(false);

  $effect(() => {
    if (tab) {
      repoConfigLoading = true;
      repoConfigError   = '';
      getRepoConfig(tab.id)
        .then(cfg => { repoConfig = cfg; repoConfigDirty = false; })
        .catch(err => { repoConfigError = String(err); })
        .finally(() => { repoConfigLoading = false; });
    }
  });

  async function saveRepo() {
    if (!tab || !repoConfig) return;
    repoConfigSaving = true;
    repoConfigError  = '';
    try {
      await setRepoConfig(tab.id, repoConfig);
      repoConfigDirty = false;
      uiStore.showToast('Repository settings saved', 'success');
    } catch (err) {
      repoConfigError = String(err);
    } finally {
      repoConfigSaving = false;
    }
  }

  function markDirty() { repoConfigDirty = true; }
</script>

<SectionHeader title="Repository Settings" description="Per-project overrides, stored in .arbor/config.toml alongside your repository." />

<!-- Global commit template (persisted in ~/.config/arbor/config.toml under [commit]). -->
<div class="card">
  <div class="card-section-title">Commit Template</div>
  <div class="card-row-note">
    Pre-fills the commit message field when empty. If the repository has a git <code>commit.template</code> config, that takes priority.
  </div>
  <div class="card-row card-row-column">
    <textarea
      class="template-textarea"
      placeholder="Enter a default commit message template…"
      rows="5"
      value={commitTemplate}
      oninput={(e) => commitConfigStore.setTemplateGlobal((e.target as HTMLTextAreaElement).value)}
    ></textarea>
  </div>
</div>

{#if !tab}
  <div class="empty-state">
    <FolderGit2 size={20} />
    <span>No repository open</span>
    <span class="empty-hint">Open a repository to configure project-specific settings.</span>
  </div>

{:else if repoConfigLoading}
  <div class="empty-state">
    <span class="loading-dots">Loading…</span>
  </div>

{:else if repoConfig}
  <div class="card">
    <div class="card-section-title">Display</div>

    <FormRow label="Display name" description="Friendly name shown in the tab bar">
      <Input
        type="text"
        placeholder={tab.name}
        bind:value={repoConfig.display_name}
        oninput={() => markDirty()}
      />
    </FormRow>

    <FormRow label="Default remote" description="Used for fetch/pull/push when unspecified">
      <Input
        type="text"
        placeholder="origin"
        bind:value={repoConfig.default_remote}
        onchange={markDirty}
      />
    </FormRow>
  </div>

  <div class="card">
    <div class="card-section-title">Author Identity Override</div>
    <div class="card-row-note">
      Overrides the global Git user for commits in this repository only.
      Leave blank to use the global <code>user.name</code> / <code>user.email</code>.
    </div>

    <FormRow label="Name" description="Author name for new commits">
      <Input
        type="text"
        placeholder="Your Name"
        bind:value={repoConfig.user.name}
        onchange={markDirty}
      />
    </FormRow>

    <FormRow label="Email" description="Author email for new commits">
      <Input
        type="text"
        placeholder="you@example.com"
        bind:value={repoConfig.user.email}
        onchange={markDirty}
      />
    </FormRow>
  </div>

  {#if repoConfigError}
    <p class="form-error">{repoConfigError}</p>
  {/if}

  <div class="form-actions">
    <button
      class="btn-primary"
      onclick={saveRepo}
      disabled={repoConfigSaving || !repoConfigDirty}
    >
      {repoConfigSaving ? 'Saving…' : 'Save Changes'}
    </button>
    {#if !repoConfigDirty && !repoConfigSaving}
      <span class="saved-label">All changes saved</span>
    {/if}
  </div>

{:else if repoConfigError}
  <p class="form-error">{repoConfigError}</p>
{/if}
