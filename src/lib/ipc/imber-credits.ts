/**
 * The credit meter's data: what each provider Imber can use has left. Served by `bennu-be` (the ledger and the
 * limits live there), so a window that is not Bennu's — Corvus — borrows that backend first.
 */
import { bennu, ensureBackend } from './rpc';
import type { PeriodStarts } from '$lib/components/bennu/assistant/assistant-periods';

export interface CreditLine { label: string; value: string }

export interface CreditSection {
  title: string;
  lines: CreditLine[];
  note?: string;
  /** Something the user can do about it: `add_admin_key`. */
  action?: string;
}

export interface ProviderCredit {
  id: string;
  name: string;
  active: boolean;
  subtitle: string;
  figure: string;
  /** Share left, 0–1, when a limit is known. */
  left: number | null;
  sections: CreditSection[];
}

export interface CreditSummary {
  providers: ProviderCredit[];
  /** Average share left over the providers with a known limit. */
  average_left: number | null;
  /** `[provider id, share left]` of the provider with the least left. */
  lowest: [string, number] | null;
  level: 'ok' | 'warn' | 'out';
}

/** `refresh` asks the provider again instead of reusing the figure read in the last five minutes. */
export async function getImberCredits(periods: PeriodStarts, refresh = false): Promise<CreditSummary> {
  await ensureBackend('bennu');
  return bennu<CreditSummary>('imber_credits', { periods, refresh });
}
