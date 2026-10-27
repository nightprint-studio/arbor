// Words and colours of the Dependencies panel, kept out of the components so the row, the
// conflicts group and the panel itself say the same thing about the same dependency.

import type { Dependency } from '$lib/ipc/bennu/deps';

export type Tone = 'success' | 'info' | 'warning' | 'neutral';

/** The colour a scope (Maven) or kind (Cargo) gets. Both vocabularies have the same three tiers:
 *  what the library needs, what its tests need, and what is supplied from outside. */
export function scopeTone(scope: string): Tone {
  if (scope === 'compile' || scope === 'normal' || scope === 'runtime') return 'success';
  if (scope === 'provided' || scope === 'system' || scope === 'build') return 'info';
  if (scope === 'test' || scope === 'dev') return 'warning';
  return 'neutral';
}

/** What the origin tag says, in the fewest words that are still true. */
export function originLabel(d: Dependency): string {
  switch (d.origin.kind) {
    case 'managed': return `pinned by ${d.origin.from}`;
    case 'inherited': return `from ${d.origin.from}`;
    default: return '';
  }
}

export function originTooltip(d: Dependency, isCargo: boolean): string {
  const unit = isCargo ? 'crate' : 'module';
  switch (d.origin.kind) {
    case 'managed':
      return isCargo
        ? `Declared here as \`workspace = true\` — ${d.origin.from}'s [workspace.dependencies] supplies ${d.version || 'the version'}.`
        : `Declared here without a version — ${d.origin.from}'s <dependencyManagement> pins ${d.version || 'it'}. Opens that pom.`;
    case 'inherited':
      return `Not declared in this ${unit}: inherited from ${d.origin.from}'s own <dependencies>. Opens that pom.`;
    default:
      return `Declared in this ${unit} (line ${d.declared_in.line}).`;
  }
}

/** `org.acme:lib` → `{ group: 'org.acme', name: 'lib' }`, for showing the two halves apart. */
export function splitCoord(coord: string): { group: string; name: string } {
  const i = coord.lastIndexOf(':');
  return i < 0 ? { group: '', name: coord } : { group: coord.slice(0, i), name: coord.slice(i + 1) };
}
