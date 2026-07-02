/**
 * Bennu "Generate" preferences — the sticky option defaults for the
 * constructor / getters / setters / withers modal (fluent accessors, accessor
 * naming, the constructor variant). The user's last choices persist so the common
 * case is "open → Ctrl+Enter".
 *
 * SEAM — IN-MEMORY ONLY (MOCK persistence). Per CLAUDE.md rule 11 settings must
 * live on the filesystem, NOT localStorage. Until the backend `[bennu.generate]`
 * config section + `get_bennu_config` / `set_bennu_config` IPC land, this holds
 * the values in a rune for the session. The shape below is deliberately the shape
 * that future TOML section maps to 1:1 — wiring it up is: replace the setters to
 * call the IPC and add a `loadConfig()` invoked from the Bennu window shell,
 * exactly like the other config stores.
 *
 * Rune store — private `$state`, returned getters + methods (CLAUDE.md · Store
 * pattern).
 */

import type { NamingStyle, ConstructorVariant } from '$lib/components/bennu/java-generate';

/** Persisted shape — maps 1:1 to the future `[bennu.generate]` TOML section. */
export interface GeneratePrefs {
  fluent: boolean;
  naming: NamingStyle;
  constructorVariant: ConstructorVariant;
}

const DEFAULTS: GeneratePrefs = {
  fluent: false,
  naming: 'camelCase',
  constructorVariant: 'all',
};

function createGenerateStore() {
  // MOCK — session-only. Replace with values hydrated from `get_bennu_config`.
  let fluent = $state(DEFAULTS.fluent);
  let naming = $state<NamingStyle>(DEFAULTS.naming);
  let constructorVariant = $state<ConstructorVariant>(DEFAULTS.constructorVariant);

  return {
    get fluent()             { return fluent; },
    get naming()             { return naming; },
    get constructorVariant() { return constructorVariant; },

    // MOCK — these only mutate the in-memory rune. The real impl also persists
    // via `set_bennu_config` (debounced) so the choice survives a restart.
    setFluent(v: boolean)                      { fluent = v; },
    setNaming(v: NamingStyle)                  { naming = v; },
    setConstructorVariant(v: ConstructorVariant) { constructorVariant = v; },
  };
}

export const generateStore = createGenerateStore();
