<script lang="ts">
  import {
    CheckCircle2, Loader2, XCircle, Copy,
    ChevronDown, ChevronRight, Eye, EyeOff, ExternalLink, Settings2,
  } from 'lucide-svelte';
  import BrandTile from '$lib/components/shared/internal/BrandTile.svelte';
  import ProviderUserBadge from '$lib/components/shared/internal/ProviderUserBadge.svelte';
  import OAuthAdvancedPanel from '$lib/components/shared/OAuthAdvancedPanel.svelte';
  import { openUrl } from '@tauri-apps/plugin-opener';
  import { listen } from '@tauri-apps/api/event';
  import {
    startGithubDeviceFlow, getGithubStatus, getGithubUser, disconnectGithub,
    startGitlabOAuth, getGitlabStatus, getGitlabUser, disconnectGitlab,
    saveCredential, saveDefaultCredential,
    type DeviceFlowInfo, type ProviderUser,
  } from '$lib/ipc/auth';
  import { uiStore } from '$lib/stores/ui.svelte';
  import { copyToClipboard } from '$lib/utils/clipboard';
  import SectionHeader from '$lib/components/shared/ui/SectionHeader.svelte';
  import { tooltip } from '$lib/actions/tooltip';

  type ConnState  = 'checking' | 'disconnected' | 'connecting' | 'connected';
  type AuthMethod = 'oauth' | 'pat' | 'userpass';

  // ── GitHub state ──────────────────────────────────────────────────────────
  let ghState        = $state<ConnState>('checking');
  let ghError        = $state('');
  let ghMethod       = $state<AuthMethod | null>(null);
  let ghDropOpen     = $state(false);
  let ghPatInput     = $state('');
  let ghShowPass     = $state(false);
  let ghUser         = $state('');
  let ghPass         = $state('');
  let ghSaving       = $state(false);
  let ghOAuthWaiting = $state(false);
  let ghDeviceInfo   = $state<DeviceFlowInfo | null>(null);
  let ghAdvancedOpen = $state(false);
  let ghOAuthUnsub: (() => void) | null = null;
  let ghIdentity     = $state<ProviderUser | null>(null);

  // ── GitLab state ──────────────────────────────────────────────────────────
  let glState        = $state<ConnState>('checking');
  let glError        = $state('');
  let glMethod       = $state<AuthMethod | null>(null);
  let glDropOpen     = $state(false);
  let glPatInput     = $state('');
  let glShowPass     = $state(false);
  let glUser         = $state('');
  let glPass         = $state('');
  let glSaving       = $state(false);
  let glHost         = $state('gitlab.com');
  let glCustom       = $state(false);
  let glOAuthWaiting = $state(false);
  let glAdvancedOpen = $state(false);
  let glOAuthUnsub: (() => void) | null = null;
  let glIdentity     = $state<ProviderUser | null>(null);

  $effect(() => {
    getGithubStatus()
      .then(ok => { ghState = ok ? 'connected' : 'disconnected'; })
      .catch(() => { ghState = 'disconnected'; });
    getGitlabStatus()
      .then(ok => { glState = ok ? 'connected' : 'disconnected'; })
      .catch(() => { glState = 'disconnected'; });
  });

  // Fetch identity whenever the state flips to connected (or back), so the
  // badge appears after a fresh OAuth flow without a settings reopen.
  $effect(() => {
    if (ghState === 'connected') {
      if (ghIdentity === null) getGithubUser().then(u => { ghIdentity = u; }).catch(() => {});
    } else {
      ghIdentity = null;
    }
  });
  $effect(() => {
    if (glState === 'connected') {
      if (glIdentity === null) getGitlabUser().then(u => { glIdentity = u; }).catch(() => {});
    } else {
      glIdentity = null;
    }
  });

  function closeDropdowns(e: MouseEvent) {
    if (!(e.target as HTMLElement).closest('.split-btn-wrap')) {
      ghDropOpen = glDropOpen = false;
    }
  }

  // ── GitHub ────────────────────────────────────────────────────────────────
  function pickGhMethod(m: AuthMethod) {
    ghMethod = m; ghDropOpen = false; ghError = '';
    if (m === 'oauth') startGithubOAuthFlow();
  }

  async function startGithubOAuthFlow() {
    ghOAuthWaiting = true; ghError = ''; ghDeviceInfo = null;
    ghState = 'connecting';

    ghOAuthUnsub?.();
    ghOAuthUnsub = await listen<string | null>('arbor://github-oauth-done', ({ payload }) => {
      ghOAuthUnsub?.(); ghOAuthUnsub = null;
      ghOAuthWaiting = false; ghDeviceInfo = null;
      if (payload === null) {
        ghState = 'connected'; ghMethod = null;
        uiStore.showToast('GitHub connected via OAuth', 'success');
      } else {
        ghState = 'disconnected';
        ghError = payload;
      }
    });

    try {
      const info = await startGithubDeviceFlow();
      ghDeviceInfo = info;
      try { await openUrl(info.verification_uri); } catch { /* user can copy */ }
    } catch (err) {
      ghOAuthWaiting = false; ghState = 'disconnected';
      ghError = String(err);
      ghOAuthUnsub?.(); ghOAuthUnsub = null;
    }
  }

  function copyGhDeviceCode() {
    if (!ghDeviceInfo) return;
    void copyToClipboard(ghDeviceInfo.user_code, { successToast: 'Code copied to clipboard' });
  }

  function openGhVerification() {
    if (ghDeviceInfo) openUrl(ghDeviceInfo.verification_uri).catch(() => {});
  }

  function cancelGithubOAuth() {
    ghOAuthUnsub?.(); ghOAuthUnsub = null;
    ghOAuthWaiting = false; ghDeviceInfo = null;
    ghState = 'disconnected'; ghMethod = null; ghError = '';
  }

  async function saveGithubPat() {
    if (!ghPatInput.trim()) return;
    ghSaving = true; ghError = '';
    try {
      await saveCredential('github.com', 'oauth', ghPatInput.trim());
      await saveDefaultCredential('github.com', 'oauth', ghPatInput.trim());
      ghState = 'connected'; ghPatInput = ''; ghMethod = null;
      uiStore.showToast('GitHub token saved', 'success');
    } catch (err) { ghError = String(err); } finally { ghSaving = false; }
  }

  async function saveGithubUserPass() {
    if (!ghUser.trim() || !ghPass.trim()) return;
    ghSaving = true; ghError = '';
    try {
      await saveCredential('github.com', ghUser.trim(), ghPass.trim());
      await saveDefaultCredential('github.com', ghUser.trim(), ghPass.trim());
      ghState = 'connected'; ghUser = ''; ghPass = ''; ghMethod = null;
      uiStore.showToast('GitHub credentials saved', 'success');
    } catch (err) { ghError = String(err); } finally { ghSaving = false; }
  }

  async function disconnectGithubAcc() {
    ghOAuthUnsub?.(); ghOAuthUnsub = null;
    ghOAuthWaiting = false; ghDeviceInfo = null;
    await disconnectGithub().catch(() => {});
    ghState = 'disconnected'; ghMethod = null; ghError = '';
    uiStore.showToast('GitHub disconnected', 'info');
  }

  // ── GitLab ────────────────────────────────────────────────────────────────
  function pickGlMethod(m: AuthMethod) {
    glMethod = m; glDropOpen = false; glError = '';
    if (m === 'oauth') startGitlabOAuthFlow();
  }

  async function startGitlabOAuthFlow() {
    glOAuthWaiting = true; glError = '';
    glState = 'connecting';

    glOAuthUnsub?.();
    glOAuthUnsub = await listen<string | null>('arbor://gitlab-oauth-done', ({ payload }) => {
      glOAuthUnsub?.(); glOAuthUnsub = null;
      glOAuthWaiting = false;
      if (payload === null) {
        glState = 'connected'; glMethod = null;
        uiStore.showToast('GitLab connected via OAuth', 'success');
      } else {
        glState = 'disconnected';
        glError = payload;
      }
    });

    try {
      const url = await startGitlabOAuth();
      try { await openUrl(url); } catch { /* user can copy */ }
    } catch (err) {
      glOAuthWaiting = false; glState = 'disconnected';
      glError = String(err);
      glOAuthUnsub?.(); glOAuthUnsub = null;
    }
  }

  async function saveGitlabPat() {
    if (!glPatInput.trim()) return;
    const host = glCustom ? glHost.trim() : 'gitlab.com';
    glSaving = true; glError = '';
    try {
      await saveCredential(host, 'oauth', glPatInput.trim());
      await saveDefaultCredential(host, 'oauth', glPatInput.trim());
      glState = 'connected'; glPatInput = ''; glMethod = null;
      uiStore.showToast('GitLab token saved', 'success');
    } catch (err) { glError = String(err); } finally { glSaving = false; }
  }

  async function saveGitlabUserPass() {
    if (!glUser.trim() || !glPass.trim()) return;
    const host = glCustom ? glHost.trim() : 'gitlab.com';
    glSaving = true; glError = '';
    try {
      await saveCredential(host, glUser.trim(), glPass.trim());
      await saveDefaultCredential(host, glUser.trim(), glPass.trim());
      glState = 'connected'; glUser = ''; glPass = ''; glMethod = null;
      uiStore.showToast('GitLab credentials saved', 'success');
    } catch (err) { glError = String(err); } finally { glSaving = false; }
  }

  async function disconnectGitlabAcc() {
    glOAuthUnsub?.(); glOAuthUnsub = null; glOAuthWaiting = false;
    await disconnectGitlab().catch(() => {});
    glState = 'disconnected'; glMethod = null; glError = '';
    uiStore.showToast('GitLab disconnected', 'info');
  }

</script>

<div class="backdrop-listener" role="presentation" onclick={closeDropdowns}></div>

<SectionHeader title="Git" description="Connect to Git hosting providers. Credentials are stored in the OS keychain." />

<!-- GitHub -->
<div class="provider-card">
  <BrandTile brand="github" />
  <div class="provider-main">
    <div class="provider-top">
      <div class="provider-info">
        <span class="provider-name">GitHub</span>
        <span class="provider-desc">github.com — OAuth &amp; credentials</span>
      </div>
      <div class="provider-action">
        {#if ghState === 'checking'}
          <span class="status-checking"><Loader2 size={12} class="spin" /> Checking…</span>
        {:else if ghState === 'connected'}
          <span class="status-ok"><CheckCircle2 size={12} /> Connected</span>
          <button class="btn-ghost-danger" onclick={disconnectGithubAcc}>Disconnect</button>
        {:else if ghState === 'connecting'}
          <span class="status-wait"><Loader2 size={12} class="spin" /> Waiting for authorisation…</span>
          <button class="btn-ghost" onclick={cancelGithubOAuth}>Cancel</button>
        {:else if ghMethod === null}
          <div class="split-btn-wrap">
            <button class="split-main github-btn" onclick={() => pickGhMethod('oauth')}>Connect</button>
            <button class="split-chev github-btn" onclick={(e) => { e.stopPropagation(); ghDropOpen = !ghDropOpen; }}>
              <ChevronDown size={11} />
            </button>
            {#if ghDropOpen}
              <div class="split-dropdown">
                <button onclick={() => pickGhMethod('oauth')}><span class="drop-icon">🔐</span> OAuth (browser)</button>
                <button onclick={() => pickGhMethod('pat')}><span class="drop-icon">🔑</span> Personal Access Token</button>
                <button onclick={() => pickGhMethod('userpass')}><span class="drop-icon">👤</span> Username + Password</button>
              </div>
            {/if}
          </div>
        {/if}
      </div>
    </div>

    {#if ghState === 'connected' && ghIdentity}
      <ProviderUserBadge
        avatarUrl={ghIdentity.avatar_url}
        name={ghIdentity.name ?? ghIdentity.login}
        secondary={ghIdentity.email ?? `@${ghIdentity.login}`}
      />
    {/if}

    {#if ghMethod === 'oauth'}
      <div class="inline-form">
        {#if ghDeviceInfo}
          <p class="form-hint">Open the verification page on GitHub and enter this code:</p>
          <div class="device-code-row">
            <code class="device-code">{ghDeviceInfo.user_code}</code>
            <button class="device-copy" use:tooltip={'Copy code'} onclick={copyGhDeviceCode}><Copy size={12} /></button>
            <button class="device-open" onclick={openGhVerification}>
              <ExternalLink size={11} /> Open {ghDeviceInfo.verification_uri.replace(/^https?:\/\//, '')}
            </button>
          </div>
          <p class="form-hint">Arbor will detect the authorisation automatically.</p>
        {:else}
          <p class="form-hint">Opens a GitHub verification page where you confirm a one-time code.</p>
          <div class="inline-form-row">
            <button class="btn-save github-btn" onclick={startGithubOAuthFlow} disabled={ghOAuthWaiting}>
              {#if ghOAuthWaiting}<Loader2 size={11} class="spin" />{/if}
              {ghOAuthWaiting ? 'Requesting code…' : 'Authorize with GitHub'}
            </button>
            <button class="btn-cancel" onclick={cancelGithubOAuth}>Cancel</button>
          </div>
        {/if}
      </div>
    {/if}

    {#if ghMethod === 'pat'}
      <div class="inline-form">
        <div class="inline-form-row">
          <div class="input-with-addon">
            <input class="text-input" type={ghShowPass ? 'text' : 'password'} placeholder="ghp_…" bind:value={ghPatInput} />
            <button class="addon-btn" onclick={() => ghShowPass = !ghShowPass}>{#if ghShowPass}<EyeOff size={12} />{:else}<Eye size={12} />{/if}</button>
          </div>
          <button class="btn-save github-btn" onclick={saveGithubPat} disabled={ghSaving || !ghPatInput.trim()}>
            {ghSaving ? 'Saving…' : 'Save'}
          </button>
          <button class="btn-cancel" onclick={() => { ghMethod = null; ghError = ''; }}>Cancel</button>
        </div>
        <p class="form-hint">Generate at <code>github.com → Settings → Developer settings → Personal access tokens</code></p>
      </div>
    {/if}

    {#if ghMethod === 'userpass'}
      <div class="inline-form">
        <div class="inline-form-row">
          <input class="text-input" type="text" placeholder="Username" bind:value={ghUser} />
          <div class="input-with-addon" style="flex:1">
            <input class="text-input" type={ghShowPass ? 'text' : 'password'} placeholder="Password / Token" bind:value={ghPass} />
            <button class="addon-btn" onclick={() => ghShowPass = !ghShowPass}>{#if ghShowPass}<EyeOff size={12} />{:else}<Eye size={12} />{/if}</button>
          </div>
          <button class="btn-save github-btn" onclick={saveGithubUserPass} disabled={ghSaving || !ghUser.trim() || !ghPass.trim()}>
            {ghSaving ? 'Saving…' : 'Save'}
          </button>
          <button class="btn-cancel" onclick={() => { ghMethod = null; ghError = ''; }}>Cancel</button>
        </div>
      </div>
    {/if}

    {#if ghError}<div class="provider-error"><XCircle size={12} />{ghError}</div>{/if}

    <button class="advanced-toggle" onclick={() => ghAdvancedOpen = !ghAdvancedOpen}>
      {#if ghAdvancedOpen}<ChevronDown size={11} />{:else}<ChevronRight size={11} />{/if}
      <Settings2 size={11} />
      Advanced — use my own OAuth app
    </button>
    {#if ghAdvancedOpen}
      <OAuthAdvancedPanel provider="github" />
    {/if}
  </div>
</div>

<!-- GitLab -->
<div class="provider-card">
  <BrandTile brand="gitlab" />
  <div class="provider-main">
    <div class="provider-top">
      <div class="provider-info">
        <span class="provider-name">GitLab</span>
        <span class="provider-desc">gitlab.com or self-hosted — OAuth &amp; credentials</span>
      </div>
      <div class="provider-action">
        {#if glState === 'checking'}
          <span class="status-checking"><Loader2 size={12} class="spin" /> Checking…</span>
        {:else if glState === 'connected'}
          <span class="status-ok"><CheckCircle2 size={12} /> Connected</span>
          <button class="btn-ghost-danger" onclick={disconnectGitlabAcc}>Disconnect</button>
        {:else if glState === 'connecting'}
          <span class="status-wait"><Loader2 size={12} class="spin" /> Waiting for browser…</span>
          <button class="btn-ghost" onclick={() => { glOAuthUnsub?.(); glOAuthWaiting=false; glState='disconnected'; glMethod=null; }}>Cancel</button>
        {:else if glMethod === null}
          <div class="split-btn-wrap">
            <button class="split-main gitlab-btn" onclick={() => pickGlMethod('oauth')}>Connect</button>
            <button class="split-chev gitlab-btn" onclick={(e) => { e.stopPropagation(); glDropOpen = !glDropOpen; }}>
              <ChevronDown size={11} />
            </button>
            {#if glDropOpen}
              <div class="split-dropdown">
                <button onclick={() => pickGlMethod('oauth')}><span class="drop-icon">🔐</span> OAuth (browser)</button>
                <button onclick={() => pickGlMethod('pat')}><span class="drop-icon">🔑</span> Personal Access Token</button>
                <button onclick={() => pickGlMethod('userpass')}><span class="drop-icon">👤</span> Username + Password</button>
              </div>
            {/if}
          </div>
        {/if}
      </div>
    </div>

    {#if glState === 'connected' && glIdentity}
      <ProviderUserBadge
        avatarUrl={glIdentity.avatar_url}
        name={glIdentity.name ?? glIdentity.login}
        secondary={glIdentity.email ?? `@${glIdentity.login}`}
      />
    {/if}

    {#if glMethod === 'oauth'}
      <div class="inline-form">
        {#if glOAuthWaiting}
          <p class="form-hint">Browser opened — approve access on GitLab then return here.</p>
        {:else}
          <p class="form-hint">Opens gitlab.com in the browser to authorize Arbor. For self-hosted instances, use PAT or Username + Password.</p>
        {/if}
        <div class="inline-form-row">
          <button class="btn-save gitlab-btn" onclick={startGitlabOAuthFlow} disabled={glOAuthWaiting}>
            {#if glOAuthWaiting}<Loader2 size={11} class="spin" />{/if}
            {glOAuthWaiting ? 'Waiting for browser…' : 'Authorize with GitLab'}
          </button>
          <button class="btn-cancel" onclick={() => { glOAuthUnsub?.(); glOAuthWaiting=false; glMethod=null; glError=''; }}>Cancel</button>
        </div>
      </div>
    {/if}

    {#if glMethod === 'pat'}
      <div class="inline-form">
        <div class="inline-form-row">
          <label class="inline-check">
            <input type="checkbox" bind:checked={glCustom} />
            <span>Self-hosted</span>
          </label>
          {#if glCustom}
            <input class="text-input" type="text" placeholder="gitlab.company.com" bind:value={glHost} />
          {/if}
        </div>
        <div class="inline-form-row">
          <div class="input-with-addon">
            <input class="text-input" type={glShowPass ? 'text' : 'password'} placeholder="glpat-…" bind:value={glPatInput} />
            <button class="addon-btn" onclick={() => glShowPass = !glShowPass}>{#if glShowPass}<EyeOff size={12} />{:else}<Eye size={12} />{/if}</button>
          </div>
          <button class="btn-save gitlab-btn" onclick={saveGitlabPat} disabled={glSaving || !glPatInput.trim()}>
            {glSaving ? 'Saving…' : 'Save'}
          </button>
          <button class="btn-cancel" onclick={() => { glMethod = null; glError = ''; }}>Cancel</button>
        </div>
      </div>
    {/if}

    {#if glMethod === 'userpass'}
      <div class="inline-form">
        <div class="inline-form-row">
          <label class="inline-check">
            <input type="checkbox" bind:checked={glCustom} />
            <span>Self-hosted</span>
          </label>
          {#if glCustom}
            <input class="text-input" type="text" placeholder="gitlab.company.com" bind:value={glHost} />
          {/if}
        </div>
        <div class="inline-form-row">
          <input class="text-input" type="text" placeholder="Username" bind:value={glUser} />
          <div class="input-with-addon" style="flex:1">
            <input class="text-input" type={glShowPass ? 'text' : 'password'} placeholder="Password / Token" bind:value={glPass} />
            <button class="addon-btn" onclick={() => glShowPass = !glShowPass}>{#if glShowPass}<EyeOff size={12} />{:else}<Eye size={12} />{/if}</button>
          </div>
          <button class="btn-save gitlab-btn" onclick={saveGitlabUserPass} disabled={glSaving || !glUser.trim() || !glPass.trim()}>
            {glSaving ? 'Saving…' : 'Save'}
          </button>
          <button class="btn-cancel" onclick={() => { glMethod = null; glError = ''; }}>Cancel</button>
        </div>
      </div>
    {/if}

    {#if glError}<div class="provider-error"><XCircle size={12} />{glError}</div>{/if}

    <button class="advanced-toggle" onclick={() => glAdvancedOpen = !glAdvancedOpen}>
      {#if glAdvancedOpen}<ChevronDown size={11} />{:else}<ChevronRight size={11} />{/if}
      <Settings2 size={11} />
      Advanced — use my own OAuth app or self-hosted host
    </button>
    {#if glAdvancedOpen}
      <OAuthAdvancedPanel provider="gitlab" />
    {/if}
  </div>
</div>

<!-- Bitbucket (coming soon) -->
<div class="provider-card provider-disabled">
  <BrandTile brand="bitbucket" />
  <div class="provider-main">
    <div class="provider-top">
      <div class="provider-info">
        <span class="provider-name">Bitbucket</span>
        <span class="provider-desc">Atlassian Git hosting</span>
      </div>
      <span class="badge-soon">Coming soon</span>
    </div>
  </div>
</div>

<style>
  .backdrop-listener { position: fixed; inset: 0; z-index: 0; pointer-events: none; }

  .provider-card {
    display: flex; align-items: flex-start; gap: 13px;
    padding: 13px 14px;
    background: var(--bg-elevated);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    transition: border-color var(--transition-fast);
  }
  .provider-disabled { opacity: 0.45; pointer-events: none; }
  /* Provider tiles use the shared BrandTile widget — no local logo styles. */

  .provider-main   { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 10px; }
  .provider-top    { display: flex; align-items: center; gap: 10px; }
  .provider-info   { flex: 1; display: flex; flex-direction: column; gap: 2px; }
  .provider-name   { font-size: 13px; font-weight: 600; color: var(--text-primary); }
  .provider-desc   { font-size: 11px; color: var(--text-muted); }
  .provider-action { flex-shrink: 0; display: flex; align-items: center; gap: 8px; }

  .status-checking, .status-wait {
    display: flex; align-items: center; gap: 5px;
    font-size: 11px; color: var(--text-muted);
  }
  .status-ok {
    display: flex; align-items: center; gap: 5px;
    font-size: 12px; font-weight: 500; color: var(--success, #6aab73);
  }

  .split-btn-wrap { position: relative; display: flex; z-index: 10; }
  .split-main {
    padding: 5px 12px; border: none;
    border-radius: var(--radius-sm) 0 0 var(--radius-sm);
    font-family: var(--font-ui-sans); font-size: 12px; font-weight: 500;
    cursor: pointer; transition: filter var(--transition-fast); white-space: nowrap;
  }
  .split-chev {
    display: flex; align-items: center; justify-content: center;
    width: 26px; border: none; border-left: 1px solid rgba(255,255,255,0.2);
    border-radius: 0 var(--radius-sm) var(--radius-sm) 0;
    cursor: pointer; transition: filter var(--transition-fast);
  }
  /* Brand-coloured buttons: hard-coded #fff foreground (the bg is an absolute
     brand colour, so the label must not borrow `--text-on-accent`, which can
     resolve to dark in light themes). */
  .github-btn { background: var(--brand-github-green); color: #fff; }
  .github-btn:hover { filter: brightness(1.15); }
  .gitlab-btn { background: var(--brand-gitlab); color: #fff; }
  .gitlab-btn:hover { filter: brightness(1.1); }

  .split-dropdown {
    position: absolute; top: calc(100% + 4px); right: 0; min-width: 210px;
    background: var(--bg-overlay); border: 1px solid var(--border);
    border-radius: var(--radius-md); box-shadow: 0 8px 24px rgba(0,0,0,0.4);
    padding: 4px; z-index: var(--z-dropdown); animation: dropIn var(--anim-dur-fast) ease-out;
  }
  @keyframes dropIn { from { opacity:0; transform:translateY(-4px); } to { opacity:1; transform:none; } }
  .split-dropdown button {
    display: flex; align-items: center; gap: 8px;
    width: 100%; padding: 7px 10px; text-align: left;
    font-size: 12px; color: var(--text-primary);
    background: transparent; border: none; border-radius: var(--radius-sm);
    cursor: pointer; font-family: var(--font-ui-sans);
    transition: background var(--transition-fast);
  }
  .split-dropdown button:hover { background: var(--bg-hover); }
  .drop-icon { font-size: 13px; }

  .inline-form { display: flex; flex-direction: column; gap: 8px; }
  .inline-form-row { display: flex; align-items: center; gap: 7px; flex-wrap: wrap; }

  .btn-save {
    padding: 5px 14px; border: none; border-radius: var(--radius-sm);
    font-family: var(--font-ui-sans); font-size: 12px; font-weight: 500;
    cursor: pointer; transition: filter var(--transition-fast); white-space: nowrap;
    display: flex; align-items: center; gap: 5px;
  }
  .btn-save:disabled { opacity: 0.45; cursor: not-allowed; }
  .btn-cancel {
    padding: 5px 10px; background: transparent;
    border: 1px solid var(--border); border-radius: var(--radius-sm);
    font-family: var(--font-ui-sans); font-size: 11px; color: var(--text-muted);
    cursor: pointer; transition: all var(--transition-fast); white-space: nowrap;
  }
  .btn-cancel:hover { background: var(--bg-hover); color: var(--text-primary); }
  .btn-ghost {
    padding: 4px 10px; background: transparent;
    border: 1px solid var(--border); border-radius: var(--radius-sm);
    font-family: var(--font-ui-sans); font-size: 11px; color: var(--text-muted);
    cursor: pointer; transition: all var(--transition-fast); white-space: nowrap;
  }
  .btn-ghost:hover { background: var(--bg-hover); color: var(--text-primary); }
  .btn-ghost-danger {
    padding: 4px 10px; background: transparent;
    border: 1px solid var(--border); border-radius: var(--radius-sm);
    font-family: var(--font-ui-sans); font-size: 11px; color: var(--error, #f87171);
    border-color: var(--error, #f87171); cursor: pointer; transition: all var(--transition-fast);
  }
  .btn-ghost-danger:hover { background: color-mix(in srgb, var(--error, #f87171) 12%, transparent); }

  .inline-check {
    display: flex; align-items: center; gap: 5px;
    font-size: 11px; color: var(--text-muted); cursor: pointer; white-space: nowrap;
  }
  .inline-check input { accent-color: var(--accent); cursor: pointer; }

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

  .device-code-row { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }
  .device-code {
    font-family: var(--font-code);
    font-size: 18px;
    font-weight: 700;
    letter-spacing: 0.18em;
    padding: 6px 12px;
    background: var(--bg-base);
    color: var(--accent);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    user-select: all;
  }
  .device-copy {
    display: flex; align-items: center; justify-content: center;
    width: 28px; height: 28px;
    background: transparent; color: var(--text-muted);
    border: 1px solid var(--border); border-radius: var(--radius-sm);
    cursor: pointer; transition: all var(--transition-fast);
  }
  .device-copy:hover { color: var(--text-primary); background: var(--bg-hover); }
  .device-open {
    display: flex; align-items: center; gap: 5px;
    padding: 5px 10px;
    background: transparent; color: var(--text-secondary);
    border: 1px solid var(--border); border-radius: var(--radius-sm);
    font-family: var(--font-ui-sans); font-size: 11px;
    cursor: pointer; transition: all var(--transition-fast); white-space: nowrap;
  }
  .device-open:hover { color: var(--text-primary); background: var(--bg-hover); }

  .form-hint { font-size: 10.5px; color: var(--text-muted); margin: 0; line-height: 1.5; }
  .form-hint code {
    font-family: var(--font-code); font-size: 10px; color: var(--accent);
    background: var(--bg-overlay); border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm); padding: 0 3px;
  }

  .provider-error {
    display: flex; align-items: center; gap: 6px;
    font-size: 11px; color: var(--error, #f87171);
  }

  .badge-soon {
    font-size: 10px; font-weight: 600; padding: 2px 7px;
    background: var(--bg-overlay); border: 1px solid var(--border-subtle);
    border-radius: 99px; color: var(--text-muted);
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

  :global(.spin) { animation: spin-anim 1.2s linear infinite; }
  @keyframes spin-anim { to { transform: rotate(360deg); } }
</style>
