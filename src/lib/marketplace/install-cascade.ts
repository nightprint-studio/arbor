// ── Marketplace install cascade ──────────────────────────────────────────────
//
// Pure dep-graph walker used by the install flow. Given a target plugin and
// the current in-memory catalog, partitions its transitive required deps into:
//
//   • already installed   → skip (the host treats them as satisfied)
//   • in catalog, missing → `pending` (we'll install before the target)
//   • not in catalog      → `missing` (hard error — manual install needed)
//
// Optional deps (`d.optional === true`) don't participate: the plugin will
// still load without them. `pending` is returned in dep-first topological
// order so the caller can install sequentially without leaving a half-broken
// graph if one step fails.
//
// Lives here (not as a modal method) so it can be unit-tested in isolation
// and reused by future surfaces (Plugin Manager, CLI install, etc.).

import type { MarketplacePlugin } from '$lib/types/marketplace';

export interface InstallCascade {
  /** Deps that need to be installed before the target, dep-first order. */
  pending: MarketplacePlugin[];
  /** Required deps the catalog doesn't know about — surface as a hard error. */
  missing: { name: string; version: string }[];
}

export function resolveInstallCascade(
  target: MarketplacePlugin,
  catalog: MarketplacePlugin[],
): InstallCascade {
  const byName = new Map(catalog.map(p => [p.name, p]));
  const pending: MarketplacePlugin[] = [];
  const missing: { name: string; version: string }[] = [];
  const seen = new Set<string>([target.name]);

  function walk(node: MarketplacePlugin) {
    for (const d of node.dependencies ?? []) {
      if (d.optional) continue;
      if (seen.has(d.name)) continue;
      seen.add(d.name);
      const candidate = byName.get(d.name);
      if (!candidate) {
        missing.push({ name: d.name, version: d.version });
        continue;
      }
      if (candidate.installed) continue; // already on disk
      // Recurse so deps of deps are installed first.
      walk(candidate);
      pending.push(candidate);
    }
  }

  walk(target);
  return { pending, missing };
}
