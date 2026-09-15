/**
 * Breakpoints in **library** source views — a class opened from a downloaded `-sources.jar` or the
 * JDK's `src.zip`.
 *
 * A library view is a read-only file in Bennu's data dir, under no project root, so nothing about
 * its *path* tells the debugger what to stop in. What it does have is a **class**: the view is the
 * source of exactly one top-level type, and that type — not the cache file — is the breakpoint's
 * identity. It is what the VM is asked about, and what reopens the view from the Breakpoints list.
 *
 * ## Real source only
 *
 * The same cache also holds **decompiled stubs**: signatures synthesized from bytecode when no
 * source exists. Their line numbers are fiction — line 118 of a stub is line 118 of a list Bennu
 * wrote, not of anything the VM's line table describes — so a stub offers no margin at all. The
 * absence is the explanation, the way it is on a line with no code: downloading the sources replaces
 * the stub, and the margin appears with the real lines.
 */

import type { BreakpointLanguage } from './breakpoint-lines';

/** How a stub announces itself — the first line the backend writes into every one. */
const STUB_MARKER = '// Decompiled from bytecode';

/** A Java identifier, which a view's file stem must be to name the class it declares. */
const JAVA_NAME = /^[A-Za-z_$][\w$]*$/;

/** `package org.springframework.web.client;` at the start of a line — never inside a comment,
 *  whose lines start with `*` or `//`. */
const PACKAGE_DECL = /^\s*package\s+([A-Za-z_$][\w$]*(?:\s*\.\s*[A-Za-z_$][\w$]*)*)\s*;/m;

/** Whether `source` is a decompiled stub rather than real source. */
export function isDecompiledStub(source: string): boolean {
  return source.trimStart().startsWith(STUB_MARKER);
}

/**
 * The fully-qualified top-level class a library view declares: its `package` plus the file's stem
 * (a view is named after the type it declares). `null` when the path is not a `.java` file named
 * like a class.
 */
export function libraryClassOfSource(path: string, source: string): string | null {
  const name = path.replace(/\\/g, '/').split('/').pop() ?? '';
  if (!name.endsWith('.java')) return null;
  const stem = name.slice(0, -'.java'.length);
  if (!JAVA_NAME.test(stem)) return null;
  const pkg = PACKAGE_DECL.exec(source)?.[1]?.replace(/\s+/g, '');
  return pkg ? `${pkg}.${stem}` : stem;
}

/** What the gutter needs to know about the buffer in front of it. */
export interface BreakTarget {
  path: string | null;
  /** The language its extension says, or `null` for one no debugger reads. */
  language: BreakpointLanguage | null;
  /** Whether it is a library source view (read-only, in the data dir). */
  libraryView: boolean;
  /** The buffer — read only for a library view, to tell real source from a stub. */
  source: string;
}

/**
 * Which language's breakpoint rules apply to a buffer, or `null` when it takes no breakpoints.
 *
 * A project file takes them by language. A library view takes them only when it is **real Java
 * source** whose class can be named — never a stub, and not while the buffer has not loaded (an
 * empty buffer cannot say which of the two it is).
 */
export function breakpointLanguageOf(target: BreakTarget): BreakpointLanguage | null {
  if (!target.path || !target.language) return null;
  if (!target.libraryView) return target.language;
  if (target.language !== 'java' || !target.source.trim() || isDecompiledStub(target.source)) {
    return null;
  }
  return libraryClassOfSource(target.path, target.source) ? 'java' : null;
}

/** `org.acme.Client$Builder$1` → `org.acme.Client` — the class with a source file of its own. */
export function outerClass(fqcn: string): string {
  const at = fqcn.indexOf('$');
  return at < 0 ? fqcn : fqcn.slice(0, at);
}

/**
 * Whether a breakpoint is the library breakpoint a frame stopped on: the frame's class is declared
 * in the breakpoint's file, on the breakpoint's line.
 */
export function isLibraryBreakpointAt(
  bp: { class?: string; line: number },
  frameClass: string,
  line: number | null,
): boolean {
  return !!bp.class && line !== null && bp.line === line
    && outerClass(bp.class) === outerClass(frameClass);
}

/** `org.springframework.web.client.RestClient` → its simple name and its package, for a list row. */
export function libraryClassLabel(fqcn: string): { simple: string; pkg: string } {
  const top = outerClass(fqcn);
  const dot = top.lastIndexOf('.');
  return dot < 0 ? { simple: top, pkg: '' } : { simple: top.slice(dot + 1), pkg: top.slice(0, dot) };
}
