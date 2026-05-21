<script lang="ts">
  import { CheckCircle2, Copy, ExternalLink, Loader2, XCircle } from 'lucide-svelte';
  import BrandTile from '$lib/components/shared/ui/BrandTile.svelte';
  import { openUrl } from '@tauri-apps/plugin-opener';
  import { listen } from '@tauri-apps/api/event';
  import {
    startGithubDeviceFlow, getGithubStatus, disconnectGithub,
    startGitlabOAuth, getGitlabStatus, disconnectGitlab,
    type DeviceFlowInfo,
  } from '$lib/ipc/auth';
  import { uiStore } from '$lib/stores/ui.svelte';
  import SectionHeader from '$lib/components/shared/ui/SectionHeader.svelte';
  import { tooltip } from '$lib/actions/tooltip';

  type OAuthState = 'checking' | 'disconnected' | 'connecting' | 'connected';

  let ghState        = $state<OAuthState>('checking');
  let ghError        = $state('');
  let ghOAuthWaiting = $state(false);
  let ghDeviceInfo   = $state<DeviceFlowInfo | null>(null);
  let ghOAuthUnsub: (() => void) | null = null;

  let glState        = $state<OAuthState>('checking');
  let glError        = $state('');
  let glOAuthWaiting = $state(false);
  let glOAuthUnsub: (() => void) | null = null;

  $effect(() => { checkOAuthStatus(); });

  async function checkOAuthStatus() {
    ghState = 'checking'; glState = 'checking';
    await Promise.allSettled([
      getGithubStatus()
        .then(ok => { ghState = ok ? 'connected' : 'disconnected'; })
        .catch(() => { ghState = 'disconnected'; }),
      getGitlabStatus()
        .then(ok => { glState = ok ? 'connected' : 'disconnected'; })
        .catch(() => { glState = 'disconnected'; }),
    ]);
  }

  async function startGithubConnect() {
    ghOAuthWaiting = true; ghError = ''; ghDeviceInfo = null;
    ghState = 'connecting';

    ghOAuthUnsub?.();
    ghOAuthUnsub = await listen<string | null>('arbor://github-oauth-done', ({ payload }) => {
      ghOAuthUnsub?.(); ghOAuthUnsub = null;
      ghOAuthWaiting = false; ghDeviceInfo = null;
      if (payload === null) {
        ghState = 'connected';
        uiStore.showToast('GitHub connected successfully!', 'success');
      } else {
        ghState = 'disconnected';
        ghError = payload;
      }
    });

    try {
      const info = await startGithubDeviceFlow();
      ghDeviceInfo = info;
      try { await openUrl(info.verification_uri); } catch { /* user can copy manually */ }
    } catch (err) {
      ghOAuthWaiting = false; ghState = 'disconnected';
      ghError = String(err);
      ghOAuthUnsub?.(); ghOAuthUnsub = null;
    }
  }

  function copyGhCode() {
    if (!ghDeviceInfo) return;
    navigator.clipboard.writeText(ghDeviceInfo.user_code).catch(() => {});
    uiStore.showToast('Code copied to clipboard', 'success');
  }

  function openGhVerification() {
    if (ghDeviceInfo) openUrl(ghDeviceInfo.verification_uri).catch(() => {});
  }

  function cancelGithubConnect() {
    ghOAuthUnsub?.(); ghOAuthUnsub = null;
    ghOAuthWaiting = false; ghDeviceInfo = null;
    ghState = 'disconnected';
  }

  async function disconnectGithubAccount() {
    ghOAuthUnsub?.(); ghOAuthUnsub = null;
    ghOAuthWaiting = false; ghDeviceInfo = null;
    await disconnectGithub().catch(() => {});
    ghState = 'disconnected'; ghError = '';
    uiStore.showToast('GitHub disconnected', 'info');
  }

  async function startGitlabConnect() {
    glOAuthWaiting = true; glError = '';
    glState = 'connecting';

    glOAuthUnsub?.();
    glOAuthUnsub = await listen<string | null>('arbor://gitlab-oauth-done', ({ payload }) => {
      glOAuthUnsub?.(); glOAuthUnsub = null;
      glOAuthWaiting = false;
      if (payload === null) {
        glState = 'connected';
        uiStore.showToast('GitLab connected successfully!', 'success');
      } else {
        glState = 'disconnected';
        glError = payload;
      }
    });

    try {
      const url = await startGitlabOAuth();
      try { await openUrl(url); } catch { /* user can copy manually */ }
    } catch (err) {
      glOAuthWaiting = false; glState = 'disconnected';
      glError = String(err);
      glOAuthUnsub?.(); glOAuthUnsub = null;
    }
  }

  async function disconnectGitlabAccount() {
    glOAuthUnsub?.(); glOAuthUnsub = null; glOAuthWaiting = false;
    await disconnectGitlab().catch(() => {});
    glState = 'disconnected'; glError = '';
    uiStore.showToast('GitLab disconnected', 'info');
  }
</script>

<SectionHeader title="Connected Accounts" description="Sign in with OAuth to enable features like PR creation and issue linking." />

<!-- GitHub -->
<div class="oauth-card">
  <BrandTile brand="github" size={22} tileSize={42} />
  <div class="oauth-main">
    <div class="oauth-top">
      <div class="oauth-details">
        <span class="oauth-name">GitHub</span>
        <span class="oauth-desc">Connect to github.com</span>
      </div>
      <div class="oauth-action">
        {#if ghState === 'checking'}
          <span class="oauth-checking"><Loader2 size={13} class="spin-slow" /> Checking…</span>
        {:else if ghState === 'connected'}
          <span class="oauth-connected"><CheckCircle2 size={13} /> Connected</span>
          <button class="btn-ghost-danger" onclick={disconnectGithubAccount}>Disconnect</button>
        {:else if ghState === 'connecting'}
          <span class="oauth-pending"><Loader2 size={13} class="spin-slow" /> Waiting for authorisation…</span>
          <button class="btn-ghost" onclick={cancelGithubConnect}>Cancel</button>
        {:else}
          <button class="btn-connect github-btn" onclick={startGithubConnect} disabled={ghOAuthWaiting}>Connect</button>
        {/if}
      </div>
    </div>

    {#if ghDeviceInfo}
      <div class="device-flow">
        <p class="device-hint">Open the verification page and enter this code:</p>
        <div class="device-code-row">
          <code class="device-code">{ghDeviceInfo.user_code}</code>
          <button class="device-copy" use:tooltip={'Copy code'} onclick={copyGhCode}><Copy size={12} /></button>
          <button class="device-open" onclick={openGhVerification}><ExternalLink size={11} /> Open {ghDeviceInfo.verification_uri.replace(/^https?:\/\//, '')}</button>
        </div>
      </div>
    {/if}

    {#if ghError}
      <div class="oauth-error"><XCircle size={12} />{ghError}</div>
    {/if}
  </div>
</div>

<!-- GitLab -->
<div class="oauth-card">
  <BrandTile brand="gitlab" size={22} tileSize={42} />
  <div class="oauth-main">
    <div class="oauth-top">
      <div class="oauth-details">
        <span class="oauth-name">GitLab</span>
        <span class="oauth-desc">Connect to gitlab.com</span>
      </div>
      <div class="oauth-action">
        {#if glState === 'checking'}
          <span class="oauth-checking"><Loader2 size={13} class="spin-slow" /> Checking…</span>
        {:else if glState === 'connected'}
          <span class="oauth-connected"><CheckCircle2 size={13} /> Connected</span>
          <button class="btn-ghost-danger" onclick={disconnectGitlabAccount}>Disconnect</button>
        {:else if glState === 'connecting'}
          <span class="oauth-pending"><Loader2 size={13} class="spin-slow" /> Waiting for browser…</span>
          <button class="btn-ghost" onclick={() => { glOAuthUnsub?.(); glOAuthWaiting=false; glState='disconnected'; }}>Cancel</button>
        {:else}
          <button class="btn-connect gitlab-btn" onclick={startGitlabConnect} disabled={glOAuthWaiting}>Connect</button>
        {/if}
      </div>
    </div>

    {#if glError}
      <div class="oauth-error"><XCircle size={12} />{glError}</div>
    {/if}
  </div>
</div>

<style>
  .oauth-card {
    display: flex;
    align-items: flex-start;
    gap: 14px;
    padding: 14px 16px;
    background: var(--bg-elevated);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    transition: border-color var(--transition-fast);
  }

  /* OAuth brand tiles render via the shared BrandTile widget. */

  .oauth-main {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .oauth-top {
    display: flex;
    align-items: center;
    gap: 12px;
  }

  .oauth-details {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .oauth-name { font-size: 13px; font-weight: 600; color: var(--text-primary); }
  .oauth-desc { font-size: 11px; color: var(--text-muted); line-height: 1.4; }

  .oauth-action {
    flex-shrink: 0;
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .oauth-checking, .oauth-pending {
    display: flex;
    align-items: center;
    gap: 5px;
    font-size: 11px;
    color: var(--text-muted);
  }

  .oauth-connected {
    display: flex;
    align-items: center;
    gap: 5px;
    font-size: 12px;
    font-weight: 500;
    color: var(--success, #6aab73);
  }

  .btn-connect {
    padding: 5px 14px;
    border: none;
    border-radius: var(--radius-sm);
    font-family: var(--font-ui-sans);
    font-size: 12px;
    font-weight: 500;
    cursor: pointer;
    transition: filter var(--transition-fast);
  }
  .btn-connect:disabled { opacity: 0.45; cursor: not-allowed; }
  .github-btn { background: var(--brand-github-green); color: #fff; }
  .github-btn:hover:not(:disabled) { filter: brightness(1.15); }
  .gitlab-btn { background: var(--brand-gitlab); color: #fff; }
  .gitlab-btn:hover:not(:disabled) { filter: brightness(1.1); }

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

  .oauth-error {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 11px;
    color: var(--error);
  }

  .device-flow {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 10px 12px;
    background: var(--bg-overlay);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
  }
  .device-hint { margin: 0; font-size: 11px; color: var(--text-muted); }
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

  :global(.spin-slow) { animation: spin-anim 1.4s linear infinite; }
  @keyframes spin-anim { from { transform: rotate(0deg); } to { transform: rotate(360deg); } }
</style>
