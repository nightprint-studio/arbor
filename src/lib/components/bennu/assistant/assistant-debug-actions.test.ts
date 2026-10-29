import { describe, expect, it } from 'vitest';
import { actionsOf, withAction, type DebugAction } from './assistant-debug-actions';

const act = (session: string, text: string, at = 1): DebugAction => ({ session_id: session, conversation_id: 'c', text, at });

describe('assistant actions on the debugged program', () => {
  it('keeps the newest ones, in order', () => {
    let list: DebugAction[] = [];
    for (let i = 0; i < 25; i++) list = withAction(list, act('s', `a${i}`, i));
    expect(list).toHaveLength(20);
    expect(list[0].text).toBe('a5');
    expect(list.at(-1)?.text).toBe('a24');
  });

  it('can be narrowed to one session', () => {
    const list = [act('s1', 'x'), act('s2', 'y'), act('s1', 'z')];
    expect(actionsOf(list, 's1').map((a) => a.text)).toEqual(['x', 'z']);
  });
});
