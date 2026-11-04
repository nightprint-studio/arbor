// Grouping rules of the Process Monitor — the pure half, so they are tested without a window.
//
// Windows starts a console host (`conhost.exe`, or `OpenConsole` under ConPTY) for every command-line
// program that has a console, so a machine running a few builds lists as many of them as programs. They
// are plumbing, not workload: they sit together in one collapsed group and each says which program it
// hosts. On macOS and Linux no process carries these names and nothing is grouped.

import type { ProcessRow } from '$lib/ipc/processes';

type Hostable = Pick<ProcessRow, 'pid' | 'parent' | 'name' | 'label'>;

/** One console host and the program it serves, when that program is in the table. */
export interface ConsoleHost<T extends Hostable> {
  host: T;
  /** The row of the process that owns the console — the host's parent — or `null` when it is not
   *  one of Arbor's (the host outlived it, or the parent is outside the tree). */
  hosted: T | null;
}

const HOST_NAMES = new Set(['conhost', 'conhost.exe', 'openconsole', 'openconsole.exe']);

export function isConsoleHost(row: Pick<ProcessRow, 'name'>): boolean {
  return HOST_NAMES.has(row.name.toLowerCase());
}

/** Split `rows` into everything that is not a console host and the hosts, each paired with the
 *  program it serves (looked up in `all`, which may be wider than `rows`). Order is preserved. */
export function splitConsoleHosts<T extends Hostable>(
  rows: readonly T[],
  all: readonly T[] = rows,
): { rest: T[]; hosts: ConsoleHost<T>[] } {
  const byPid = new Map(all.map((r) => [r.pid, r] as const));
  const rest: T[] = [];
  const hosts: ConsoleHost<T>[] = [];
  for (const row of rows) {
    if (!isConsoleHost(row)) {
      rest.push(row);
      continue;
    }
    const parent = row.parent != null ? byPid.get(row.parent) : undefined;
    hosts.push({ host: row, hosted: parent && !isConsoleHost(parent) ? parent : null });
  }
  return { rest, hosts };
}

/** What a console host is called: `conhost → java`, or just `conhost` when its program is unknown. */
export function consoleHostLabel<T extends Hostable>(entry: ConsoleHost<T>): string {
  const host = entry.host.name.replace(/\.exe$/i, '');
  return entry.hosted ? `${host} → ${entry.hosted.label ?? entry.hosted.name}` : host;
}
