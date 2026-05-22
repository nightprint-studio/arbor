<script lang="ts">
  import {
    CheckCircle2, Loader2, XCircle, Copy,
    ChevronDown, Eye, EyeOff, ExternalLink,
  } from 'lucide-svelte';
  import BrandTile from '$lib/components/shared/internal/BrandTile.svelte';
  import { openUrl } from '@tauri-apps/plugin-opener';
  import { listen } from '@tauri-apps/api/event';
  import {
    startGithubDeviceFlow, getGithubStatus, disconnectGithub,
    startGitlabOAuth, getGitlabStatus, disconnectGitlab,
    startLinearOAuth, disconnectLinearOAuth,
    saveCredential, saveDefaultCredential,
    type DeviceFlowInfo,
  } from '$lib/ipc/auth';
  import { issuesStore } from '$lib/stores/issues.svelte';
  import { uiStore } from '$lib/stores/ui.svelte';
  import SectionHeader from '$lib/components/shared/ui/SectionHeader.svelte';
  import { tooltip } from '$lib/actions/tooltip';

  // ── Types ──────────────────────────────────────────────────────────────────
  type ConnState   = 'checking' | 'disconnected' | 'connecting' | 'connected';
  type AuthMethod  = 'oauth' | 'pat' | 'userpass';

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
  let ghOAuthUnsub: (() => void) | null = null;

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
  let glOAuthUnsub: (() => void) | null = null;

  // ── Linear state ──────────────────────────────────────────────────────────
  let linState        = $state<ConnState>('checking');
  let linMethod       = $state<AuthMethod | null>(null);
  let linDropOpen     = $state(false);
  let linPatInput     = $state('');
  let linShowPat      = $state(false);
  let linPatSaving    = $state(false);
  let linPatError     = $state('');
  let linOAuthWaiting = $state(false);
  let linOAuthError   = $state('');
  let linOAuthUnsub: (() => void) | null = null;

  // ── Init ──────────────────────────────────────────────────────────────────
  $effect(() => {
    checkAllStatuses();
    if (issuesStore.authStatus === null) issuesStore.loadAuthStatus();
  });

  async function checkAllStatuses() {
    await Promise.allSettled([
      getGithubStatus()
        .then(ok => { ghState = ok ? 'connected' : 'disconnected'; })
        .catch(() => { ghState = 'disconnected'; }),
      getGitlabStatus()
        .then(ok => { glState = ok ? 'connected' : 'disconnected'; })
        .catch(() => { glState = 'disconnected'; }),
    ]);
    if (issuesStore.authStatus?.authenticated) linState = 'connected';
    else if (issuesStore.authStatus !== null) linState = 'disconnected';
  }

  $effect(() => {
    if (issuesStore.authStatus?.authenticated) linState = 'connected';
    else if (issuesStore.authStatus !== null && linState === 'checking') linState = 'disconnected';
  });

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
    navigator.clipboard.writeText(ghDeviceInfo.user_code).catch(() => {});
    uiStore.showToast('Code copied to clipboard', 'success');
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

  // ── Linear ────────────────────────────────────────────────────────────────
  function pickLinMethod(m: AuthMethod) {
    linMethod = m; linDropOpen = false;
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

  function copyToClipboard(text: string) {
    navigator.clipboard.writeText(text).catch(() => {});
    uiStore.showToast('Copied to clipboard', 'success');
  }
</script>

<SectionHeader title="Access" description="Connect to Git hosts and issue trackers. Credentials are stored in the OS keychain." />

<!-- ═══════════════════════════════ GIT ══════════════════════════════════ -->
<div class="record-card">
  <div class="record-title">
    <span>Git</span>
    <span class="record-subtitle">Version control authentication</span>
  </div>

  <!-- GitHub -->
  <div class="record-row">
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

      {#if ghError}
        <div class="provider-error"><XCircle size={12} />{ghError}</div>
      {/if}
    </div>
  </div>

  <!-- GitLab -->
  <div class="record-row">
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

      {#if glError}
        <div class="provider-error"><XCircle size={12} />{glError}</div>
      {/if}
    </div>
  </div>

  <!-- Bitbucket (coming soon) -->
  <div class="record-row provider-disabled">
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
</div>

<!-- ═══════════════════════════════ ISSUE TRACKERS ════════════════════════ -->
<div class="record-card">
  <div class="record-title">
    <span>Issue Trackers</span>
    <span class="record-subtitle">Project management integrations</span>
  </div>

  <!-- Linear -->
  <div class="record-row" class:flow-active={linState === 'connecting'}>
    <BrandTile brand="linear" />
    <div class="provider-main">
      <div class="provider-top">
        <div class="provider-info">
          <span class="provider-name">Linear</span>
          <span class="provider-desc">Issue tracker — OAuth &amp; Personal API Key</span>
        </div>
        <div class="provider-action">
          {#if linState === 'checking' || issuesStore.authStatus === null}
            <span class="status-checking"><Loader2 size={12} class="spin" /> Checking…</span>
          {:else if linState === 'connected' && issuesStore.authStatus?.authenticated}
            <span class="status-ok"><CheckCircle2 size={12} /> Connected</span>
            <button class="btn-ghost-danger" onclick={disconnectLinear}>Disconnect</button>
          {:else if linState === 'connecting'}
            <span class="status-wait"><Loader2 size={12} class="spin" /> Waiting…</span>
            <button class="btn-ghost" onclick={() => { linOAuthWaiting = false; linState = 'disconnected'; linMethod = null; linOAuthUnsub?.(); }}>Cancel</button>
          {:else if linMethod === null}
            <div class="split-btn-wrap">
              <button class="split-main linear-btn" onclick={() => pickLinMethod('oauth')}>Connect</button>
              <button class="split-chev linear-btn" onclick={(e) => { e.stopPropagation(); linDropOpen = !linDropOpen; }}>
                <ChevronDown size={11} />
              </button>
              {#if linDropOpen}
                <div class="split-dropdown">
                  <button onclick={() => pickLinMethod('oauth')}><span class="drop-icon">🔐</span> OAuth (recommended)</button>
                  <button onclick={() => pickLinMethod('pat')}><span class="drop-icon">🔑</span> Personal API Key</button>
                </div>
              {/if}
            </div>
          {/if}
        </div>
      </div>

      {#if linState === 'connected' && issuesStore.authStatus?.user}
        {@const u = issuesStore.authStatus.user}
        <div class="connected-user">
          {#if u.avatarUrl}
            <img class="user-avatar" src={u.avatarUrl} alt="" />
          {:else}
            <span class="user-avatar-ph">{u.displayName[0]}</span>
          {/if}
          <div>
            <div class="user-name">{u.displayName}</div>
            {#if u.email}<div class="user-email">{u.email}</div>{/if}
          </div>
        </div>
      {/if}

      {#if linMethod === 'oauth'}
        <div class="inline-form">
          {#if linOAuthWaiting}
            <p class="form-hint">Browser opened — approve access in Linear then return here.</p>
          {:else}
            <p class="form-hint">Opens Linear in the browser to authorize Arbor.</p>
          {/if}
          <div class="inline-form-row">
            <button class="btn-save linear-btn" onclick={startLinearOAuthFlow} disabled={linOAuthWaiting}>
              {#if linOAuthWaiting}<Loader2 size={11} class="spin" />{/if}
              {linOAuthWaiting ? 'Waiting for browser…' : 'Authorize with Linear'}
            </button>
            <button class="btn-cancel" onclick={() => { linOAuthWaiting = false; linMethod = null; linOAuthError = ''; linOAuthUnsub?.(); }}>Cancel</button>
          </div>
          {#if linOAuthError}
            <div class="provider-error"><XCircle size={12} />{linOAuthError}</div>
          {/if}
        </div>
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
    </div>
  </div>

  <!-- Jira (coming soon) -->
  <div class="record-row provider-disabled">
    <BrandTile brand="jira" />
    <div class="provider-main">
      <div class="provider-top">
        <div class="provider-info">
          <span class="provider-name">Jira</span>
          <span class="provider-desc">Atlassian issue tracker</span>
        </div>
        <span class="badge-soon">Coming soon</span>
      </div>
    </div>
  </div>
</div>

<style>
  /* ── Layout ─────────────────────────────────────────────────────────────── */
  /* ── Record card (container for a group of providers) ───────────────────── */
  .record-card {
    background: var(--bg-elevated);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    overflow: hidden;
  }

  .record-title {
    display: flex; align-items: baseline; gap: 8px;
    padding: 9px 14px 8px;
    background: var(--bg-overlay);
    border-bottom: 1px solid var(--border);
    font-size: 11px; font-weight: 700; letter-spacing: 0.05em;
    text-transform: uppercase; color: var(--text-secondary);
  }

  .record-subtitle {
    font-size: 10px; font-weight: 400; letter-spacing: 0;
    text-transform: none; color: var(--text-muted);
  }

  /* ── Provider row (inside record-card) ──────────────────────────────────── */
  .record-row {
    display: flex; align-items: flex-start; gap: 13px;
    padding: 13px 14px;
    border-top: 1px solid var(--border-subtle);
    transition: border-color var(--transition-fast), background var(--transition-fast);
  }
  .record-row:first-of-type { border-top: none; }
  .record-row.flow-active { background: color-mix(in srgb, var(--accent) 4%, transparent); }

  .provider-disabled { opacity: 0.45; pointer-events: none; }
  /* Provider tiles use the shared BrandTile widget — no local logo styles. */

  .provider-main { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 10px; }
  .provider-top  { display: flex; align-items: center; gap: 10px; }
  .provider-info { flex: 1; display: flex; flex-direction: column; gap: 2px; }
  .provider-name { font-size: 13px; font-weight: 600; color: var(--text-primary); }
  .provider-desc { font-size: 11px; color: var(--text-muted); }
  .provider-action { flex-shrink: 0; display: flex; align-items: center; gap: 8px; }

  /* ── Status indicators ──────────────────────────────────────────────────── */
  .status-checking, .status-wait {
    display: flex; align-items: center; gap: 5px;
    font-size: 11px; color: var(--text-muted);
  }
  .status-ok {
    display: flex; align-items: center; gap: 5px;
    font-size: 12px; font-weight: 500; color: var(--success, #6aab73);
  }

  /* ── Split button ───────────────────────────────────────────────────────── */
  .split-btn-wrap { position: relative; display: flex; z-index: var(--z-dropdown); }
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
    font-family: var(--font-ui-sans); cursor: pointer;
    transition: filter var(--transition-fast);
  }
  /* Brand-coloured CTAs — hard-coded #fff foreground (the bg is an absolute
     brand colour; `--text-on-accent` would resolve to dark in light themes). */
  .github-btn { background: var(--brand-github-green); color: #fff; }
  .github-btn:hover { filter: brightness(1.15); }
  .gitlab-btn { background: var(--brand-gitlab); color: #fff; }
  .gitlab-btn:hover { filter: brightness(1.1); }
  .linear-btn { background: var(--brand-linear); color: #fff; }
  .linear-btn:hover { filter: brightness(1.12); }

  .split-dropdown {
    position: absolute; top: calc(100% + 4px); right: 0;
    min-width: 210px;
    background: var(--bg-overlay); border: 1px solid var(--border);
    border-radius: var(--radius-md);
    box-shadow: 0 8px 24px rgba(0,0,0,0.4);
    padding: 4px; z-index: var(--z-tooltip);
    animation: dropIn var(--anim-dur-fast) ease-out;
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

  /* ── Inline forms ───────────────────────────────────────────────────────── */
  .inline-form { display: flex; flex-direction: column; gap: 8px; }
  .inline-form-row { display: flex; align-items: center; gap: 7px; flex-wrap: wrap; }
  .btn-save {
    padding: 5px 14px; border: none; border-radius: var(--radius-sm);
    font-family: var(--font-ui-sans); font-size: 12px; font-weight: 500;
    cursor: pointer; transition: filter var(--transition-fast); white-space: nowrap;
  }
  .btn-save:disabled { opacity: 0.45; cursor: not-allowed; }
  .btn-cancel {
    padding: 5px 10px; background: transparent;
    border: 1px solid var(--border); border-radius: var(--radius-sm);
    font-family: var(--font-ui-sans); font-size: 11px; color: var(--text-muted);
    cursor: pointer; transition: all var(--transition-fast); white-space: nowrap;
  }
  .btn-cancel:hover { background: var(--bg-hover); color: var(--text-primary); }

  .inline-check {
    display: flex; align-items: center; gap: 5px;
    font-size: 11px; color: var(--text-muted); cursor: pointer; white-space: nowrap;
  }
  .inline-check input { accent-color: var(--accent); cursor: pointer; }

  .form-hint {
    font-size: 10.5px; color: var(--text-muted); margin: 0; line-height: 1.5;
  }
  .form-hint code {
    font-family: var(--font-code); font-size: 10px; color: var(--accent);
    background: var(--bg-overlay); border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm); padding: 0 3px;
  }

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

  /* ── Connected user ─────────────────────────────────────────────────────── */
  .connected-user { display: flex; align-items: center; gap: 8px; padding: 6px 0 2px; }
  .user-avatar { width: 26px; height: 26px; border-radius: 50%; object-fit: cover; }
  .user-avatar-ph {
    width: 26px; height: 26px; border-radius: 50%;
    background: var(--accent-subtle); color: var(--accent);
    font-size: 11px; font-weight: 700;
    display: flex; align-items: center; justify-content: center;
  }
  .user-name  { font-size: 12px; font-weight: 500; color: var(--text-primary); }
  .user-email { font-size: 10px; color: var(--text-muted); }

  /* ── Error ──────────────────────────────────────────────────────────────── */
  .provider-error {
    display: flex; align-items: center; gap: 6px;
    font-size: 11px; color: var(--error, #f87171);
  }

  /* ── Misc ───────────────────────────────────────────────────────────────── */
  .badge-soon {
    font-size: 10px; font-weight: 600; padding: 2px 7px;
    background: var(--bg-overlay); border: 1px solid var(--border-subtle);
    border-radius: 99px; color: var(--text-muted);
  }

  /* ── Ghost buttons ──────────────────────────────────────────────────────── */
  .btn-ghost, .btn-ghost-danger {
    padding: 4px 10px; background: transparent;
    border: 1px solid var(--border); border-radius: var(--radius-sm);
    font-family: var(--font-ui-sans); font-size: 11px;
    cursor: pointer; transition: all var(--transition-fast); white-space: nowrap;
  }
  .btn-ghost { color: var(--text-muted); }
  .btn-ghost:hover { background: var(--bg-hover); color: var(--text-primary); }
  .btn-ghost-danger { color: var(--error, #f87171); border-color: var(--error, #f87171); }
  .btn-ghost-danger:hover { background: color-mix(in srgb, var(--error, #f87171) 12%, transparent); }

  /* ── Input helpers ──────────────────────────────────────────────────────── */
  .text-input {
    padding: 5px 8px; background: var(--bg-input); color: var(--text-primary);
    border: 1px solid var(--border); border-radius: var(--radius-sm);
    font-family: var(--font-ui-sans); font-size: 12px;
    outline: none; transition: border-color var(--transition-fast);
  }
  .text-input:focus { border-color: var(--accent); }

  .input-with-addon { display: flex; position: relative; }
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
