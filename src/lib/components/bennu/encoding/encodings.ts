/**
 * The encodings Bennu offers when converting or reinterpreting a file.
 *
 * The values are the canonical names the backend's `Codec::name()` reports, so a choice round-trips
 * through `from` / `to` unchanged. `bom` marks the encodings that have a byte-order mark to speak of
 * (the BOM option is meaningless for the rest).
 */
export interface EncodingChoice {
  value: string;
  label: string;
  bom: boolean;
}

export const BENNU_ENCODINGS: EncodingChoice[] = [
  { value: 'UTF-8',        label: 'UTF-8',                          bom: true },
  { value: 'windows-1252', label: 'windows-1252 (Cp1252)',          bom: false },
  { value: 'ISO-8859-1',   label: 'ISO-8859-1 (Latin-1)',           bom: false },
  { value: 'ISO-8859-15',  label: 'ISO-8859-15 (Latin-9, € sign)',  bom: false },
  { value: 'UTF-16LE',     label: 'UTF-16 LE',                      bom: true },
  { value: 'UTF-16BE',     label: 'UTF-16 BE',                      bom: true },
  { value: 'windows-1250', label: 'windows-1250 (Central European)', bom: false },
  { value: 'macintosh',    label: 'MacRoman',                       bom: false },
  { value: 'Shift_JIS',    label: 'Shift_JIS',                      bom: false },
  { value: 'GB18030',      label: 'GB18030',                        bom: false },
  { value: 'EUC-KR',       label: 'EUC-KR',                         bom: false },
];

/** Whether `value` is an encoding with a byte-order mark. */
export function supportsBom(value: string): boolean {
  return BENNU_ENCODINGS.find((e) => e.value === value)?.bom ?? false;
}

/**
 * The picker value for a label as a pom / config / the editor spells it — `Cp1252`, `utf-8`,
 * `ISO-8859-1` — or `null` when the list has no such encoding.
 */
export function canonicalEncoding(label: string): string | null {
  const norm = label.toLowerCase().replace(/[-_ ]/g, '');
  const aliases: Record<string, string> = {
    utf8: 'UTF-8', cp1252: 'windows-1252', windows1252: 'windows-1252', iso88591: 'ISO-8859-1',
    latin1: 'ISO-8859-1', iso885915: 'ISO-8859-15', utf16le: 'UTF-16LE', utf16: 'UTF-16LE',
    utf16be: 'UTF-16BE', cp1250: 'windows-1250', windows1250: 'windows-1250',
    macintosh: 'macintosh', macroman: 'macintosh', xmacroman: 'macintosh',
    shiftjis: 'Shift_JIS', gb18030: 'GB18030', euckr: 'EUC-KR',
  };
  return aliases[norm] ?? null;
}

/** Human size: `1.2 KB`. */
export function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / 1024 / 1024).toFixed(1)} MB`;
}
