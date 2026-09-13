/**
 * Process monitor IPC — what Arbor's own processes are costing this machine.
 *
 * Served by the **shell**, which is the only process every backend, language server and run is a
 * descendant of, so one call answers for the whole application rather than one per product.
 */

import { platform } from './rpc';

/** What a process is *to Arbor*, which is a different question from what it is called. */
export type ProcessKind = 'shell' | 'backend' | 'language-server' | 'child' | 'helper';

/** One process of Arbor's. */
export interface ProcessRow {
  pid: number;
  /** The process that started it, within Arbor's own tree. `null` only for the shell. */
  parent: number | null;
  /** The executable's file stem — `bennu-be`, `rust-analyzer`, `java`. */
  name: string;
  kind: ProcessKind;
  /** The product it belongs to, inherited from the nearest backend ancestor — so a language
   *  server carries the icon of the product that started it. */
  product: string | null;
  /** Percent of **one** core, so a four-core machine can legitimately total 400. */
  cpu: number;
  /** What it costs in memory, in bytes, as the system counts it: the physical footprint on macOS,
   *  private bytes on Windows, resident size elsewhere. */
  memory: number;
  uptime_s: number;
}

/** The whole picture, in one round-trip. */
export interface ProcessReport {
  rows: ProcessRow[];
  total_cpu: number;
  total_memory: number;
  cores: number;
  /** Total physical memory, in bytes. */
  machine_memory: number;
  /** Whether the CPU figures are real yet — the first sample has nothing to compare against. */
  cpu_sampled: boolean;
}

/**
 * One thing a backend says it is holding in memory — a line of its `__memory` answer.
 *
 * **An estimate unless `exact`**: a backend sizes its own structures by walking them, and there is
 * no allocator keeping statistics to check that against. The screen shows the process's measured
 * total beside the items so the gap is visible.
 */
export interface MemoryItem {
  /** A project, repository or vault root, or `"process"` for what the backend holds once whatever is open. */
  scope: string;
  label: string;
  /** `null` when the structure is only counted — sizing some would cost more than it tells. */
  bytes: number | null;
  count: number | null;
  /** `bytes` is text actually held, summed — not an estimate. */
  exact: boolean;
  /** Memory-mapped files: in the resident total, but the system can take the pages back. */
  mapped: boolean;
}

/** A backend's whole breakdown. */
export interface MemoryReport {
  items: MemoryItem[];
}

/**
 * What `product`'s backend says it is holding, or `null` when it does not measure itself — which is
 * a different answer from an empty breakdown. Asked on demand, never on the sampling tick.
 * Wire: `process_memory`.
 */
export function processMemory(product: string): Promise<MemoryReport | null> {
  return platform<MemoryReport | null>('process_memory', { product });
}

/** Every process of Arbor's, with what it is costing. Wire: `process_report`. */
export function processReport(): Promise<ProcessReport> {
  return platform<ProcessReport>('process_report');
}
