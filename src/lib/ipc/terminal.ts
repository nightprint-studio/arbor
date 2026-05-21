import { invoke } from '@tauri-apps/api/core';
import type {
  TerminalInfo, TerminalExecResult,
  BuiltinShellInfo, TerminalsConfig,
} from '$lib/types/terminal';

export function terminalCreate(opts: {
  shell?: string;
  cwd?:   string;
  cols?:  number;
  rows?:  number;
}): Promise<TerminalInfo> {
  return invoke('terminal_create', {
    shell: opts.shell ?? null,
    cwd:   opts.cwd   ?? null,
    cols:  opts.cols  ?? null,
    rows:  opts.rows  ?? null,
  });
}

export function terminalWrite(id: string, data: string): Promise<void> {
  return invoke('terminal_write', { id, data });
}

export function terminalResize(id: string, cols: number, rows: number): Promise<void> {
  return invoke('terminal_resize', { id, cols, rows });
}

export function terminalClose(id: string): Promise<void> {
  return invoke('terminal_close', { id });
}

export function terminalList(): Promise<TerminalInfo[]> {
  return invoke('terminal_list');
}

export function terminalDefaultShell(): Promise<string> {
  return invoke('terminal_default_shell');
}

export function terminalExec(
  command:    string,
  cwd?:       string,
  pluginName?: string,
): Promise<TerminalExecResult> {
  return invoke('terminal_exec', {
    command,
    cwd:        cwd        ?? null,
    pluginName: pluginName ?? null,
  });
}

export const listBuiltinShells = () =>
  invoke<BuiltinShellInfo[]>('list_builtin_shells');

export const getTerminalsConfig = () =>
  invoke<TerminalsConfig>('get_terminals_config');

export const setTerminalsConfig = (config: TerminalsConfig) =>
  invoke<void>('set_terminals_config', { config });

/** Fire shell detection as a non-cancellable background job.
 *  Returns the job_id. Results arrive via `arbor://shell-detection-done`. */
export const startShellDetection = () =>
  invoke<string>('start_shell_detection');
