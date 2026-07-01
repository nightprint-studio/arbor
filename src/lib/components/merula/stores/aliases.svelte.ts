/**
 * merula sound-alias store — the global `alias → target` name map (e.g.
 * `kick → "RolandTR808_bd"`, `timp → "perc.timpani"`). Resolved by the audio
 * registry so `s("alias")` / `.inst("alias")` play the target voice. **Global**,
 * not per-project / per-file: a dedicated merula-data file (`aliases.json`), NOT
 * localStorage (Arbor hard rule #11). Rune-store pattern (factory + getters);
 * mutators persist immediately, and the engine re-reads the file on the next eval
 * (so a save then Run is enough — no live registry mutation needed).
 */

import { getMerulaAliases, setMerulaAliases } from '$lib/ipc/merula/merula';

/** One alias row, for list rendering. */
export interface AliasEntry { name: string; target: string; }

function createAliasesStore() {
  // Source of truth: the alias map. Kept as a plain object (the wire shape).
  let map = $state<Record<string, string>>({});
  let loaded = $state(false);

  /** Sorted rows for the UI. */
  const entries = $derived<AliasEntry[]>(
    Object.entries(map)
      .map(([name, target]) => ({ name, target }))
      .sort((a, b) => a.name.localeCompare(b.name)),
  );

  function persist() { void setMerulaAliases({ ...map }).catch(() => {}); }

  return {
    get map()     { return map; },
    get entries() { return entries; },
    get count()   { return entries.length; },
    get loaded()  { return loaded; },
    /** The alias names (for autocomplete). */
    get names(): string[] { return Object.keys(map); },
    /** Whether `name` is a defined alias. */
    has(name: string): boolean { return name in map; },

    /** Load the persisted alias map (keeps defaults / empty on failure). */
    async load() {
      try { map = await getMerulaAliases(); loaded = true; }
      catch { /* first run / backend not ready — empty map */ }
    },

    /** Set (create or update) `name → target`. Empty name/target is ignored; a
     *  blank target removes the alias. Names are trimmed and lower-cased to match
     *  how the engine looks leaves up. Persists. */
    set(name: string, target: string) {
      const key = name.trim();
      const val = target.trim();
      if (!key) return;
      if (!val) { this.remove(key); return; }
      map = { ...map, [key]: val };
      persist();
    },
    /** Rename an alias key (preserving its target). No-op when the new name clashes
     *  or is blank. Persists. */
    rename(oldName: string, newName: string) {
      const key = newName.trim();
      if (!key || key === oldName || !(oldName in map) || key in map) return;
      const { [oldName]: target, ...rest } = map;
      map = { ...rest, [key]: target };
      persist();
    },
    /** Remove an alias. Persists. */
    remove(name: string) {
      if (!(name in map)) return;
      const { [name]: _drop, ...rest } = map;
      map = rest;
      persist();
    },
  };
}

export const aliasesStore = createAliasesStore();
