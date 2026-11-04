import { describe, expect, it } from 'vitest';
import { consoleHostLabel, isConsoleHost, splitConsoleHosts } from './process-groups';

const row = (pid: number, name: string, parent: number | null = null, label: string | null = null) =>
  ({ pid, parent, name, label });

describe('console hosts', () => {
  it('recognises the Windows console hosts and nothing else', () => {
    expect(isConsoleHost(row(1, 'conhost'))).toBe(true);
    expect(isConsoleHost(row(1, 'Conhost.exe'))).toBe(true);
    expect(isConsoleHost(row(1, 'OpenConsole'))).toBe(true);
    expect(isConsoleHost(row(1, 'java'))).toBe(false);
    expect(isConsoleHost(row(1, 'conhost-wrapper'))).toBe(false);
  });

  it('pulls the hosts out and pairs each with the program it serves', () => {
    const rows = [row(1, 'bennu-be'), row(2, 'java', 1), row(3, 'conhost', 2), row(4, 'cargo', 1), row(5, 'conhost', 4)];
    const { rest, hosts } = splitConsoleHosts(rows);
    expect(rest.map((r) => r.pid)).toEqual([1, 2, 4]);
    expect(hosts.map((h) => [h.host.pid, h.hosted?.pid])).toEqual([[3, 2], [5, 4]]);
  });

  it('leaves the program unknown when its parent is not in the table, or is itself a host', () => {
    const { hosts } = splitConsoleHosts([row(3, 'conhost', 99), row(4, 'conhost', 3), row(5, 'conhost')]);
    expect(hosts.map((h) => h.hosted)).toEqual([null, null, null]);
  });

  it('looks the program up in a wider table than the one being split', () => {
    const all = [row(2, 'node', 1, 'Angular language server'), row(3, 'conhost', 2)];
    const { hosts } = splitConsoleHosts([all[1]], all);
    expect(consoleHostLabel(hosts[0])).toBe('conhost → Angular language server');
  });

  it('names an orphan by its own name', () => {
    expect(consoleHostLabel({ host: row(3, 'conhost.exe'), hosted: null })).toBe('conhost');
  });

  it('groups nothing on a system without hosts', () => {
    const rows = [row(1, 'zsh'), row(2, 'java', 1)];
    expect(splitConsoleHosts(rows)).toEqual({ rest: rows, hosts: [] });
  });
});
