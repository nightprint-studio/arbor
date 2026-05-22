<script lang="ts">
  import {
    GitBranch, Tag, RotateCcw, Copy,
    GitCommit, ArrowDown, CornerDownLeft, Zap, ArrowUpToLine, ExternalLink, TicketCheck, StickyNote,
    Search, Check, X, SkipForward, Link2,
  } from 'lucide-svelte';
  import { copyDeepLink } from '$lib/utils/deep-link-builder';
  import { copyToClipboard } from '$lib/utils/clipboard';
  import ContextMenu, { type MenuItem } from '../shared/ContextMenu.svelte';
  import { tabsStore } from '$lib/stores/tabs.svelte';
  import { uiStore } from '$lib/stores/ui.svelte';
  import { graphStore } from '$lib/stores/graph.svelte';
  import { repoStore } from '$lib/stores/repo.svelte';
  import { contributionStore } from '$lib/stores/contribution.svelte';
  import { pluginStore }       from '$lib/stores/plugin.svelte';
  import { ticketLinksStore } from '$lib/stores/ticket_links.svelte';
  import { bisectStore } from '$lib/stores/bisect.svelte';
  import { checkoutCommit, resetToCommit } from '$lib/ipc/branch';
  import { pushBranch, openInBrowser } from '$lib/ipc/remote';
  import { cherryPick, revertCommit } from '$lib/ipc/stage';
  import { getGraph } from '$lib/ipc/graph';
  import { getStatus } from '$lib/ipc/stage';
  import { firePluginAction } from '$lib/ipc/plugin';
  import type { CommitNode } from '$lib/types/git';

  let {
    node, x, y, onClose,
    onShowCreateBranch,
    onShowCreateTag,
    onShowLinkTicket,
    onShowNotes,
  }: {
    node: CommitNode;
    x: number; y: number;
    onClose: () => void;
    /** Lifted to parent — prevents modal from being destroyed with the context menu. */
    onShowCreateBranch: (n: CommitNode) => void;
    onShowCreateTag:    (n: CommitNode) => void;
    onShowLinkTicket?:  (n: CommitNode) => void;
    onShowNotes?:       (n: CommitNode) => void;
  } = $props();

  const tab    = $derived(tabsStore.activeTab);
  const status = $derived(repoStore.status);

  // Plugin context-menu items contributed to `arbor:context-menu:commit`.
  const pluginCommitItems = $derived(
    contributionStore.forPoint('arbor:context-menu:commit')
      .filter(c => !pluginStore.disabledPlugins.has(c.plugin_name))
  );

  const bisectActive    = $derived(bisectStore.state?.active === true);
  const bisectHasRange  = $derived((bisectStore.state?.good_hashes?.length ?? 0) > 0);

  const items: MenuItem[] = $derived([
    { id: 'checkout',      label: 'Checkout (detached HEAD)', icon: CornerDownLeft, iconColor: 'var(--accent)', disabled: node.is_head },
    { id: 'create-branch', label: 'New branch here…',         icon: GitBranch,      iconColor: 'var(--success)' },
    // Push only makes sense at HEAD
    ...(node.is_head ? [
      { id: 'sep-push', label: '', separator: true },
      { id: 'push',     label: 'Push current branch', icon: ArrowUpToLine, iconColor: 'var(--accent)', action: 'push' },
    ] : []),
    { id: 'sep1', label: '', separator: true },
    { id: 'cherry-pick', label: 'Cherry-pick',   icon: GitCommit, iconColor: '#c75450', disabled: node.is_head },
    { id: 'revert',      label: 'Revert commit', icon: RotateCcw, iconColor: 'var(--warning)' },
    { id: 'sep2', label: '', separator: true },
    { id: 'reset-soft',  label: 'Soft reset to here',  icon: ArrowDown, iconColor: 'var(--warning)' },
    { id: 'reset-mixed', label: 'Mixed reset to here', icon: ArrowDown, iconColor: 'var(--warning)' },
    { id: 'reset-hard',  label: 'Hard reset to here',  icon: ArrowDown, danger: true },
    { id: 'sep3', label: '', separator: true },
    { id: 'create-tag',   label: 'Tag this commit…',    icon: Tag, iconColor: 'var(--color-tag)' },
    { id: 'sep4',         label: '', separator: true },
    { id: 'copy-sha',     label: 'Copy full SHA',        icon: Copy, iconColor: 'var(--text-muted)' },
    { id: 'copy-short',   label: 'Copy short SHA',       icon: Copy, iconColor: 'var(--text-muted)' },
    { id: 'copy-message', label: 'Copy commit message',  icon: Copy, iconColor: 'var(--text-muted)' },
    { id: 'sep5',         label: '', separator: true },
    { id: 'open-browser', label: 'Open commit in browser', icon: ExternalLink, iconColor: '#20b2aa' },
    { id: 'copy-deep-link', label: 'Copy arbor:// link to this commit', icon: Link2, iconColor: '#20b2aa' },
    ...(ticketLinksStore.isEnabled() && onShowLinkTicket ? [
      { id: 'sep-ticket',  label: '', separator: true },
      { id: 'link-ticket', label: 'Link to ticket…', icon: TicketCheck, iconColor: 'var(--accent)' },
    ] : []),
    { id: 'sep-notes',  label: '', separator: true },
    { id: 'add-note',   label: 'Notes…', icon: StickyNote, iconColor: '#ffc66d' },
    { id: 'sep-bisect', label: '', separator: true },
    ...(bisectActive ? [
      { id: 'bisect-good', label: 'Bisect: Mark as Good', icon: Check, iconColor: 'var(--success)' },
      { id: 'bisect-bad',  label: 'Bisect: Mark as Bad',  icon: X,     iconColor: 'var(--error)' },
      ...(bisectHasRange ? [
        { id: 'bisect-skip', label: 'Bisect: Skip commit', icon: SkipForward, iconColor: 'var(--text-muted)' },
      ] : []),
    ] : [
      { id: 'bisect-start-bad', label: 'Start Bisect — mark as Bad', icon: Search, iconColor: 'var(--accent)' },
    ]),
    ...(pluginCommitItems.length > 0 ? [
      { id: 'sep-plugins', label: '', separator: true },
      ...pluginCommitItems.map(c => {
        const p = c.payload as { label?: string; action?: string };
        return {
          id:        `plugin:${c.plugin_name}:${p.action ?? ''}`,
          label:     p.label ?? '',
          icon:      Zap,
          iconColor: '#ffc66d',
        };
      }),
    ] : []),
  ] as MenuItem[]);

  async function handleSelect(id: string) {
    if (!tab) { onClose(); return; }
    try {
      switch (id) {
        case 'checkout': {
          // Capture values and close BEFORE awaiting — same pattern as bisect-start-bad.
          // setGraph() triggered by refresh() can cause Svelte 5 to re-evaluate
          // contextMenu.node as a fine-grained signal BEFORE the {#if contextMenu}
          // guard dismantles the component, producing a null-ref on contextMenu.
          const oid      = node.oid;
          const shortOid = node.short_oid;
          const tabId    = tab.id;
          onClose();
          await checkoutCommit(tabId, oid);
          uiStore.showToast(`Checked out ${shortOid}`, 'success');
          await refresh();
          return;
        }

        // These open a modal — delegate to parent so the modal is NOT owned
        // by this component and survives the context menu being unmounted.
        case 'create-branch':
          onShowCreateBranch(node);
          return; // intentionally skip onClose() — parent owns the flow

        case 'create-tag':
          onShowCreateTag(node);
          return;

        case 'push': {
          const branch = status?.current_branch ?? tab.currentBranch;
          if (!branch) { uiStore.showToast('No active branch to push', 'warning'); break; }
          uiStore.showToast(`Pushing '${branch}'…`, 'info');
          await pushBranch(tab.id, 'origin', `refs/heads/${branch}`, false);
          uiStore.showToast(`Pushed '${branch}' to origin`, 'success');
          await refresh();
          break;
        }

        case 'cherry-pick': {
          const cpOid = node.oid; const cpTabId = tab.id; onClose();
          const cpResult = await cherryPick(cpTabId, cpOid);
          await refresh();
          if (cpResult.has_conflicts) {
            uiStore.showToast(
              `Cherry-pick produced ${cpResult.conflicted_files.length} conflict${cpResult.conflicted_files.length === 1 ? '' : 's'} — resolve them in the Stage area`,
              'warning',
            );
          } else if (cpResult.no_changes) {
            uiStore.showToast(
              'Cherry-pick senza modifiche — le modifiche del commit sono già presenti su HEAD',
              'info',
            );
          } else {
            uiStore.showToast('Cherry-picked — rivedi e committa dalla Stage area', 'success');
          }
          return;
        }

        case 'revert': {
          const rvOid = node.oid; const rvTabId = tab.id; onClose();
          const rvResult = await revertCommit(rvTabId, rvOid);
          await refresh();
          if (rvResult.has_conflicts) {
            uiStore.showToast(
              `Revert produced ${rvResult.conflicted_files.length} conflict${rvResult.conflicted_files.length === 1 ? '' : 's'} — resolve them in the Stage area`,
              'warning',
            );
          } else {
            uiStore.showToast('Reverted', 'success');
          }
          return;
        }

        case 'reset-soft': {
          const rsOid = node.oid; const rsTabId = tab.id; onClose();
          await resetToCommit(rsTabId, rsOid, 'soft');
          uiStore.showToast('Soft reset done', 'success');
          await refreshAfterMutation(rsTabId);
          return;
        }
        case 'reset-mixed': {
          const rmOid = node.oid; const rmTabId = tab.id; onClose();
          await resetToCommit(rmTabId, rmOid, 'mixed');
          uiStore.showToast('Mixed reset done', 'success');
          await refreshAfterMutation(rmTabId);
          return;
        }
        case 'reset-hard': {
          const rhOid = node.oid; const rhTabId = tab.id; onClose();
          await resetToCommit(rhTabId, rhOid, 'hard');
          uiStore.showToast('Hard reset done', 'warning');
          await refreshAfterMutation(rhTabId);
          return;
        }

        case 'copy-sha':
          await copyToClipboard(node.oid, { successToast: 'SHA copied' });
          break;
        case 'copy-short':
          await copyToClipboard(node.short_oid, { successToast: 'Short SHA copied' });
          break;
        case 'copy-message':
          await copyToClipboard(node.summary, { successToast: 'Message copied' });
          break;

        case 'open-browser':
          await openInBrowser(tab.id, `commit:${node.oid}`);
          break;

        case 'copy-deep-link':
          await copyDeepLink({ kind: 'commit_jump', sha: node.oid }, tab.id);
          break;

        case 'link-ticket':
          onShowLinkTicket?.(node);
          return; // skip onClose() — parent owns the flow

        case 'add-note':
          onShowNotes?.(node);
          return;

        case 'bisect-start-bad': {
          // Capture values and close before awaiting: bisectStore.start() triggers
          // reactive updates that can reset the context menu node in CommitGraph,
          // causing a null-ref while this handler is still executing.
          const oid      = node.oid;
          const shortOid = node.short_oid;
          const tabId    = tab.id;
          onClose();
          await bisectStore.start(tabId);
          await bisectStore.mark(tabId, oid, 'bad');
          uiStore.showToast(
            `Bisect started — ${shortOid} marked as Bad. Right-click a known good commit.`,
            'info',
            6000,
          );
          return; // onClose() already called above
        }

        case 'bisect-good': {
          const oid = node.oid; const short = node.short_oid; const tabId = tab.id; onClose();
          try {
            await bisectStore.mark(tabId, oid, 'good');
            graphStore.setGraph(await getGraph(tabId, 0, 500));
            uiStore.showToast(`${short} marked as Good`, 'success');
          } catch (err) { uiStore.showToast(`Bisect: ${err}`, 'error'); }
          return;
        }

        case 'bisect-bad': {
          const oid = node.oid; const short = node.short_oid; const tabId = tab.id; onClose();
          try {
            await bisectStore.mark(tabId, oid, 'bad');
            graphStore.setGraph(await getGraph(tabId, 0, 500));
            uiStore.showToast(`${short} marked as Bad`, 'info');
          } catch (err) { uiStore.showToast(`Bisect: ${err}`, 'error'); }
          return;
        }

        case 'bisect-skip': {
          const oid = node.oid; const tabId = tab.id; onClose();
          try {
            await bisectStore.mark(tabId, oid, 'skip');
            graphStore.setGraph(await getGraph(tabId, 0, 500));
          } catch (err) { uiStore.showToast(`Bisect: ${err}`, 'error'); }
          return;
        }

        default:
          if (id.startsWith('plugin:')) {
            const parts      = id.split(':');
            const pluginName = parts[1];
            const action     = parts.slice(2).join(':');
            await firePluginAction(pluginName, action,
              JSON.stringify({ oid: node.oid, summary: node.summary, author: node.author }),
            );
          }
          break;
      }
    } catch (err) {
      uiStore.showToast(`${err}`, 'error');
    }
    onClose();
  }

  // Full refresh for any mutation that can change graph + index/workdir +
  // branch state at once (reset, revert, cherry-pick, …). Reloads the
  // graph data, refetches status so conflict banners light up immediately
  // without tab-switching, and bumps refreshTick so the sidebar reloads
  // branches/ahead-behind/stashes.
  async function refreshAfterMutation(tabId: string) {
    const [gd, s] = await Promise.all([
      getGraph(tabId, 0, 500),
      getStatus(tabId).catch(() => null),
    ]);
    graphStore.setGraph(gd);
    graphStore.refresh();
    if (s) repoStore.setStatus(s);
  }

  async function refresh() {
    if (!tab) return;
    await refreshAfterMutation(tab.id);
  }
</script>

<ContextMenu {items} {x} {y} onSelect={handleSelect} {onClose} />
