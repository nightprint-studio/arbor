import { describe, expect, it } from 'vitest';
import { activityLabel, formatElapsed } from './assistant-activity';

describe('what the assistant says it is doing', () => {
  it('names the phases', () => {
    expect(activityLabel({ phase: 'thinking', iteration: 1 })).toBe('Thinking');
    expect(activityLabel({ phase: 'writing', iteration: 2 })).toBe('Writing');
    expect(activityLabel({ phase: 'compacting', iteration: 1 })).toBe('Compacting the conversation');
  });

  it('names a tool by what it does, not by its identifier', () => {
    const tool = (name: string) => activityLabel({ phase: 'tool', tool: name, iteration: 1 });
    expect(tool('bennu_read_file')).toBe('Reading');
    expect(tool('search_text')).toBe('Searching');
    expect(tool('edit_file')).toBe('Editing');
    expect(tool('rename_symbol')).toBe('Planning a refactoring');
    expect(tool('bennu_find_usages')).toBe('Finding usages');
    expect(tool('bennu_list_problems')).toBe('Checking');
    expect(tool('ask_user')).toBe('Waiting for your answer');
    expect(tool('update_plan')).toBe('Updating the plan');
  });

  it('has a phrase for a tool it does not know', () => {
    expect(activityLabel({ phase: 'tool', tool: 'bennu_something_new', iteration: 1 })).toBe('Looking into the project');
  });
});

describe('formatElapsed', () => {
  it('shows minutes and padded seconds', () => {
    expect(formatElapsed(0)).toBe('0:00');
    expect(formatElapsed(7_400)).toBe('0:07');
    expect(formatElapsed(92_000)).toBe('1:32');
  });

  it('never goes negative', () => {
    expect(formatElapsed(-5)).toBe('0:00');
  });
});
