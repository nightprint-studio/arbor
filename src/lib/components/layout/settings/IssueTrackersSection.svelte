<script lang="ts">
  import { XCircle, Eye, EyeOff, ArrowUp, ArrowDown, ChevronDown, ChevronRight, Settings2 } from 'lucide-svelte';
  import SplitButton from '$lib/components/shared/ui/SplitButton.svelte';
  import OAuthAdvancedPanel from '$lib/components/shared/OAuthAdvancedPanel.svelte';
  import ProviderConnectionStatus from '$lib/components/shared/internal/ProviderConnectionStatus.svelte';
  import OAuthBrowserAuthForm from '$lib/components/shared/internal/OAuthBrowserAuthForm.svelte';
  import type { IssueSortField, IssueSortDir } from '$lib/types/issues';
  import { SORT_FIELD_LABELS } from '$lib/types/issues';
  import { openUrl } from '@tauri-apps/plugin-opener';
  import { listen } from '@tauri-apps/api/event';
  import { startLinearOAuth, disconnectLinearOAuth, startJiraOAuth, disconnectJira } from '$lib/ipc/auth';
  import { issuesStore } from '$lib/stores/issues.svelte';
  import { jiraGetAuthStatus } from '$lib/ipc/issues';
  import { uiStore } from '$lib/stores/ui.svelte';
  import SectionHeader from '$lib/components/shared/ui/SectionHeader.svelte';
  import FormRow from '$lib/components/shared/ui/FormRow.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import BrandTile from '$lib/components/shared/internal/BrandTile.svelte';
  import ProviderUserBadge from '$lib/components/shared/internal/ProviderUserBadge.svelte';

  type ConnState  = 'checking' | 'disconnected' | 'connecting' | 'connected';
  type AuthMethod = 'oauth' | 'pat' | 'basic';

  // ── Linear ───────────────────────────────────────────────────────────────
  let linState        = $state<ConnState>('checking');
  let linMethod       = $state<AuthMethod | null>(null);
  let linPatInput     = $state('');
  let linShowPat      = $state(false);
  let linPatSaving    = $state(false);
  let linPatError     = $state('');
  let linOAuthWaiting = $state(false);
  let linOAuthError   = $state('');
  let linAdvancedOpen = $state(false);
  let linOAuthUnsub: (() => void) | null = null;

  // ── Jira ─────────────────────────────────────────────────────────────────
  let jiraState        = $state<ConnState>('checking');
  let jiraMethod       = $state<AuthMethod | null>(null);
  let jiraEmail        = $state('');
  let jiraApiToken     = $state('');
  let jiraDomain       = $state('');
  let jiraShowToken    = $state(false);
  let jiraBasicSaving  = $state(false);
  let jiraBasicError   = $state('');
  let jiraOAuthWaiting = $state(false);
  let jiraOAuthError   = $state('');
  let jiraAdvancedOpen = $state(false);
  let jiraOAuthUnsub: (() => void) | null = null;
  let jiraUser         = $state<{ displayName: string; email: string | null; avatarUrl: string | null } | null>(null);
  let jiraDomainDisplay = $state<string | null>(null);

  // ── Init ─────────────────────────────────────────────────────────────────

  $effect(() => {
    if (issuesStore.authStatus === null) issuesStore.loadAuthStatus();
  });

  $effect(() => {
    if (issuesStore.authStatus?.authenticated) linState = 'connected';
    else if (issuesStore.authStatus !== null) linState = 'disconnected';
  });

  $effect(() => {
    loadJiraStatus();
  });

  async function loadJiraStatus() {
    jiraState = 'checking';
    try {
      const s = await jiraGetAuthStatus();
      if (s.authenticated) {
        jiraState = 'connected';
        jiraUser  = s.user ? {
          displayName: s.user.displayName,
          email:       s.user.email,
          avatarUrl:   s.user.avatarUrl,
        } : null;
        jiraDomainDisplay = s.domain ?? null;
      } else {
        jiraState = 'disconnected';
      }
    } catch {
      jiraState = 'disconnected';
    }
  }

  // ── Linear handlers ───────────────────────────────────────────────────────

  function pickLinMethod(m: AuthMethod) {
    linMethod = m;
    linPatError = ''; linOAuthError = '';
    if (m === 'oauth') startLinearOAuthFlow();
  }

  async function startLinearOAuthFlow() {
    linOAuthWaiting = true; linOAuthError = '';
    linState = 'connecting';
    linOAuthUnsub?.();
    linOAuthUnsub = await listen<boolean>('arbor://linear-oauth-done', ({ payload }) => {
      linOAuthUnsub?.(); linOAuthUnsub = null;
      linOAuthWaiting = false;
      if (payload) {
        linState = 'connected'; linMethod = null;
        issuesStore.loadAuthStatus();
        uiStore.showToast('Linear connected via OAuth', 'success');
      } else {
        linState = 'disconnected';
        linOAuthError = 'OAuth failed — check your client ID or try again.';
      }
    });
    try {
      const url = await startLinearOAuth();
      try { await openUrl(url); } catch { /* user can copy */ }
    } catch (err) {
      linOAuthWaiting = false; linState = 'disconnected';
      linOAuthError = String(err);
      linOAuthUnsub?.(); linOAuthUnsub = null;
    }
  }

  async function saveLinearPat() {
    if (!linPatInput.trim()) return;
    linPatSaving = true; linPatError = '';
    try {
      await issuesStore.saveToken(linPatInput.trim());
      linPatInput = ''; linMethod = null; linState = 'connected';
      uiStore.showToast('Linear Personal API Key saved', 'success');
    } catch (e) { linPatError = String(e); } finally { linPatSaving = false; }
  }

  async function disconnectLinear() {
    linOAuthUnsub?.(); linOAuthUnsub = null; linOAuthWaiting = false;
    await issuesStore.logout().catch(() => {});
    await disconnectLinearOAuth().catch(() => {});
    linState = 'disconnected'; linMethod = null; linOAuthError = '';
    uiStore.showToast('Linear disconnected', 'info');
  }

  // ── Jira handlers ─────────────────────────────────────────────────────────

  function pickJiraMethod(m: AuthMethod) {
    jiraMethod = m;
    jiraBasicError = ''; jiraOAuthError = '';
    if (m === 'oauth') startJiraOAuthFlow();
  }

  async function startJiraOAuthFlow() {
    jiraOAuthWaiting = true; jiraOAuthError = '';
    jiraState = 'connecting';
    jiraOAuthUnsub?.();
    jiraOAuthUnsub = await listen<boolean>('arbor://jira-oauth-done', ({ payload }) => {
      jiraOAuthUnsub?.(); jiraOAuthUnsub = null;
      jiraOAuthWaiting = false;
      if (payload) {
        jiraState = 'connected'; jiraMethod = null;
        loadJiraStatus();
        uiStore.showToast('Jira connected via OAuth', 'success');
      } else {
        jiraState = 'disconnected';
        jiraOAuthError = 'OAuth failed — please retry.';
      }
    });
    try {
      const url = await startJiraOAuth();
      try { await openUrl(url); } catch { /* user can copy */ }
    } catch (err) {
      jiraOAuthWaiting = false; jiraState = 'disconnected';
      jiraOAuthError = String(err);
      jiraOAuthUnsub?.(); jiraOAuthUnsub = null;
    }
  }

  async function saveJiraBasicAuth() {
    const isCloud = jiraDomain.trim().endsWith('.atlassian.net');
    if (!jiraDomain.trim() || !jiraApiToken.trim() || (isCloud && !jiraEmail.trim())) return;
    jiraBasicSaving = true; jiraBasicError = '';
    try {
      await issuesStore.saveJiraBasicAuth(jiraEmail.trim(), jiraApiToken.trim(), jiraDomain.trim());
      jiraEmail = ''; jiraApiToken = ''; jiraDomain = '';
      jiraMethod = null; jiraState = 'connected';
      loadJiraStatus();
      uiStore.showToast('Jira connected with API token', 'success');
    } catch (e) {
      jiraBasicError = String(e);
    } finally {
      jiraBasicSaving = false;
    }
  }

  async function disconnectJiraAccount() {
    jiraOAuthUnsub?.(); jiraOAuthUnsub = null; jiraOAuthWaiting = false;
    await disconnectJira().catch(() => {});
    jiraState = 'disconnected'; jiraMethod = null;
    jiraOAuthError = ''; jiraBasicError = '';
    jiraUser = null; jiraDomainDisplay = null;
    uiStore.showToast('Jira disconnected', 'info');
  }
</script>

<SectionHeader title="Issue Trackers" description="Connect to project management tools. Tokens are stored in the OS keychain." />

<!-- ── Linear ─────────────────────────────────────────────────────────────── -->
<div class="provider-card" class:flow-active={linState === 'connecting'}>
  <BrandTile brand="linear" />
  <div class="provider-main">
    <div class="provider-top">
      <div class="provider-info">
        <span class="provider-name">Linear</span>
        <span class="provider-desc">Issue tracker — OAuth &amp; Personal API Key</span>
      </div>
      <ProviderConnectionStatus
        state={linState === 'checking' || issuesStore.authStatus === null
          ? 'checking'
          : linState === 'connected' && issuesStore.authStatus?.authenticated
            ? 'connected'
            : linState}
        onDisconnect={disconnectLinear}
        onCancel={() => { linOAuthWaiting = false; linState = 'disconnected'; linMethod = null; linOAuthUnsub?.(); }}
      >
        {#snippet connect()}
          {#if linMethod === null}
            <SplitButton
              label="Connect"
              color="var(--brand-linear)"
              direction="down"
              options={[
                { id: 'oauth', label: 'OAuth (recommended)' },
                { id: 'pat',   label: 'Personal API Key' },
              ]}
              onclick={() => pickLinMethod('oauth')}
              onselect={(id) => pickLinMethod(id as AuthMethod)}
            />
          {/if}
        {/snippet}
      </ProviderConnectionStatus>
    </div>

    {#if linState === 'connected' && issuesStore.authStatus?.user}
      {@const u = issuesStore.authStatus.user}
      <ProviderUserBadge
        avatarUrl={u.avatarUrl}
        name={u.displayName}
        secondary={u.email}
      />
    {/if}

    {#if linMethod === 'oauth'}
      <OAuthBrowserAuthForm
        waiting={linOAuthWaiting}
        error={linOAuthError}
        brandColor="var(--brand-linear)"
        hintIdle="Opens Linear in the browser to authorize Arbor."
        hintWaiting="Browser opened — approve access in Linear then return here."
        idleLabel="Authorize with Linear"
        busyLabel="Waiting for browser…"
        onAuthorize={startLinearOAuthFlow}
        onCancel={() => { linOAuthWaiting = false; linMethod = null; linOAuthError = ''; linOAuthUnsub?.(); }}
      />
    {/if}

    {#if linMethod === 'pat'}
      <div class="inline-form">
        <div class="inline-form-row">
          <div class="input-with-addon" style="flex:1">
            <input class="text-input" type={linShowPat ? 'text' : 'password'} placeholder="lin_api_…" bind:value={linPatInput} />
            <button class="addon-btn" onclick={() => linShowPat = !linShowPat}>{#if linShowPat}<EyeOff size={12} />{:else}<Eye size={12} />{/if}</button>
          </div>
          <button class="btn-save linear-btn" onclick={saveLinearPat} disabled={linPatSaving || !linPatInput.trim()}>
            {linPatSaving ? 'Saving…' : 'Save'}
          </button>
          <button class="btn-cancel" onclick={() => { linMethod = null; linPatError = ''; }}>Cancel</button>
        </div>
        <p class="form-hint">Generate at <code>linear.app → Settings → API → Personal API keys</code></p>
        {#if linPatError}<div class="provider-error"><XCircle size={12} />{linPatError}</div>{/if}
      </div>
    {/if}

    <button class="advanced-toggle" onclick={() => linAdvancedOpen = !linAdvancedOpen}>
      {#if linAdvancedOpen}<ChevronDown size={11} />{:else}<ChevronRight size={11} />{/if}
      <Settings2 size={11} />
      Advanced — use my own OAuth app
    </button>
    {#if linAdvancedOpen}
      <OAuthAdvancedPanel provider="linear" />
    {/if}
  </div>
</div>

<!-- ── Jira ───────────────────────────────────────────────────────────────── -->
<div class="provider-card" class:flow-active={jiraState === 'connecting'}>
  <BrandTile brand="jira" />
  <div class="provider-main">
    <div class="provider-top">
      <div class="provider-info">
        <span class="provider-name">Jira</span>
        <span class="provider-desc">Atlassian issue tracker — API Token &amp; OAuth</span>
      </div>
      <ProviderConnectionStatus
        state={jiraState}
        onDisconnect={disconnectJiraAccount}
        onCancel={() => { jiraOAuthWaiting = false; jiraState = 'disconnected'; jiraMethod = null; jiraOAuthUnsub?.(); }}
      >
        {#snippet connect()}
          {#if jiraMethod === null}
            <SplitButton
              label="Connect"
              color="var(--brand-jira)"
              direction="down"
              options={[
                { id: 'basic', label: 'API Token (recommended)' },
                { id: 'oauth', label: 'OAuth 2.0 (requires Atlassian app)' },
              ]}
              onclick={() => pickJiraMethod('basic')}
              onselect={(id) => pickJiraMethod(id as AuthMethod)}
            />
          {/if}
        {/snippet}
      </ProviderConnectionStatus>
    </div>

    {#if jiraState === 'connected' && jiraUser}
      <ProviderUserBadge
        avatarUrl={jiraUser.avatarUrl}
        name={jiraUser.displayName}
        secondary={jiraUser.email ?? jiraDomainDisplay}
      />
    {/if}

    <!-- Basic Auth form (API Token) -->
    {#if jiraMethod === 'basic'}
      {@const isCloud = jiraDomain.trim().endsWith('.atlassian.net')}
      <div class="inline-form">
        <p class="form-hint">
          {#if isCloud}
            Jira Cloud: email + API token from <code>id.atlassian.com → Security → API tokens</code>.
          {:else}
            Jira Data Center/Server: Personal Access Token from Jira → Profile → Personal Access Tokens.
          {/if}
        </p>
        <div class="inline-form-row">
          <input class="text-input" style="flex:1; min-width:90px" type="text" placeholder="mycompany.atlassian.net" bind:value={jiraDomain} />
          {#if isCloud}
            <input class="text-input" style="flex:1.5; min-width:120px" type="email" placeholder="you@example.com" bind:value={jiraEmail} />
          {/if}
        </div>
        <div class="inline-form-row">
          <div class="input-with-addon" style="flex:1">
            <input class="text-input" type={jiraShowToken ? 'text' : 'password'}
                   placeholder={isCloud ? 'API token' : 'Personal Access Token (PAT)'}
                   bind:value={jiraApiToken} />
            <button class="addon-btn" onclick={() => jiraShowToken = !jiraShowToken}>
              {#if jiraShowToken}<EyeOff size={12} />{:else}<Eye size={12} />{/if}
            </button>
          </div>
          <button class="btn-save jira-btn" onclick={saveJiraBasicAuth}
                  disabled={jiraBasicSaving || (isCloud && !jiraEmail.trim()) || !jiraApiToken.trim() || !jiraDomain.trim()}>
            {jiraBasicSaving ? 'Connecting…' : 'Connect'}
          </button>
          <button class="btn-cancel" onclick={() => { jiraMethod = null; jiraBasicError = ''; }}>Cancel</button>
        </div>
        {#if jiraBasicError}
          <div class="provider-error"><XCircle size={12} />{jiraBasicError}</div>
        {/if}
      </div>
    {/if}

    <!-- OAuth form -->
    {#if jiraMethod === 'oauth'}
      <OAuthBrowserAuthForm
        waiting={jiraOAuthWaiting}
        error={jiraOAuthError}
        brandColor="var(--brand-jira)"
        hintIdle="Opens Atlassian in the browser to authorize Arbor. Requires a configured Atlassian OAuth app."
        hintWaiting="Browser opened — approve access in Atlassian then return here."
        idleLabel="Authorize with Atlassian"
        busyLabel="Waiting for browser…"
        onAuthorize={startJiraOAuthFlow}
        onCancel={() => { jiraOAuthWaiting = false; jiraMethod = null; jiraOAuthError = ''; jiraOAuthUnsub?.(); }}
      />
    {/if}

    <button class="advanced-toggle" onclick={() => jiraAdvancedOpen = !jiraAdvancedOpen}>
      {#if jiraAdvancedOpen}<ChevronDown size={11} />{:else}<ChevronRight size={11} />{/if}
      <Settings2 size={11} />
      Advanced — use my own Atlassian OAuth app
    </button>
    {#if jiraAdvancedOpen}
      <OAuthAdvancedPanel provider="jira" />
    {/if}
  </div>
</div>

<!-- ── Display Preferences ── -->
<div class="card" style="margin-top:16px">
  <div class="card-section-title">Display Preferences</div>
  <div class="card-row-note">
    Default sort order applied to the Issues sidebar and Ticket Picker. Changes are saved immediately.
  </div>

  <FormRow label="Sort by" description="Field used to order issues">
    <Select
      value={issuesStore.sortField}
      options={Object.entries(SORT_FIELD_LABELS).map(([field, label]) => ({ value: field, label }))}
      onchange={(v) => issuesStore.setSort(v as IssueSortField, issuesStore.sortDir)}
    />
  </FormRow>

  <FormRow label="Direction" description="Ascending or descending order">
    <div class="sort-dir-toggle">
      <button
        class="dir-btn"
        class:dir-btn-active={issuesStore.sortDir === 'asc'}
        onclick={() => issuesStore.setSort(issuesStore.sortField, 'asc')}
        use:tooltip={'Ascending'}
      >
        <ArrowUp size={12} /> Ascending
      </button>
      <button
        class="dir-btn"
        class:dir-btn-active={issuesStore.sortDir === 'desc'}
        onclick={() => issuesStore.setSort(issuesStore.sortField, 'desc')}
        use:tooltip={'Descending'}
      >
        <ArrowDown size={12} /> Descending
      </button>
    </div>
  </FormRow>
</div>

<style>

  .provider-card {
    display: flex; align-items: flex-start; gap: 13px;
    padding: 13px 14px;
    background: var(--bg-elevated);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    transition: border-color var(--transition-fast);
  }
  .provider-card.flow-active { border-color: var(--accent); }
  /* Provider tiles use the shared BrandTile widget — no local logo styles. */

  .provider-main { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 10px; }
  .provider-top  { display: flex; align-items: center; gap: 10px; }
  .provider-info { flex: 1; display: flex; flex-direction: column; gap: 2px; }
  .provider-name { font-size: 13px; font-weight: 600; color: var(--text-primary); }
  .provider-desc { font-size: 11px; color: var(--text-muted); }

  .inline-form { display: flex; flex-direction: column; gap: 8px; }
  .inline-form-row { display: flex; align-items: center; gap: 7px; flex-wrap: wrap; }

  /* Brand-coloured CTA for the PAT/Basic-auth forms (the OAuth-redirect
     button lives inside <OAuthBrowserAuthForm>, which owns its own styles). */
  .btn-save {
    padding: 5px 14px; border: none; border-radius: var(--radius-sm);
    font-family: var(--font-ui-sans); font-size: 12px; font-weight: 500;
    cursor: pointer; transition: filter var(--transition-fast); white-space: nowrap;
    display: flex; align-items: center; gap: 5px;
  }
  .btn-save:disabled { opacity: 0.45; cursor: not-allowed; }

  /* Brand backgrounds — hard-coded #fff foreground (the bg is an absolute
     brand colour; `--text-on-accent` would resolve to dark in light themes). */
  .linear-btn { background: var(--brand-linear); color: #fff; }
  .linear-btn:hover:not(:disabled) { filter: brightness(1.12); }
  .jira-btn   { background: var(--brand-jira);   color: #fff; }
  .jira-btn:hover:not(:disabled)   { filter: brightness(1.12); }

  .btn-cancel {
    padding: 5px 10px; background: transparent;
    border: 1px solid var(--border); border-radius: var(--radius-sm);
    font-family: var(--font-ui-sans); font-size: 11px; color: var(--text-muted);
    cursor: pointer; transition: all var(--transition-fast); white-space: nowrap;
  }
  .btn-cancel:hover { background: var(--bg-hover); color: var(--text-primary); }

  .form-hint { font-size: 10.5px; color: var(--text-muted); margin: 0; line-height: 1.5; }
  .form-hint code {
    font-family: var(--font-code); font-size: 10px; color: var(--accent);
    background: var(--bg-overlay); border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm); padding: 0 3px;
  }

  .advanced-toggle {
    display: inline-flex; align-items: center; gap: 5px;
    align-self: flex-start;
    padding: 4px 8px;
    background: transparent; color: var(--text-muted);
    border: 1px dashed var(--border); border-radius: var(--radius-sm);
    font-family: var(--font-ui-sans); font-size: 11px;
    cursor: pointer; transition: all var(--transition-fast);
  }
  .advanced-toggle:hover { color: var(--text-primary); border-color: var(--accent); background: var(--bg-hover); }

  .provider-error {
    display: flex; align-items: center; gap: 6px;
    font-size: 11px; color: var(--error, #f87171);
  }

  .text-input {
    padding: 5px 8px; background: var(--bg-input); color: var(--text-primary);
    border: 1px solid var(--border); border-radius: var(--radius-sm);
    font-family: var(--font-ui-sans); font-size: 12px;
    outline: none; transition: border-color var(--transition-fast);
  }
  .text-input:focus { border-color: var(--accent); }

  .input-with-addon { display: flex; }
  .input-with-addon .text-input { border-radius: var(--radius-sm) 0 0 var(--radius-sm); flex: 1; }
  .addon-btn {
    display: flex; align-items: center; justify-content: center;
    width: 28px; background: var(--bg-input); border: 1px solid var(--border);
    border-left: none; border-radius: 0 var(--radius-sm) var(--radius-sm) 0;
    cursor: pointer; color: var(--text-muted); transition: color var(--transition-fast);
  }
  .addon-btn:hover { color: var(--text-primary); }

  /* Sort direction toggle */
  .sort-dir-toggle { display: flex; gap: 4px; }
  .dir-btn {
    display: flex; align-items: center; gap: 5px;
    padding: 4px 10px;
    font-size: 11px;
    font-family: var(--font-ui-sans);
    background: var(--bg-elevated);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    color: var(--text-secondary);
    cursor: pointer;
    transition: background var(--transition-fast), border-color var(--transition-fast), color var(--transition-fast);
  }
  .dir-btn:hover { background: var(--bg-hover); color: var(--text-primary); }
  .dir-btn-active {
    background: var(--accent-subtle);
    border-color: var(--accent);
    color: var(--accent);
  }
</style>
