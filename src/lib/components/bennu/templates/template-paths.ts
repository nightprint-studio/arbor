/**
 * What a template file's path says about it: its kind, from the folder it is kept in, and the
 * extension of what it writes, from its name.
 *
 * Read off the path because that is where the backend keeps the answer too —
 * `bennu/templates/<kind>/<name>.<extension>.jinja` in the profile, and a built-in written out to be read
 * under `bennu/builtin-templates/<kind>/` in the data directory — so an open template needs no round trip
 * to know what it is.
 */

import { JINJA_FILE } from '$lib/utils/jinja-words';
import { TEMPLATE_KINDS, type TemplateKindId } from '$lib/ipc/bennu/templates';

/** The kind of a code template, or `null` for a Jinja file kept anywhere else. */
export function templateKindOfPath(path: string): TemplateKindId | null {
  const match = /[\\/]bennu[\\/](?:builtin-)?templates[\\/]([a-z-]+)[\\/][^\\/]+$/.exec(path);
  const kind = match?.[1] ?? '';
  return (TEMPLATE_KINDS as readonly string[]).includes(kind) ? (kind as TemplateKindId) : null;
}

/** A built-in template written out to be read — which the editor keeps read-only. */
export function isBuiltinTemplatePath(path: string | null | undefined): boolean {
  return /[\\/]bennu[\\/]builtin-templates[\\/]/.test(path ?? '');
}

/** `repository.java.jinja` → `repository` and `java`. The name ends at the first dot, as the store
 *  names them; a template with no inner extension writes plain text. */
export function templateFileParts(path: string): { name: string; extension: string } {
  const file = path.split(/[\\/]/).pop() ?? path;
  const bare = file.replace(JINJA_FILE, '');
  const dot = bare.indexOf('.');
  return dot < 0 ? { name: bare, extension: 'txt' } : { name: bare.slice(0, dot), extension: bare.slice(dot + 1) };
}
