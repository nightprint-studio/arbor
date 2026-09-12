/**
 * The DTO Lab's test values — the rules that fill a field for its name or a constraint it carries — as
 * the settings page edits them.
 *
 * The backend owns the file and checks every save. This mirrors it and always saves whole lists, so the
 * order of the rules, which decides the one that answers, is written exactly as it is shown.
 */

import { dtoLabValueRules, saveDtoLabValueRules, type DtoLabValueRule } from '$lib/ipc/bennu/dtolab';
import { toastStore } from '$lib/feedback/stores/toasts.svelte';

function createBennuValueRulesStore() {
  let rules = $state<DtoLabValueRule[]>([]);
  let disabled = $state<string[]>([]);
  let builtins = $state<DtoLabValueRule[]>([]);
  let path = $state('');

  async function save(nextRules: DtoLabValueRule[], nextDisabled: string[]): Promise<boolean> {
    try {
      await saveDtoLabValueRules(nextRules, nextDisabled);
      rules = nextRules;
      disabled = nextDisabled;
      return true;
    } catch (e) {
      toastStore.show(String(e), 'error');
      return false;
    }
  }

  return {
    get rules() { return rules; },
    get disabled() { return disabled; },
    get builtins() { return builtins; },
    get path() { return path; },

    async load() {
      try {
        const answer = await dtoLabValueRules();
        rules = answer.rules;
        disabled = answer.disabled;
        builtins = answer.builtins;
        path = answer.path;
      } catch (e) {
        toastStore.show(`The test values could not be read: ${e}`, 'error');
      }
    },

    /** Add a rule, or replace the one called `previous`. */
    upsert(rule: DtoLabValueRule, previous: string | null): Promise<boolean> {
      const next = previous === null ? [...rules, rule] : rules.map((r) => (r.name === previous ? rule : r));
      return save(next, disabled);
    },

    remove(name: string): Promise<boolean> {
      return save(rules.filter((r) => r.name !== name), disabled);
    },

    /** Move a rule one place; an earlier one is tried first. */
    move(name: string, by: -1 | 1): Promise<boolean> {
      const from = rules.findIndex((r) => r.name === name);
      const to = from + by;
      if (from < 0 || to < 0 || to >= rules.length) return Promise.resolve(false);
      const next = [...rules];
      [next[from], next[to]] = [next[to], next[from]];
      return save(next, disabled);
    },

    setBuiltinEnabled(name: string, enabled: boolean): Promise<boolean> {
      const next = enabled ? disabled.filter((n) => n !== name) : [...disabled, name];
      return save(rules, next);
    },
  };
}

export const bennuValueRulesStore = createBennuValueRulesStore();
