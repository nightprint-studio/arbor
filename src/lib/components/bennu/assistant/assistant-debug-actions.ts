/** The changes the assistant made to the program it debugs — kept short, newest last, for the Debug panel. */

export interface DebugAction {
  session_id: string;
  conversation_id: string;
  /** `` `x` was written: x = 1 → x = 2 `` — already masked by the backend's rules for secret-looking names. */
  text: string;
  at: number;
}

export const MAX_DEBUG_ACTIONS = 20;

export function withAction(list: DebugAction[], a: DebugAction, max = MAX_DEBUG_ACTIONS): DebugAction[] {
  return [...list, a].slice(-max);
}

/** The actions of one debug session. */
export function actionsOf(list: DebugAction[], sessionId: string): DebugAction[] {
  return list.filter((a) => a.session_id === sessionId);
}
