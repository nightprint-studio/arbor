// ── Marketplace UI helpers ───────────────────────────────────────────────────
//
// Pure functions that map `MarketplacePlugin` / `MarketplaceTheme` shapes to
// presentation concerns (badge labels, source icons, permission chips,
// context-menu items). They live here — not in `catalog-helpers.ts` — because
// they pull in lucide-svelte component references, which catalog-helpers
// deliberately avoids to keep itself UI-agnostic.

import {
  Globe, Shield, FolderGit2, FolderOpen, Tag, Package, Link as LinkIcon,
  Plus, Trash2, RefreshCw, Power, PowerOff, ExternalLink, X,
} from 'lucide-svelte';
import type { MenuItem } from '$lib/components/shared/ContextMenu.svelte';
import type { MarketplacePlugin, MarketplaceTheme, MarketplaceSource } from '$lib/types/marketplace';

// ── Permission chips for the detail pane ─────────────────────────────────────

export interface PermissionChip {
  icon: typeof Globe;
  label: string;
  tone: 'safe' | 'warn' | 'danger';
}

export function permissionChips(p: MarketplacePlugin): PermissionChip[] {
  const out: PermissionChip[] = [];
  const perms = p.permissions;
  if (!perms) return out;
  if (perms.network?.length) out.push({ icon: Globe, label: `net: ${perms.network.join(', ')}`, tone: 'warn' });
  if (perms.fs && perms.fs !== 'none') {
    const unrestricted = perms.fs_scope?.includes('*');
    out.push({ icon: Shield, label: `fs: ${perms.fs}${unrestricted ? ' *' : ''}`, tone: unrestricted ? 'danger' : 'warn' });
  }
  if (perms.git && perms.git !== 'none') {
    out.push({ icon: FolderGit2, label: `git: ${perms.git}`, tone: perms.git === 'history_rewrite' ? 'danger' : 'warn' });
  }
  if (perms.terminal && perms.terminal !== 'none') {
    out.push({ icon: Tag, label: `term: ${perms.terminal}`, tone: perms.terminal === 'any' ? 'danger' : 'warn' });
  }
  return out;
}

// ── Source badge helpers ─────────────────────────────────────────────────────

export function sourceBadgeLabel(s: MarketplaceSource): string {
  switch (s) {
    case 'community': return 'Community';
    case 'custom':    return 'Custom source';
    case 'local':     return 'Local';
  }
}

export function sourceBadgeTooltip(s: MarketplaceSource): string {
  switch (s) {
    case 'community':
      return 'Listed on the arbor-extensions registry — vetted via PR review.';
    case 'custom':
      return 'Third-party git URL you added by hand. Inspect before enabling.';
    case 'local':
      return 'Manually installed plugin (zip import or dev folder). Not tied to a marketplace entry.';
  }
}

/** Icon component picked by source — used for the compact row marker that
 *  replaces the verbose Community/Custom/Local pills inline. */
export function sourceIcon(s: MarketplaceSource): typeof Globe {
  switch (s) {
    case 'community': return Globe;
    case 'custom':    return LinkIcon;
    case 'local':     return Package;
  }
}

// ── Tiny presentation utilities ──────────────────────────────────────────────

/** True when the icon value is an inline SVG string (Phase 1 mock); false
 *  when it's a URL we should hand to `<img src>` (Phase 2+). Inline SVGs need
 *  `{@html}` so `currentColor` inherits the parent's tint; `<img>` would
 *  render the SVG in an isolated context and ignore our CSS. */
export function isInlineSvg(s: string): boolean {
  return s.trimStart().startsWith('<svg');
}

/** Compact human-readable summary for a multi-select dropdown's trigger. */
export function summarizeSelection(label: string, selected: string[], total: number): string {
  if (selected.length === 0) return `${label}: any`;
  if (selected.length === 1) return `${label}: ${selected[0]}`;
  return `${label}: ${selected.length} of ${total}`;
}

// ── Context-menu builders ────────────────────────────────────────────────────

export function pluginCtxItems(p: MarketplacePlugin): MenuItem[] {
  const items: MenuItem[] = [];
  if (p.installed) {
    if (p.enabled) {
      items.push({ id: 'disable', label: 'Disable', icon: PowerOff, iconColor: 'var(--text-muted)' });
    } else {
      items.push({ id: 'enable',  label: 'Enable',  icon: Power,    iconColor: 'var(--success)' });
    }
    if (p.update_available) {
      items.push({
        id: 'update',
        label: `Update to v${p.update_available}`,
        icon: RefreshCw,
        iconColor: 'var(--accent)',
      });
    }
    items.push({ id: 'sep1', label: '', separator: true });
    items.push({ id: 'uninstall', label: 'Uninstall', icon: Trash2, danger: true });
  } else {
    items.push({ id: 'install', label: 'Install', icon: Plus, iconColor: 'var(--accent)' });
  }
  items.push({ id: 'sep2', label: '', separator: true });
  if (p.homepage) {
    items.push({ id: 'homepage', label: 'Open homepage', icon: ExternalLink });
  }
  // Local plugins (zip sideload, dev folder) have no remote repo to open —
  // reveal their on-disk folder instead. For community/custom we still surface
  // the repo link even when installed, since the source-of-truth lives upstream.
  if (p.source === 'local' && p.installed) {
    items.push({ id: 'explorer', label: 'Open in Explorer', icon: FolderOpen });
  } else {
    items.push({ id: 'repo', label: 'Open repository', icon: FolderGit2 });
  }
  if (p.source === 'custom') {
    items.push({ id: 'sep3', label: '', separator: true });
    items.push({ id: 'remove_source', label: 'Remove custom source', icon: X, danger: true });
  }
  return items;
}

export function themeCtxItems(t: MarketplaceTheme): MenuItem[] {
  const items: MenuItem[] = [];
  if (t.installed) {
    items.push({ id: 'uninstall_theme', label: 'Remove', icon: Trash2, danger: true });
  } else {
    items.push({ id: 'install_theme', label: 'Install', icon: Plus, iconColor: 'var(--accent)' });
  }
  items.push({ id: 'sep1', label: '', separator: true });
  items.push({ id: 'repo', label: 'Open repository', icon: FolderGit2 });
  return items;
}
