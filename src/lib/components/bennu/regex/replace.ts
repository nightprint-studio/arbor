/**
 * Substitution and splitting on top of a list of matches.
 *
 * `Matcher.replaceAll`'s template language (`$1`, `${name}`, `\$`) and `String.split`'s rules for
 * empty strings are Java behaviour, so they live here with the engine rather than being delegated
 * to the host's `String.replace`, whose templates (`$&`, `$<name>`) mean something else.
 */

import type { RegexMatch } from './types';

export interface Replaced { out: string; error: string | null }

/** Apply a Java `replaceAll` template to every match. */
export function javaReplaceAll(
  text: string, matches: RegexMatch[], template: string, groupNames: (string | null)[],
): Replaced {
  const groupCount = groupNames.length - 1;
  let out = '';
  let last = 0;
  for (const m of matches) {
    out += text.slice(last, m.start);
    const r = expandJava(template, m, groupNames, groupCount);
    if (r.error) return { out: '', error: r.error };
    out += r.out;
    last = m.end;
  }
  return { out: out + text.slice(last), error: null };
}

function expandJava(
  template: string, m: RegexMatch, names: (string | null)[], groupCount: number,
): Replaced {
  let out = '';
  for (let i = 0; i < template.length; i++) {
    const c = template[i];
    if (c === '\\') {
      i++;
      if (i >= template.length) return { out: '', error: 'character to be escaped is missing' };
      out += template[i];
    } else if (c === '$') {
      i++;
      if (i >= template.length) return { out: '', error: 'Illegal group reference: group index is missing' };
      if (template[i] === '{') {
        const end = template.indexOf('}', i);
        if (end < 0) return { out: '', error: "named capturing group is missing trailing '}'" };
        const name = template.slice(i + 1, end);
        const idx = names.indexOf(name);
        if (idx < 1) return { out: '', error: `No group with name {${name}}` };
        out += m.groups[idx]?.text ?? '';
        i = end;
      } else if (template[i] >= '0' && template[i] <= '9') {
        // The first digit is always part of the reference; later ones only while the group exists.
        let n = Number(template[i]);
        if (n > groupCount) return { out: '', error: `No group ${n}` };
        while (i + 1 < template.length && template[i + 1] >= '0' && template[i + 1] <= '9') {
          const next = n * 10 + Number(template[i + 1]);
          if (next > groupCount) break;
          n = next;
          i++;
        }
        out += m.groups[n]?.text ?? '';
      } else {
        return { out: '', error: 'Illegal group reference' };
      }
    } else {
      out += c;
    }
  }
  return { out, error: null };
}

/** Apply a Rust-style template (`$1`, `${name}`, `$name`, `$$`) to every match. */
export function rustReplaceAll(
  text: string, matches: RegexMatch[], template: string, groupNames: (string | null)[],
): Replaced {
  let out = '';
  let last = 0;
  for (const m of matches) {
    out += text.slice(last, m.start);
    out += template.replace(/\$(\$|\{(\w+)\}|(\w+))/g, (_all, d: string, braced?: string, bare?: string) => {
      if (d === '$') return '$';
      const ref = braced ?? bare!;
      const idx = /^\d+$/.test(ref) ? Number(ref) : groupNames.indexOf(ref);
      return idx >= 0 ? m.groups[idx]?.text ?? '' : '';
    });
    last = m.end;
  }
  return { out: out + text.slice(last), error: null };
}

/** `String.split(regex)` for a limit of zero: no leading empty string for a zero-width match at the
 *  start, and trailing empty strings removed. */
export function javaSplit(text: string, matches: RegexMatch[]): string[] {
  const parts: string[] = [];
  let index = 0;
  for (const m of matches) {
    if (index === 0 && m.start === 0 && m.end === 0) continue;
    parts.push(text.slice(index, m.start));
    index = m.end;
  }
  if (index === 0) return [text];
  parts.push(text.slice(index));
  while (parts.length > 0 && parts[parts.length - 1] === '') parts.pop();
  return parts;
}
