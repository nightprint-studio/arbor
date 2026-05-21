<script lang="ts">
  /**
   * Per-finding detail modal — opens when the user clicks a row in
   * `SecurityDetailModal`. Layout mirrors the GitHub Dependabot alert page:
   *
   *   Header: ShieldAlert · Title · state Badge · [Open in {Provider}]
   *   ─── severity-tinted accent stripe ────────────────────────────────
   *   Subheader: severity Badge · scanner · type · file:line · age
   *   ┌─ Main: rendered markdown ──┐ ┌─ Sidebar: severity hero + meta ─┐
   *
   * Markdown is sanitised through the shared `renderMarkdown` helper (same
   * code path used by MR/Issue modals). CVE/CWE/GHSA chips with a `url`
   * open the upstream advisory in the system browser.
   */
  import { onMount, tick } from 'svelte';
  import {
    ShieldAlert, ExternalLink, FileCode2, Bug, Tag, Calendar, Hash,
    AlertOctagon, CircleDot, CheckCircle2, Ban, Clock, Building2, Layers,
    Wrench,
  } from 'lucide-svelte';
  import { openUrl } from '@tauri-apps/plugin-opener';

  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import CopyButton from '$lib/components/shared/ui/CopyButton.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { renderMarkdown } from '$lib/utils/markdown';

  import { SEVERITY_META } from './severity-meta';
  import type { SecurityFinding, FindingState } from '$lib/types/security';

  interface Props {
    finding: SecurityFinding;
    onClose: () => void;
  }

  let { finding, onClose }: Props = $props();

  const sev = $derived(SEVERITY_META[finding.severity]);

  const providerLabel = $derived(
    finding.provider === 'gitlab' ? 'GitLab'
      : finding.provider === 'github' ? 'GitHub'
      : finding.provider === 'gitea' ? 'Gitea'
      : finding.provider === 'bitbucket' ? 'Bitbucket'
      : 'provider',
  );

  type StateInfo = {
    label: string;
    color: string;
    bg:    string;
    icon:  typeof CircleDot;
    tone:  'warning' | 'success' | 'error' | 'neutral';
  };

  const stateMeta = $derived.by<StateInfo>(() => {
    const map: Record<FindingState, StateInfo> = {
      detected:  { label: 'Detected',  color: 'var(--severity-high)', bg: 'var(--severity-high-bg)', icon: AlertOctagon, tone: 'error'   },
      confirmed: { label: 'Confirmed', color: 'var(--warning)',       bg: 'color-mix(in srgb, var(--warning) 14%, transparent)', icon: CircleDot,    tone: 'warning' },
      resolved:  { label: 'Resolved',  color: 'var(--success)',       bg: 'color-mix(in srgb, var(--success) 14%, transparent)', icon: CheckCircle2, tone: 'success' },
      dismissed: { label: 'Dismissed', color: 'var(--text-muted)',    bg: 'var(--bg-elevated)',     icon: Ban,           tone: 'neutral' },
    };
    return map[finding.state];
  });

  /** Strip duplicated `pkg-name:` prefixes (some advisories title like
   *  "rustls-webpki: rustls-webpki: Denial of service…"). */
  const cleanTitle = $derived.by(() => {
    let t = finding.title;
    const dup = t.match(/^(\S[^:]{0,40}:\s*)\1/);
    if (dup) t = t.slice(dup[1].length);
    return t.trim();
  });

  const fileLabel = $derived.by(() => {
    if (!finding.file_path) return null;
    return finding.start_line != null
      ? `${finding.file_path}:${finding.start_line}`
      : finding.file_path;
  });

  const ageLabel = $derived.by(() => {
    const d = finding.age_days;
    if (d < 1)   return 'less than a day ago';
    if (d === 1) return '1 day ago';
    if (d < 30)  return `${d} days ago`;
    if (d < 365) return `${Math.round(d / 30)} months ago`;
    return `${(d / 365).toFixed(1)} years ago`;
  });

  const createdAtLabel = $derived.by(() => {
    try {
      const d = new Date(finding.created_at);
      if (Number.isNaN(d.getTime())) return finding.created_at;
      return d.toLocaleString(undefined, {
        year:   'numeric',
        month:  'short',
        day:    '2-digit',
        hour:   '2-digit',
        minute: '2-digit',
      });
    } catch {
      return finding.created_at;
    }
  });

  const descriptionHtml = $derived(
    finding.description ? renderMarkdown(finding.description) : '',
  );
  const solutionHtml = $derived(
    finding.solution ? renderMarkdown(finding.solution) : '',
  );

  function openExternal() {
    if (finding.web_url) openUrl(finding.web_url).catch(() => {});
  }

  function openIdentifier(url: string | null) {
    if (url) openUrl(url).catch(() => {});
  }

  function idIcon(kind: string) {
    return kind.toLowerCase() === 'cve' ? Hash : Tag;
  }

  // ── Hover-reveal copy on code blocks ────────────────────────────────────
  // After the description renders, attach a small copy button overlay to
  // every `<pre>` so users can grab the PoC snippets. Done imperatively
  // because the HTML comes through `{@html}` — there's no Svelte loop we can
  // wrap each block in.
  let mainEl: HTMLElement | undefined = $state();

  $effect(() => {
    // Re-run whenever the description changes (e.g. switching findings).
    void descriptionHtml;
    if (!mainEl) return;
    let raf = 0;
    raf = requestAnimationFrame(() => attachCopyButtons(mainEl!));
    return () => cancelAnimationFrame(raf);
  });

  function attachCopyButtons(root: HTMLElement) {
    const pres = root.querySelectorAll<HTMLPreElement>('pre.md-pre');
    pres.forEach((pre) => {
      if (pre.dataset.copyAttached === '1') return;
      pre.dataset.copyAttached = '1';
      // Need positioning context for the absolute copy button.
      if (getComputedStyle(pre).position === 'static') {
        pre.style.position = 'relative';
      }

      const btn = document.createElement('button');
      btn.type = 'button';
      btn.className = 'md-pre-copy';
      btn.setAttribute('aria-label', 'Copy code');
      btn.innerHTML =
        '<svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="9" y="9" width="13" height="13" rx="2" ry="2"></rect><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"></path></svg>';

      btn.addEventListener('click', async (ev) => {
        ev.preventDefault();
        ev.stopPropagation();
        const code = pre.querySelector('code');
        const text = code?.textContent ?? pre.textContent ?? '';
        try {
          await navigator.clipboard.writeText(text);
          btn.classList.add('copied');
          btn.innerHTML =
            '<svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round"><polyline points="20 6 9 17 4 12"></polyline></svg>';
          setTimeout(() => {
            btn.classList.remove('copied');
            btn.innerHTML =
              '<svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="9" y="9" width="13" height="13" rx="2" ry="2"></rect><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"></path></svg>';
          }, 1400);
        } catch { /* ignore — clipboard denied */ }
      });

      pre.appendChild(btn);
    });
  }

  // Force a tick after mount so the effect above has a DOM to walk.
  onMount(async () => { await tick(); });
</script>

<Modal
  {onClose}
  width="min(96vw, 1040px)"
  height="84vh"
  padBody={false}
  zIndex="var(--z-modal-picker)"
  ariaLabel="Security finding detail"
>
  {#snippet header()}
    <ModalHeader {onClose}>
      <span class="sec-title-icon" style:color={sev.color}>
        <ShieldAlert size={14} />
      </span>
      <span class="modal-title" title={cleanTitle}>{cleanTitle}</span>

      <Badge
        variant="chip"
        size="md"
        color={stateMeta.color}
        bg={stateMeta.bg}
        border={`color-mix(in srgb, ${stateMeta.color} 32%, transparent)`}
      >
        {#snippet icon()}<stateMeta.icon size={10} />{/snippet}
        {stateMeta.label}
      </Badge>

      {#snippet actions()}
        {#if finding.web_url}
          <Button
            variant="secondary"
            size="sm"
            onclick={openExternal}
            tooltip={`Open in ${providerLabel}`}
          >
            {#snippet iconStart()}<ExternalLink size={13} />{/snippet}
            Open in {providerLabel}
          </Button>
        {/if}
      {/snippet}
    </ModalHeader>
  {/snippet}

  <div class="body">
    <!-- Severity accent stripe — risk visible before reading. -->
    <div class="sev-stripe" style:--sev-color={sev.color}></div>

    <!-- ── Subheader: scanner / file / age ─────────────────────────────── -->
    <div class="subheader">
      <Badge
        variant="chip"
        size="md"
        color={sev.color}
        bg={sev.bgColor}
        border={`color-mix(in srgb, ${sev.color} 35%, transparent)`}
      >{sev.label}</Badge>

      <span class="sub-meta">
        {#if finding.scanner}
          <Badge variant="tone" tone="neutral" size="md">
            {#snippet icon()}<Bug size={11} />{/snippet}
            {finding.scanner}
          </Badge>
        {/if}
        {#if finding.report_type}
          <Badge variant="tone" tone="accent" size="md">{finding.report_type}</Badge>
        {/if}
        {#if fileLabel}
          <span class="file-chip" use:tooltip={fileLabel}>
            <FileCode2 size={11} />
            <span class="file-text">{fileLabel}</span>
          </span>
        {/if}
        <span class="age" use:tooltip={createdAtLabel}>
          <Calendar size={11} />{ageLabel}
        </span>
      </span>
    </div>

    <!-- ── Content grid ────────────────────────────────────────────────── -->
    <div class="grid">
      <section class="main" bind:this={mainEl}>
        <!-- Solution callout — surface remediation BEFORE the description so
             the actionable bit isn't buried under the technical write-up.
             Tinted with the success palette to read as "here's the fix". -->
        {#if solutionHtml}
          <aside class="solution">
            <div class="solution-hdr">
              <Wrench size={13} />
              <span>Suggested fix</span>
            </div>
            <div class="markdown md-body solution-body">
              {@html solutionHtml}
            </div>
          </aside>
        {/if}

        {#if descriptionHtml}
          <!-- Provider descriptions (Dependabot, GitLab Security Bot, …) almost
               always start with their own "Summary" / "Description" heading, so
               we don't prefix one here — would render as a duplicate. -->
          <div class="markdown md-body">
            {@html descriptionHtml}
          </div>
        {:else}
          <div class="no-desc">
            <ShieldAlert size={22} />
            <p>No description provided by the scanner.</p>
            {#if finding.web_url}
              <Button variant="ghost" size="sm" onclick={openExternal}>
                {#snippet iconEnd()}<ExternalLink size={11} />{/snippet}
                Open in {providerLabel} for full details
              </Button>
            {/if}
          </div>
        {/if}
      </section>

      <aside class="side">
        <!-- Severity hero — filled, no header, the colored panel IS the label. -->
        <div
          class="sev-hero"
          style:--sev-color={sev.color}
          style:--sev-bg={sev.bgColor}
        >
          <ShieldAlert size={18} />
          <div class="sev-hero-text">
            <span class="sev-hero-label">{sev.label}</span>
            {#if finding.report_type}
              <span class="sev-hero-sub">{finding.report_type.replace(/_/g, ' ')}</span>
            {/if}
          </div>
        </div>

        {#if fileLabel}
          <div class="meta-section">
            <div class="meta-label"><FileCode2 size={11} /> Location</div>
            <div class="meta-value mono path-row">
              <span class="path-text" title={fileLabel}>{fileLabel}</span>
              <CopyButton
                value={fileLabel}
                variant="icon"
                title="Copy path"
                toastSuccess="Path copied"
              />
            </div>
          </div>
        {/if}

        {#if finding.scanner}
          <div class="meta-section">
            <div class="meta-label"><Bug size={11} /> Scanner</div>
            <div class="meta-value">{finding.scanner}</div>
          </div>
        {/if}

        {#if finding.identifiers.length > 0}
          <div class="meta-section">
            <div class="meta-label"><Tag size={11} /> Identifiers</div>
            <div class="ids">
              {#each finding.identifiers as id (id.kind + ':' + id.value)}
                {@const Icon = idIcon(id.kind)}
                <button
                  type="button"
                  class="id-chip"
                  class:linkable={!!id.url}
                  disabled={!id.url}
                  onclick={() => openIdentifier(id.url)}
                  use:tooltip={id.url ? `Open ${id.value}` : id.value}
                >
                  <Icon size={10} />
                  <span class="id-kind">{id.kind.toUpperCase()}</span>
                  <span class="id-val">{id.value}</span>
                  {#if id.url}<ExternalLink size={9} class="id-ext" />{/if}
                </button>
              {/each}
            </div>
          </div>
        {/if}

        <div class="meta-section">
          <div class="meta-label"><Calendar size={11} /> Detected</div>
          <div class="meta-value">{createdAtLabel}</div>
        </div>

        <div class="meta-section">
          <div class="meta-label"><Clock size={11} /> Age</div>
          <div class="meta-value">{ageLabel}</div>
        </div>

        <div class="meta-section">
          <div class="meta-label"><Layers size={11} /> State</div>
          <div class="meta-value">
            <Badge variant="tone" tone={stateMeta.tone} size="sm">{stateMeta.label}</Badge>
          </div>
        </div>

        <div class="meta-section">
          <div class="meta-label"><Building2 size={11} /> Provider</div>
          <div class="meta-value">{providerLabel}</div>
        </div>
      </aside>
    </div>
  </div>
</Modal>

<style>
  .body {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
    background: var(--bg-base);
    font-family: var(--font-ui-sans);
  }

  /* ── Severity accent stripe ────────────────────────────────────────── */
  /* 2px coloured band tinted by severity, sitting between the modal header
     and the subheader. Lets the reader register "this is critical / high"
     at a glance, before parsing any text. */
  .sev-stripe {
    flex-shrink: 0;
    height: 2px;
    background: linear-gradient(
      90deg,
      var(--sev-color) 0%,
      color-mix(in srgb, var(--sev-color) 55%, transparent) 65%,
      transparent 100%
    );
  }

  /* ── Subheader ─────────────────────────────────────────────────────── */
  /* Transparent over the modal body — the only visual separator is the
     bottom edge. `--border-subtle` was vanishing on `--bg-base`, so we
     promote to `--border` (1px is enough). */
  .subheader {
    flex-shrink: 0;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 16px;
    border-bottom: 1px solid var(--border);
    overflow: hidden;
  }
  .sub-meta {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    color: var(--text-secondary);
    font-size: 11px;
    min-width: 0;
    overflow: hidden;
  }
  /* file path is a path-style chip, not really a "badge" — keep custom */
  .file-chip {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 2px 7px;
    border: 1px solid var(--border-subtle);
    background: var(--bg-overlay);
    border-radius: var(--radius-sm);
    color: var(--text-secondary);
    font-family: var(--font-code);
    line-height: 1.4;
    min-width: 0;
    flex-shrink: 1;
  }
  .file-chip .file-text {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 360px;
  }
  .age {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    color: var(--text-muted);
    flex-shrink: 0;
  }

  /* ── Grid ──────────────────────────────────────────────────────────── */
  .grid {
    flex: 1;
    min-height: 0;
    display: grid;
    grid-template-columns: 1fr 240px;
    overflow: hidden;
  }
  .main {
    overflow: auto;
    padding: 18px 22px 24px;
    border-right: 1px solid var(--border-subtle);
  }
  .side {
    overflow: auto;
    padding: 14px 14px 20px;
    display: flex;
    flex-direction: column;
    gap: 14px;
    background: var(--bg-base);
  }

  /* ── Markdown body ─────────────────────────────────────────────────── */
  .markdown {
    color: var(--text-primary);
    font-size: 13px;
    line-height: 1.6;
    word-wrap: break-word;
    overflow-wrap: anywhere;
  }
  .markdown :global(.md-p)       { margin: 0 0 10px; }
  .markdown :global(.md-p:last-child) { margin-bottom: 0; }
  .markdown :global(.md-h1),
  .markdown :global(.md-h2),
  .markdown :global(.md-h3) {
    margin: 20px 0 8px;
    font-weight: 600;
    color: var(--text-primary);
  }
  .markdown :global(.md-h1) { font-size: 16px; }
  .markdown :global(.md-h2) {
    font-size: 14px;
    padding-bottom: 4px;
    border-bottom: 1px solid var(--border-subtle);
  }
  .markdown :global(.md-h3) { font-size: 13px; color: var(--text-secondary); }
  .markdown :global(.md-h1:first-child),
  .markdown :global(.md-h2:first-child),
  .markdown :global(.md-h3:first-child) { margin-top: 0; }
  .markdown :global(.md-inline-code) {
    font-family: var(--font-code);
    font-size: 0.9em;
    padding: 1px 5px;
    border-radius: var(--radius-sm);
    background: var(--bg-overlay);
    border: 1px solid var(--border-subtle);
    color: var(--accent);
  }
  .markdown :global(.md-pre) {
    margin: 10px 0;
    padding: 10px 36px 10px 12px;
    border-radius: var(--radius-md);
    background: var(--bg-overlay);
    border: 1px solid var(--border-subtle);
    overflow-x: auto;
  }
  .markdown :global(.md-code) {
    font-family: var(--font-code);
    font-size: 12px;
    line-height: 1.5;
    background: none;
    padding: 0;
  }
  .markdown :global(.md-ul),
  .markdown :global(.md-ol) {
    margin: 6px 0 10px;
    padding-left: 22px;
  }
  .markdown :global(.md-ul li),
  .markdown :global(.md-ol li) { margin: 2px 0; }
  .markdown :global(.md-bq) {
    margin: 10px 0;
    padding: 8px 12px;
    border-left: 3px solid var(--accent-subtle);
    border-radius: 0 var(--radius-sm) var(--radius-sm) 0;
    color: var(--text-muted);
    font-style: italic;
    background: var(--bg-overlay);
  }
  .markdown :global(.md-link) {
    color: var(--accent);
    text-decoration: underline;
    text-decoration-color: color-mix(in srgb, var(--accent) 40%, transparent);
  }
  .markdown :global(.md-spacer) { height: 6px; }
  .markdown :global(.md-hr) {
    margin: 14px 0;
    border: none;
    border-top: 1px solid var(--border-subtle);
  }

  /* ── Code-block copy button (injected by attachCopyButtons) ─────────── */
  .markdown :global(.md-pre-copy) {
    position: absolute;
    top: 6px;
    right: 6px;
    width: 22px;
    height: 22px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    padding: 0;
    background: var(--bg-elevated);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    color: var(--text-muted);
    cursor: pointer;
    opacity: 0;
    transition: opacity var(--transition-fast),
                color var(--transition-fast),
                background var(--transition-fast);
  }
  .markdown :global(.md-pre:hover .md-pre-copy),
  .markdown :global(.md-pre-copy:focus-visible) { opacity: 1; }
  .markdown :global(.md-pre-copy:hover) {
    color: var(--text-primary);
    background: var(--bg-hover);
  }
  .markdown :global(.md-pre-copy.copied) {
    color: var(--success);
    border-color: color-mix(in srgb, var(--success) 38%, transparent);
    opacity: 1;
  }

  /* ── Solution callout ─────────────────────────────────────────────── */
  /* Sits above the description so users see the remediation first. Subtle
     success tint conveys "actionable" without being shouty. */
  .solution {
    margin-bottom: 18px;
    padding: 10px 14px;
    border-radius: var(--radius-md);
    background: color-mix(in srgb, var(--success) 8%, var(--bg-elevated));
    border: 1px solid color-mix(in srgb, var(--success) 28%, transparent);
    border-left: 3px solid var(--success);
  }
  .solution-hdr {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    margin-bottom: 6px;
    font-size: 10px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.6px;
    color: var(--success);
  }
  .solution-body { font-size: 12.5px; }
  .solution-body :global(.md-inline-code) {
    color: var(--success);
    background: color-mix(in srgb, var(--success) 14%, transparent);
    border-color: color-mix(in srgb, var(--success) 30%, transparent);
  }

  .no-desc {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 10px;
    padding: 56px 24px;
    color: var(--text-muted);
    text-align: center;
  }
  .no-desc p { margin: 0; font-size: 12px; }

  /* ── Sidebar: severity hero ────────────────────────────────────────── */
  .sev-hero {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 12px 12px;
    border-radius: var(--radius-md);
    background: linear-gradient(
      135deg,
      color-mix(in srgb, var(--sev-color) 22%, var(--bg-elevated)) 0%,
      color-mix(in srgb, var(--sev-color) 8%, var(--bg-elevated)) 100%
    );
    border: 1px solid color-mix(in srgb, var(--sev-color) 32%, transparent);
    color: var(--sev-color);
  }
  .sev-hero-text {
    display: flex;
    flex-direction: column;
    line-height: 1.15;
    min-width: 0;
  }
  .sev-hero-label {
    font-size: 14px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.6px;
  }
  .sev-hero-sub {
    margin-top: 2px;
    font-size: 10px;
    font-weight: 500;
    color: var(--text-secondary);
    text-transform: lowercase;
    letter-spacing: 0.2px;
  }

  /* ── Sidebar: meta sections (IssueDetailModal pattern) ─────────────── */
  .meta-section {
    display: flex;
    flex-direction: column;
    gap: 4px;
    min-width: 0;
  }
  .meta-label {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font-size: 10px;
    font-weight: 600;
    letter-spacing: 0.05em;
    text-transform: uppercase;
    color: var(--text-muted);
  }
  .meta-value {
    font-size: 12px;
    color: var(--text-primary);
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
  }
  .meta-value.mono {
    font-family: var(--font-code);
    font-size: 11px;
  }
  .path-row { justify-content: space-between; }
  .path-text {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    min-width: 0;
    flex: 1;
  }

  /* ── Identifiers ───────────────────────────────────────────────────── */
  .ids {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }
  .id-chip {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 3px 8px;
    background: var(--bg-overlay);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    color: var(--text-secondary);
    font-family: var(--font-code);
    font-size: 10.5px;
    line-height: 1.5;
    cursor: default;
    transition: background var(--transition-fast), border-color var(--transition-fast),
                color var(--transition-fast);
  }
  .id-chip.linkable { cursor: pointer; }
  .id-chip.linkable:hover {
    background: var(--bg-hover);
    border-color: color-mix(in srgb, var(--accent) 40%, var(--border-subtle));
    color: var(--text-primary);
  }
  .id-chip:disabled { color: var(--text-secondary); }
  .id-kind {
    color: var(--text-muted);
    font-weight: 700;
    letter-spacing: 0.3px;
  }
  .id-chip.linkable:hover .id-kind { color: var(--accent); }
  :global(.id-chip .id-ext) { color: var(--text-muted); }

  /* ── Header bits ───────────────────────────────────────────────────── */
  .modal-title {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 720px;
  }
  .sec-title-icon {
    display: inline-flex;
    align-items: center;
    flex-shrink: 0;
  }

  /* Narrow viewport — stack columns. */
  @media (max-width: 760px) {
    .grid {
      grid-template-columns: 1fr;
    }
    .main { border-right: none; border-bottom: 1px solid var(--border-subtle); }
  }
</style>
