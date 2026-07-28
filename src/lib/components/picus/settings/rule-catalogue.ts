/**
 * The fourteen rules, in the words a person choosing whether to run them needs.
 *
 * This is deliberately **not** the backend's catalogue: `picus-analyze` owns the
 * closed set of ids and the severities, and it says them in the language of a
 * finding ("PARAMETRI is not touched by the PostgreSQL scripts"). What a settings
 * page needs is the other sentence — *what this rule is for, and when a team is
 * right to switch it off* — and inventing that at each call site is how two
 * screens end up describing the same rule differently.
 *
 * The ids are the contract: they are what `[analysis] disabled_rules` holds, what
 * a `-- picus: ignore CONS002 — …` comment names, and what the backend parses.
 * They do not get renamed.
 */

import type { RuleId, Severity } from '$lib/types/picus';

export interface RuleDescriptor {
  id: RuleId;
  /** What it looks for, in one line. */
  title: string;
  /** When a team is legitimately right to turn it off. Empty when there is no
   *  such case worth naming — which is most of them. */
  offWhen?: string;
  severity: Severity;
}

/** The families, in report order — the grouping the settings page renders. */
export interface RuleFamily {
  label: string;
  /** Why these belong together. */
  blurb: string;
  rules: RuleDescriptor[];
}

export const RULE_FAMILIES: RuleFamily[] = [
  {
    label: 'One dialect against the other',
    blurb:
      'The reason Picus exists: a change that landed in one engine’s scripts and not in the other’s.',
    rules: [
      {
        id: 'CONS001',
        title: 'An object one dialect changes and the other never does',
        offWhen:
          'Rarely — a table your scripts only read is already exempt, so views over tables another repository installs raise nothing.',
        severity: 'blocking',
      },
      {
        id: 'CONS004',
        title: 'The same table filled in differently in the two dialects',
        severity: 'blocking',
      },
    ],
  },
  {
    label: 'Installing against upgrading',
    blurb:
      'One dialect’s initialisation against its own updates. Which direction is even a question depends on what your initialisation folders are — see the model above.',
    rules: [
      {
        id: 'CONS002',
        title: 'A row the initialisation writes and no update ever writes',
        offWhen:
          'Your initialisation is kept at the latest version, so it holds first-release rows no update carries. Setting the model to “cumulative” switches this off for you.',
        severity: 'blocking',
      },
      {
        id: 'CONS003',
        title: 'A row an update writes and the initialisation never writes',
        offWhen:
          'Your initialisation is frozen at the first release and is not meant to receive later rows.',
        severity: 'blocking',
      },
    ],
  },
  {
    label: 'The version chain',
    blurb: 'Only ever in update folders. All three need the version table to be declared.',
    rules: [
      {
        id: 'VER001',
        title: 'An update script that writes without checking where it started from',
        offWhen:
          'Nothing, usually — if scripts for a second module guard against a second table, declare that table on the Version table page instead of switching this off.',
        severity: 'blocking',
      },
      {
        id: 'VER002',
        title: 'An update script that never carries the version forward',
        severity: 'blocking',
      },
      {
        id: 'VER003',
        title: 'A hole or an overlap in the chain of update files',
        offWhen:
          'Your update files are not named after the versions they install, so there is no chain to read.',
        severity: 'blocking',
      },
    ],
  },
  {
    label: 'The script itself',
    blurb: 'Facts about one file, independent of any comparison.',
    rules: [
      {
        id: 'DIA001',
        title: 'A statement written in the dialect the folder is not',
        severity: 'blocking',
      },
      {
        id: 'DUP001',
        title: 'The same row inserted twice in one script',
        offWhen:
          'Nothing in a normal repository — a DELETE or TRUNCATE of the table between the two INSERTs already excuses the second one.',
        severity: 'blocking',
      },
      {
        id: 'DUP002',
        title: 'The same object created twice in one half of the install story',
        offWhen:
          'Rarely — a CREATE OR REPLACE is already exempt, so a wrapper function every update script redefines does not reach this rule.',
        severity: 'review',
      },
      {
        id: 'DML001',
        title: 'A DELETE or an UPDATE with no WHERE clause',
        offWhen: 'Your install scripts reload whole tables by design.',
        severity: 'review',
      },
      {
        id: 'DML002',
        title: 'An INSERT with no column list',
        offWhen: 'The repository predates the convention and is not being converted.',
        severity: 'review',
      },
    ],
  },
  {
    label: 'Encoding',
    blurb: 'What the bytes are, against what the folder expects them to be.',
    rules: [
      {
        id: 'ENC001',
        title: 'A file whose encoding drifted from what its folder expects',
        offWhen: 'The repository mixes encodings knowingly and is not being converted.',
        severity: 'review',
      },
      {
        id: 'ENC002',
        title: 'A character the folder’s encoding cannot represent',
        severity: 'blocking',
      },
    ],
  },
];

/** Every rule, flat — for counting and for lookups. */
export const ALL_RULES: RuleDescriptor[] = RULE_FAMILIES.flatMap((f) => f.rules);
