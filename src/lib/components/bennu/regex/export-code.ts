/**
 * A pattern as code in each host language — the "copy as" half of the Regex Tester.
 *
 * Only the *quoting* is done here (a backslash doubled for Java, a raw string for Rust); the pattern
 * itself is never rewritten, so what is copied is the pattern that was tested.
 */

import { escapeForLiteral } from '../inner-lang/units';
import type { Flavour } from './types';

const NO_KIND = { textBlock: false, raw: false, template: false };

/** The Java string literal that reads back as `pattern`. */
export function toJavaString(pattern: string): string {
  return `"${escapeForLiteral(pattern, 'java', NO_KIND) ?? ''}"`;
}

const JAVA_FLAG_NAMES: Record<string, string> = {
  i: 'CASE_INSENSITIVE', m: 'MULTILINE', s: 'DOTALL', x: 'COMMENTS', u: 'UNICODE_CASE',
  U: 'UNICODE_CHARACTER_CLASS', d: 'UNIX_LINES',
};

/** `Pattern.compile("…", Pattern.CASE_INSENSITIVE | …)`. */
export function toJavaCompile(pattern: string, flags: string): string {
  const names = [...flags].map((f) => JAVA_FLAG_NAMES[f]).filter(Boolean).map((n) => `Pattern.${n}`);
  return `Pattern.compile(${toJavaString(pattern)}${names.length ? `, ${names.join(' | ')}` : ''})`;
}

/** A `/…/flags` literal: a slash outside a class is escaped, a line break written as `\n`. */
export function toJsLiteral(pattern: string, flags: string): string {
  let out = '';
  let inClass = false;
  for (let i = 0; i < pattern.length; i++) {
    const c = pattern[i];
    if (c === '\\') { out += c + (pattern[i + 1] ?? ''); i++; continue; }
    if (c === '[') inClass = true;
    else if (c === ']') inClass = false;
    if (c === '/' && !inClass) out += '\\/';
    else if (c === '\n') out += '\\n';
    else if (c === '\r') out += '\\r';
    else out += c;
  }
  return `/${out || '(?:)'}/${flags.replace(/[^dgimsuvy]/g, '')}`;
}

/** `Regex::new(r"…")`, with as many `#` as the pattern needs. */
export function toRustRegex(pattern: string): string {
  let hashes = 0;
  while (pattern.includes('"' + '#'.repeat(hashes))) hashes++;
  const h = '#'.repeat(hashes);
  return `Regex::new(r${h}"${pattern}"${h}).unwrap()`;
}

/** Every rendering, labelled, for the flavour the pattern is written in. */
export function exportsFor(pattern: string, flavour: Flavour, flags: string): { label: string; code: string }[] {
  const java = [
    { label: 'Java string', code: toJavaString(pattern) },
    { label: 'Java Pattern.compile', code: toJavaCompile(pattern, flavour === 'java' ? flags : '') },
  ];
  const js = [{ label: 'JavaScript literal', code: toJsLiteral(pattern, flavour === 'js' ? flags : '') }];
  const rust = [{ label: 'Rust Regex::new', code: toRustRegex(pattern) }];
  if (flavour === 'js') return [...js, ...java.slice(0, 1), ...rust];
  if (flavour === 'rust') return [...rust, ...java.slice(0, 1), ...js];
  return [...java, ...js, ...rust];
}
