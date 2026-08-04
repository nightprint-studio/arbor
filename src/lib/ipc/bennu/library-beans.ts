/**
 * Library-bean IPC — the Spring beans an **allowlisted** dependency declares.
 *
 * Mirrors `crates/products/bennu/be/src/library_beans.rs` in snake_case; the Rust side is
 * authoritative. In its own file rather than `index.ts` for the same reason as `ext.ts` and
 * `deps.ts`: concurrent edits to the main bennu IPC surface shouldn't race.
 *
 * **These are declarations, not facts.** A bean in a jar is what Spring *may* register —
 * `@ConditionalOnMissingBean` and friends decide the rest, and deciding them faithfully means
 * running Spring's own condition evaluator. So this view is read-only in the strongest sense:
 * nothing here takes part in injection resolution, completion or any diagnostic, and a bean
 * carrying `conditions` has to be presented as gated rather than listed like the rest.
 */

import { bennu } from '../rpc';

/** One bean declared inside a dependency. */
export interface LibraryBean {
  /** The name Spring would register it under. */
  name: string;
  /** Dotted FQCN of the implementation (for a `@Bean` method, its return type). */
  fqcn: string;
  /** What was written — `@Service`, `@Bean`, `@Configuration`. */
  stereotype: string;
  /** The declaring class, dotted. For a `@Bean` method this is the configuration class,
   *  which is the thing you actually want to open. */
  declared_in: string;
  /** The `@ConditionalOn…` gates, as written. **Non-empty means it may not exist here.** */
  conditions: string[];
  primary: boolean;
}

/** The beans one dependency contributes. */
export interface LibraryBeanGroup {
  group_id: string;
  artifact_id: string;
  version: string;
  /** `com.acme:shared-security:2.1.0`. */
  coordinate: string;
  beans: LibraryBean[];
}

/** The allowlisted dependencies' beans for the project at `root`, grouped by artifact.
 *
 *  Empty — and free — while the allowlist is empty, which is the default: no jar is opened
 *  until somebody names the artifacts they want read (Settings → Spring → Beans).
 *  Wire: `bennu_library_beans` — `{ root }`. */
export function libraryBeans(root: string): Promise<LibraryBeanGroup[]> {
  return bennu('bennu_library_beans', { args: { root } });
}
