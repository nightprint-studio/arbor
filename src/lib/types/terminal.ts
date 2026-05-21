// Mirrors src-tauri/src/terminal/mod.rs

export interface TerminalInfo {
  id:    string;
  shell: string;
  cwd:   string;
  title: string;
  cols:  number;
  rows:  number;
}

export interface TerminalExecResult {
  exit_code: number;
  stdout:    string;
  stderr:    string;
}

/** A tab entry managed by the frontend terminal store. */
export interface TerminalTab {
  /** UUID returned by terminal_create */
  id:     string;
  /** User-visible display name (e.g. "Command Prompt", "bash 2") */
  title:  string;
  shell:  string;
  cwd:    string;
}

/** Built-in shell entry mirrored from the Rust catalogue. */
export interface BuiltinShellInfo {
  id:        string;
  name:      string;
  cmd:       string;
  platforms: string[];
}

/** Detection result for one shell (parallels DetectedIde). */
export interface DetectedShell {
  id:            string;
  name:          string;
  available:     boolean;
  detected_path: string | null;
}

/** A user-defined custom terminal entry. */
export interface TerminalEntry {
  id:      string;
  name:    string;
  command: string;
  args:    string[];
}

/** Persisted [terminals] config block. */
export interface TerminalsConfig {
  default_shell:  string | null;
  custom_shells:  TerminalEntry[];
  path_overrides: Record<string, string>;
}
