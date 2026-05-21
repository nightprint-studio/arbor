<script lang="ts">
  import { onMount } from 'svelte';
  import { Link2, ShieldCheck, ListChecks, MessagesSquare, Settings as SettingsIcon, Share2 } from 'lucide-svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import {
    getDeepLinkConfig, setDeepLinkConfig,
  } from '$lib/ipc/deep-link';
  import type {
    DeepLinkConfig, ConfirmConfig, EnableConfig, CrossWorkspaceStrategy,
  } from '$lib/types/deep-link';
  import { uiStore } from '$lib/stores/ui.svelte';
  import { invalidateWorkerBaseUrlCache } from '$lib/utils/deep-link-builder';
  import SectionHeader from '$lib/components/shared/ui/SectionHeader.svelte';
  import FormRow from '$lib/components/shared/ui/FormRow.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import Toggle from '$lib/components/shared/ui/Toggle.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import Card from '$lib/components/shared/ui/Card.svelte';

  let cfg     = $state<DeepLinkConfig | null>(null);
  let loading = $state(true);
  let saving  = $state(false);

  onMount(async () => {
    try { cfg = await getDeepLinkConfig(); }
    catch (e) { uiStore.showToast(`Failed to load: ${e}`, 'error'); }
    finally { loading = false; }
  });

  async function persist(next: DeepLinkConfig) {
    saving = true;
    try {
      await setDeepLinkConfig(next);
      cfg = next;
      invalidateWorkerBaseUrlCache();
    } catch (e) {
      uiStore.showToast(`Save failed: ${e}`, 'error');
    } finally {
      saving = false;
    }
  }

  /** Debounced commit for the worker host text field so we don't fire an
   *  IPC + file write on every keystroke. */
  let workerSaveTimer: ReturnType<typeof setTimeout> | null = null;
  function onWorkerBaseUrlInput(v: string) {
    if (!cfg) return;
    // Update the in-memory cfg immediately so the input remains controlled.
    cfg = { ...cfg, worker_base_url: v };
    if (workerSaveTimer) clearTimeout(workerSaveTimer);
    workerSaveTimer = setTimeout(() => {
      if (cfg) void persist({ ...cfg, worker_base_url: v });
    }, 400);
  }

  // ── Derived ───────────────────────────────────────────────────────────
  const masterOn = $derived(cfg?.enabled ?? false);

  const strategyOptions = [
    { value: 'switch',    label: 'Switch to that workspace' },
    { value: 'open_here', label: 'Open here as cross-workspace tab' },
  ];

  // ── Persist helpers ───────────────────────────────────────────────────
  function setEnabled(v: boolean) {
    if (!cfg) return;
    void persist({ ...cfg, enabled: v });
  }
  function setEnableAction(key: keyof EnableConfig, v: boolean) {
    if (!cfg) return;
    void persist({ ...cfg, enable: { ...cfg.enable, [key]: v } });
  }
  function setStrategy(v: string) {
    if (!cfg) return;
    void persist({ ...cfg, cross_workspace_strategy: v as CrossWorkspaceStrategy });
  }
  function setCheckoutAsWorktree(v: boolean) {
    if (!cfg) return;
    void persist({ ...cfg, checkout_as_worktree: v });
  }
  function setConfirm(key: keyof ConfirmConfig, v: boolean) {
    if (!cfg) return;
    void persist({ ...cfg, confirm: { ...cfg.confirm, [key]: v } });
  }

  // ── Row catalogues ────────────────────────────────────────────────────
  type ActionRow<T> = { key: T; label: string; description: string };

  const enableRows: ActionRow<keyof EnableConfig>[] = [
    { key: 'repo_open',
      label: 'Open repository',
      description: 'Allow `arbor://repo/open` links — activates an existing tab or prompts to clone.' },
    { key: 'commit_jump',
      label: 'Jump to commit',
      description: 'Allow `arbor://commit/<sha>` links — read-only, scrolls the graph.' },
    { key: 'branch_checkout',
      label: 'Branch checkout',
      description: 'Allow `arbor://branch/<name>?checkout=1` links. Mutates HEAD on the main checkout — keep off if your shared workflow always uses worktrees.' },
    { key: 'branch_worktree',
      label: 'Branch worktree',
      description: 'Allow `arbor://branch/<name>?worktree=1` links — opens the Add Worktree dialog pre-filled.' },
    { key: 'mr_open',
      label: 'Open MR / PR',
      description: 'Allow `arbor://mr/open/<number>` links — fetches MR detail.' },
    { key: 'pipeline_open',
      label: 'Open CI pipeline',
      description: 'Allow `arbor://pipeline/<run-id>` links — fetches CI run detail.' },
  ];

  const confirmRows: ActionRow<keyof ConfirmConfig>[] = [
    { key: 'repo_open',
      label: 'Confirm "open repository"',
      description: 'Show the prompt before activating or cloning a repo.' },
    { key: 'commit_jump',
      label: 'Confirm "jump to commit"',
      description: 'Show the prompt before scrolling the graph to a commit.' },
    { key: 'branch_checkout',
      label: 'Confirm "checkout branch"',
      description: 'Show the prompt before running a stash-safe checkout. Strongly recommended — checkouts move HEAD on your main worktree.' },
    { key: 'branch_worktree',
      label: 'Confirm "create worktree"',
      description: 'Show the prompt before opening the Add Worktree dialog.' },
    { key: 'mr_open',
      label: 'Confirm "open MR / PR"',
      description: 'Show the prompt before fetching merge-request detail.' },
    { key: 'pipeline_open',
      label: 'Confirm "open CI pipeline"',
      description: 'Show the prompt before fetching CI run detail.' },
  ];
</script>

<SectionHeader
  title="Deep Links"
  description="How Arbor handles incoming `arbor://…` URLs (e.g. shared by a colleague or a CI bot)."
/>

{#if loading}
  <div class="empty-state">
    <Spinner size="sm" label="Loading…" />
  </div>
{:else if cfg}

  <!-- ── Master kill-switch ──────────────────────────────────────────── -->
  <Card padding="md">
    {#snippet header()}
      <span class="card-hdr"><ShieldCheck size={13} /> Master switch</span>
    {/snippet}

    {#if !masterOn}
      <Alert
        variant="warning"
        title="Deep links are off"
        text="Incoming arbor:// URLs are blocked and shown a notice modal. Enable to start processing them — every action kind is still individually opt-in below."
      />
    {/if}

    <FormRow
      label="Enable deep links"
      description="Master kill-switch. Default: off (CYA — sharing a link should never silently mutate Arbor on first install). Turn on, then opt in each action kind separately."
    >
      <Toggle
        checked={cfg.enabled}
        onchange={setEnabled}
        disabled={saving}
      />
    </FormRow>
  </Card>

  <!-- ── Per-action enable toggles ───────────────────────────────────── -->
  <Card padding="md">
    {#snippet header()}
      <span class="card-hdr"><ListChecks size={13} /> Enabled actions</span>
    {/snippet}

    <Alert
      variant="info"
      compact
      text="Each action kind is opt-in. The master switch above is required as well — turning a row on here has no effect until the master is enabled."
    />

    {#each enableRows as row}
      <FormRow label={row.label} description={row.description}>
        <Toggle
          checked={cfg.enable[row.key]}
          onchange={(v) => setEnableAction(row.key, v)}
          disabled={saving || !masterOn}
        />
      </FormRow>
    {/each}
  </Card>

  <!-- ── Routing behaviour ───────────────────────────────────────────── -->
  <Card padding="md">
    {#snippet header()}
      <span class="card-hdr"><SettingsIcon size={13} /> Routing</span>
    {/snippet}

    <FormRow
      label="Cross-workspace target"
      description="When a deep link points to a repo that's a member of another workspace, do you want Arbor to switch to that workspace, or surface the repo as a cross-workspace tab in the workspace you're already focused on?"
    >
      <Select
        value={cfg.cross_workspace_strategy}
        options={strategyOptions}
        onchange={setStrategy}
        disabled={saving}
      />
    </FormRow>

    <FormRow
      label="Checkout links create a worktree"
      description="Rewrite incoming `arbor://branch/<name>?checkout=1` to the worktree variant before dispatch. Useful when every shared branch should become its own worktree, so the link never moves HEAD on your main checkout. Doesn't affect links you copy out of Arbor."
    >
      <Toggle
        checked={cfg.checkout_as_worktree}
        onchange={setCheckoutAsWorktree}
        disabled={saving}
      />
    </FormRow>
  </Card>

  <!-- ── Confirmations ───────────────────────────────────────────────── -->
  <Card padding="md">
    {#snippet header()}
      <span class="card-hdr"><MessagesSquare size={13} /> Confirmations</span>
    {/snippet}

    <Alert
      variant="info"
      compact
      text="Every enabled action shows an 'Are you sure?' prompt by default. Disable individual entries below for actions you consistently trust. The clone-confirm modal is independent of these toggles — a missing local copy always asks for consent (you need to pick the destination folder)."
    />

    {#each confirmRows as row}
      <FormRow label={row.label} description={row.description}>
        <Toggle
          checked={cfg.confirm[row.key]}
          onchange={(v) => setConfirm(row.key, v)}
          disabled={saving}
        />
      </FormRow>
    {/each}
  </Card>

  <!-- ── Sharing / HTTPS redirect worker ─────────────────────────────── -->
  <Card padding="md">
    {#snippet header()}
      <span class="card-hdr"><Share2 size={13} /> Sharing (HTTPS redirect)</span>
    {/snippet}

    <Alert
      variant="info"
      compact
      text="Chat platforms like Google Chat, Slack and Teams refuse to render `arbor://…` URLs as clickable. Arbor solves this by emitting `https://<worker>/<path>?<query>` instead — a tiny redirect worker (Cloudflare Worker, by default) bounces the request back to the equivalent `arbor://` URL, which the OS then hands to Arbor."
    />

    <FormRow
      label="Redirect worker host"
      description="Domain of the HTTPS redirect worker (no scheme, no trailing slash). Every link you copy from Arbor will start with `https://<this-domain>/`. Leave empty to copy raw `arbor://` URLs instead — useful for offline / private deployments."
    >
      <Input
        value={cfg.worker_base_url}
        placeholder="arbor-redirect.nightprint-studio.workers.dev"
        oninput={onWorkerBaseUrlInput}
        disabled={saving}
        clearable
      />
    </FormRow>

    <div class="url-preview">
      <span class="url-preview-label">Sample output</span>
      <code>
        {#if cfg.worker_base_url && cfg.worker_base_url.trim()}
          https://{cfg.worker_base_url.trim().replace(/^https?:\/\//i, '').replace(/\/+$/, '')}/mr/open/8?url=&lt;git-url&gt;
        {:else}
          arbor://mr/open/8?url=&lt;git-url&gt;
        {/if}
      </code>
    </div>
  </Card>

  <!-- ── Reference ───────────────────────────────────────────────────── -->
  <Card padding="md">
    {#snippet header()}
      <span class="card-hdr"><Link2 size={13} /> Supported URLs</span>
    {/snippet}

    <p class="card-note">
      The redirect worker rewrites <code>https://&lt;worker&gt;/&lt;path&gt;</code>
      back to <code>arbor://&lt;path&gt;</code>, so the two forms are interchangeable.
      The table below uses the canonical <code>arbor://</code> shape.
    </p>

    <ul class="url-list">
      <li><code>arbor://repo/open?url=&lt;git-url&gt;</code> — open or clone</li>
      <li><code>arbor://commit/&lt;sha&gt;?url=&lt;git-url&gt;</code> — jump to a commit</li>
      <li><code>arbor://branch/&lt;name&gt;?url=&lt;git-url&gt;&amp;checkout=1</code> — check out a branch</li>
      <li><code>arbor://branch/&lt;name&gt;?url=&lt;git-url&gt;&amp;worktree=1</code> — create a worktree on a branch</li>
      <li><code>arbor://mr/open/&lt;number&gt;?url=&lt;git-url&gt;</code> — open a merge / pull request</li>
      <li><code>arbor://pipeline/&lt;run-id&gt;?url=&lt;git-url&gt;</code> — open a CI pipeline run</li>
    </ul>

    <Alert
      variant="info"
      compact
      text="<git-url> is the remote git URL (HTTPS or SSH). Arbor matches it against your registered repositories using a fuzzy host/owner/repo key, so it doesn't matter whether the link author used HTTPS while you cloned with SSH."
    />
  </Card>
{/if}

<style>
  /* Vertical stack of cards. */
  :global(.card) + :global(.card) { margin-top: 12px; }

  .empty-state {
    display: flex; align-items: center; gap: 8px;
    color: var(--text-muted);
    font-size: var(--font-size-sm);
    padding: 12px;
  }

  .card-hdr {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    color: var(--text-secondary);
    font-size: 11.5px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.4px;
  }

  .url-list {
    list-style: none;
    margin: 0 0 10px 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .url-list li {
    color: var(--text-secondary);
    font-size: var(--font-size-sm);
    line-height: 1.5;
  }
  .url-list code {
    font-family: var(--font-code);
    font-size: 11.5px;
    padding: 1px 5px;
    background: var(--bg-overlay);
    border: 1px solid var(--border);
    border-radius: 3px;
    color: var(--text-primary);
  }

  .card-note {
    margin: 0 0 10px 0;
    color: var(--text-secondary);
    font-size: var(--font-size-sm);
    line-height: 1.5;
  }
  .card-note code {
    font-family: var(--font-code);
    font-size: 11.5px;
    padding: 1px 5px;
    background: var(--bg-overlay);
    border: 1px solid var(--border);
    border-radius: 3px;
    color: var(--text-primary);
  }

  .url-preview {
    margin-top: 10px;
    padding: 8px 10px;
    background: var(--bg-overlay);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .url-preview-label {
    font-size: 10.5px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.4px;
    color: var(--text-muted);
  }
  .url-preview code {
    font-family: var(--font-code);
    font-size: 11.5px;
    color: var(--text-primary);
    word-break: break-all;
  }

</style>
