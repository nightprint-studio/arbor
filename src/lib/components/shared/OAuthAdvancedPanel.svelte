<script lang="ts">
  import { Loader2, Save, RotateCcw, Info } from 'lucide-svelte';
  import {
    getOAuthOverrides, setOAuthOverrides, getOAuthDefaults,
    type OAuthOverrides, type OAuthDefaults,
  } from '$lib/ipc/auth';
  import { uiStore } from '$lib/stores/ui.svelte';

  type Provider = 'github' | 'gitlab' | 'linear' | 'jira';

  interface Props {
    provider: Provider;
    /** Called after a save succeeds — useful for callers that need to refresh UI. */
    onsaved?: () => void;
  }

  let { provider, onsaved }: Props = $props();

  let loading   = $state(true);
  let saving    = $state(false);
  let error     = $state('');
  let overrides = $state<OAuthOverrides | null>(null);
  let defaults  = $state<OAuthDefaults | null>(null);

  // Local-edit buffers (separated from the saved state so the inputs don't
  // immediately commit on keystroke).
  let clientId = $state('');
  let baseHost = $state('');

  $effect(() => { void load(); });

  async function load() {
    loading = true; error = '';
    try {
      const [o, d] = await Promise.all([getOAuthOverrides(), getOAuthDefaults()]);
      overrides = o;
      defaults  = d;
      clientId = pickClientId(o, provider);
      baseHost = (provider === 'gitlab' ? (o.gitlab.base_host ?? '') : '');
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  function pickClientId(o: OAuthOverrides, p: Provider): string {
    return (o[p].client_id ?? '') as string;
  }

  const defaultClientId = $derived.by(() => {
    if (!defaults) return '';
    switch (provider) {
      case 'github': return defaults.github_client_id;
      case 'gitlab': return defaults.gitlab_client_id;
      case 'linear': return defaults.linear_client_id;
      case 'jira':   return defaults.jira_client_id;
    }
  });

  const defaultBaseHost = $derived(defaults?.gitlab_base_host ?? 'gitlab.com');

  const dirty = $derived.by(() => {
    if (!overrides) return false;
    const savedCid = (overrides[provider].client_id ?? '') as string;
    if (clientId.trim() !== savedCid.trim()) return true;
    if (provider === 'gitlab') {
      const savedHost = (overrides.gitlab.base_host ?? '') as string;
      if (baseHost.trim() !== savedHost.trim()) return true;
    }
    return false;
  });

  async function save() {
    if (!overrides) return;
    saving = true; error = '';
    try {
      const next: OAuthOverrides = {
        github: { ...overrides.github },
        gitlab: { ...overrides.gitlab },
        linear: { ...overrides.linear },
        jira:   { ...overrides.jira },
      };
      const cid = clientId.trim();
      next[provider].client_id = cid === '' ? null : cid;
      if (provider === 'gitlab') {
        const bh = baseHost.trim();
        next.gitlab.base_host = bh === '' ? null : bh;
      }
      await setOAuthOverrides(next);
      overrides = next;
      uiStore.showToast('OAuth settings saved', 'success');
      onsaved?.();
    } catch (e) {
      error = String(e);
    } finally {
      saving = false;
    }
  }

  function resetToDefault() {
    clientId = '';
    if (provider === 'gitlab') baseHost = '';
  }

  const labels: Record<Provider, string> = {
    github: 'GitHub OAuth App',
    gitlab: 'GitLab OAuth Application',
    linear: 'Linear OAuth Application',
    jira:   'Atlassian OAuth 2.0 (3LO) App',
  };

  const setupUrls: Record<Provider, string> = {
    github: 'github.com → Settings → Developer settings → OAuth Apps',
    gitlab: 'gitlab.com → Preferences → Applications',
    linear: 'linear.app → Settings → API → OAuth applications',
    jira:   'developer.atlassian.com → OAuth 2.0 (3LO)',
  };

  const callbackHints: Record<Provider, string> = {
    github: 'Device Flow — no callback URL needed',
    gitlab: 'Redirect URI: http://127.0.0.1:7731/callback',
    linear: 'Redirect URI: http://127.0.0.1:7729/callback',
    jira:   'Redirect URI: http://127.0.0.1:7730/callback',
  };
</script>

<div class="oauth-advanced">
  {#if loading}
    <div class="adv-loading"><Loader2 size={12} class="spin" /> Loading…</div>
  {:else}
    <div class="adv-intro">
      <Info size={11} />
      <span>
        Override the bundled <strong>{labels[provider]}</strong>.
        Register your own at <code>{setupUrls[provider]}</code>.
        <span class="adv-hint">{callbackHints[provider]}</span>
      </span>
    </div>

    <label class="adv-field">
      <span class="adv-label">Client ID</span>
      <input
        class="adv-input"
        type="text"
        placeholder={defaultClientId}
        bind:value={clientId}
        spellcheck="false"
        autocomplete="off"
      />
      <span class="adv-help">Leave empty to use Arbor's bundled OAuth app.</span>
    </label>

    {#if provider === 'gitlab'}
      <label class="adv-field">
        <span class="adv-label">Base host</span>
        <input
          class="adv-input"
          type="text"
          placeholder={defaultBaseHost}
          bind:value={baseHost}
          spellcheck="false"
          autocomplete="off"
        />
        <span class="adv-help">Self-hosted GitLab — e.g. <code>gitlab.company.com</code>.</span>
      </label>
    {/if}

    {#if error}
      <div class="adv-error">{error}</div>
    {/if}

    <div class="adv-actions">
      <button class="adv-btn-save" onclick={save} disabled={saving || !dirty}>
        {#if saving}<Loader2 size={11} class="spin" />{:else}<Save size={11} />{/if}
        Save
      </button>
      <button class="adv-btn-reset" onclick={resetToDefault} disabled={saving}>
        <RotateCcw size={11} /> Reset to default
      </button>
    </div>
  {/if}
</div>

<style>
  .oauth-advanced {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 12px 14px;
    background: var(--bg-overlay);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
  }

  .adv-loading {
    display: flex; align-items: center; gap: 6px;
    font-size: 11px; color: var(--text-muted);
  }

  .adv-intro {
    display: flex; gap: 6px; align-items: flex-start;
    font-size: 11px; color: var(--text-muted); line-height: 1.5;
  }
  .adv-intro :global(svg) { flex-shrink: 0; margin-top: 2px; }
  .adv-intro strong { color: var(--text-secondary); font-weight: 500; }
  .adv-intro code {
    font-family: var(--font-code); font-size: 10px;
    color: var(--accent);
    background: var(--bg-base); border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm); padding: 0 4px;
  }
  .adv-hint { display: block; margin-top: 2px; color: var(--text-disabled); }

  .adv-field { display: flex; flex-direction: column; gap: 4px; }
  .adv-label {
    font-size: 11px; font-weight: 500; color: var(--text-secondary);
  }
  .adv-input {
    padding: 5px 8px;
    background: var(--bg-input); color: var(--text-primary);
    border: 1px solid var(--border); border-radius: var(--radius-sm);
    font-family: var(--font-code); font-size: 12px;
    outline: none; transition: border-color var(--transition-fast);
  }
  .adv-input:focus { border-color: var(--accent); }
  .adv-input::placeholder {
    color: var(--text-disabled);
    font-family: var(--font-code);
  }
  .adv-help { font-size: 10.5px; color: var(--text-muted); }
  .adv-help code {
    font-family: var(--font-code); font-size: 10px;
    color: var(--accent);
    background: var(--bg-base); border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm); padding: 0 3px;
  }

  .adv-error {
    font-size: 11px; color: var(--error);
    padding: 6px 8px;
    background: color-mix(in srgb, var(--error) 8%, transparent);
    border: 1px solid color-mix(in srgb, var(--error) 40%, transparent);
    border-radius: var(--radius-sm);
  }

  .adv-actions { display: flex; gap: 8px; }

  .adv-btn-save, .adv-btn-reset {
    display: inline-flex; align-items: center; gap: 5px;
    padding: 5px 12px;
    font-family: var(--font-ui-sans); font-size: 11px; font-weight: 500;
    border-radius: var(--radius-sm); cursor: pointer;
    transition: all var(--transition-fast); white-space: nowrap;
  }
  .adv-btn-save {
    background: var(--accent); color: var(--text-on-accent);
    border: 1px solid var(--accent);
  }
  .adv-btn-save:hover:not(:disabled) { filter: brightness(1.1); }
  .adv-btn-save:disabled { opacity: 0.4; cursor: not-allowed; }
  .adv-btn-reset {
    background: transparent; color: var(--text-muted);
    border: 1px solid var(--border);
  }
  .adv-btn-reset:hover:not(:disabled) { background: var(--bg-hover); color: var(--text-primary); }
  .adv-btn-reset:disabled { opacity: 0.4; cursor: not-allowed; }

  :global(.spin) { animation: spin-anim 1.2s linear infinite; }
  @keyframes spin-anim { to { transform: rotate(360deg); } }
</style>
