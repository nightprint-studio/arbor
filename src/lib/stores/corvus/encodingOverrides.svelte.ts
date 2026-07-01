/**
 * Per-(repo, file) encoding overrides.
 *
 * Auto-detection in the backend covers ~95% of real-world cases (UTF-8 vs
 * windows-1252 fallback). The remaining 5% are files where:
 *   · the leading bytes are pure ASCII so detection picks UTF-8 but the
 *     rest of the file is windows-1252 with non-ASCII bytes, or
 *   · ISO-8859-1 / ISO-8859-15 / MacRoman files that look identical to
 *     1252 in the leading bytes but differ on a handful of codepoints.
 *
 * The user can pin an explicit encoding for a given file via the
 * <EncodingPill /> widget; the choice persists across sessions in
 * localStorage so the override survives reloads.
 *
 * Keys: `${repoPath}::${filePath}` — both come from the backend (repo
 * absolute path + relative posix path). Using both means two repos with
 * the same relative file (e.g. `pom.xml`) keep independent overrides.
 */

const STORAGE_KEY = 'arbor:encoding-overrides';

class EncodingOverridesStore {
  // Plain object so $state proxying works on insert/delete via spread.
  #map = $state<Record<string, string>>({});

  constructor() {
    if (typeof localStorage === 'undefined') return;
    try {
      const raw = localStorage.getItem(STORAGE_KEY);
      if (raw) this.#map = JSON.parse(raw) ?? {};
    } catch { /* corrupted storage — start clean */ }
  }

  #key(repo: string, file: string): string {
    return `${repo}::${file}`;
  }

  /** Reactive read: getter returns current snapshot. */
  get(repo: string, file: string): string | undefined {
    return this.#map[this.#key(repo, file)];
  }

  set(repo: string, file: string, encoding: string): void {
    this.#map = { ...this.#map, [this.#key(repo, file)]: encoding };
    this.#persist();
  }

  clear(repo: string, file: string): void {
    const k = this.#key(repo, file);
    if (!(k in this.#map)) return;
    const next = { ...this.#map };
    delete next[k];
    this.#map = next;
    this.#persist();
  }

  /** Snapshot for an entire repo, useful when batching diff requests. */
  snapshotForRepo(repo: string): Record<string, string> {
    const prefix = `${repo}::`;
    const out: Record<string, string> = {};
    for (const [k, v] of Object.entries(this.#map)) {
      if (k.startsWith(prefix)) {
        out[k.slice(prefix.length)] = v;
      }
    }
    return out;
  }

  #persist(): void {
    try { localStorage.setItem(STORAGE_KEY, JSON.stringify(this.#map)); }
    catch { /* quota / disabled — not worth surfacing */ }
  }
}

export const encodingOverrides = new EncodingOverridesStore();

/**
 * Encoding choices surfaced in the picker. Order matches user-frequency on
 * Western European Windows codebases — UTF-8 first (modern default),
 * windows-1252 second (legacy Java/PHP). The CJK options sit at the end as
 * occasional fallbacks for imported sources. "Auto" clears any override.
 */
export const ENCODING_CHOICES: { value: string; label: string }[] = [
  { value: '',              label: 'Auto-detect' },
  { value: 'UTF-8',         label: 'UTF-8' },
  { value: 'windows-1252',  label: 'windows-1252 (CP1252)' },
  { value: 'ISO-8859-1',    label: 'ISO-8859-1 (Latin-1)' },
  { value: 'ISO-8859-15',   label: 'ISO-8859-15 (Latin-9, € sign)' },
  { value: 'macintosh',     label: 'MacRoman' },
  { value: 'windows-1250',  label: 'windows-1250 (Central European)' },
  { value: 'Shift_JIS',     label: 'Shift_JIS' },
  { value: 'GB18030',       label: 'GB18030' },
  { value: 'EUC-KR',        label: 'EUC-KR' },
];
