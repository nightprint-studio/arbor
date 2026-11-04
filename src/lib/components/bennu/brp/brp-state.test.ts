import { describe, expect, it } from 'vitest';
import { currentStateName, isDestructiveTransition, isStateResource, parseStateTypes, pendingValue } from './brp-state';

describe('parseStateTypes', () => {
  it('pairs State<T> with NextState<T>', () => {
    const found = parseStateTypes([
      'bevy_state::state::resources::State<game::AppState>',
      'bevy_state::state::resources::NextState<game::AppState>',
      'bevy_state::state::resources::State<game::Paused>',
      'game::Score',
    ]);
    expect(found).toEqual([
      { enumPath: 'game::AppState', statePath: 'bevy_state::state::resources::State<game::AppState>', nextPath: 'bevy_state::state::resources::NextState<game::AppState>' },
      { enumPath: 'game::Paused', statePath: 'bevy_state::state::resources::State<game::Paused>', nextPath: null },
    ]);
  });

  it('ignores a NextState with no State beside it', () => {
    expect(parseStateTypes(['bevy_state::NextState<game::Lone>'])).toEqual([]);
  });

  it('keeps a generic enum whole', () => {
    expect(parseStateTypes(['bevy_state::State<game::Mode<u8>>'])[0].enumPath).toBe('game::Mode<u8>');
  });
});

describe('currentStateName', () => {
  it('reads the shapes a state value comes in', () => {
    expect(currentStateName('Playing')).toBe('Playing');
    expect(currentStateName({ Paused: { by: 1 } })).toBe('Paused');
    expect(currentStateName(['Playing'])).toBe('Playing');
    expect(currentStateName({ 0: 'Menu' })).toBe('Menu');
    expect(currentStateName({ 0: { Paused: {} } })).toBe('Paused');
  });

  it('says nothing when it cannot tell', () => {
    expect(currentStateName(null)).toBeNull();
    expect(currentStateName({ a: 1, b: 2 })).toBeNull();
    expect(currentStateName(3)).toBeNull();
  });
});

describe('transitions', () => {
  it('asks NextState for a pending variant', () => {
    expect(pendingValue('Paused')).toEqual({ Pending: 'Paused' });
  });

  it('flags the transitions that end a session', () => {
    for (const n of ['Quit', 'MainMenu', 'GameOver', 'ResetLevel', 'Exit']) expect(isDestructiveTransition(n)).toBe(true);
    for (const n of ['Playing', 'Paused', 'Loading']) expect(isDestructiveTransition(n)).toBe(false);
  });
});

describe('isStateResource', () => {
  it('recognises both halves of a state machine', () => {
    expect(isStateResource('bevy_state::State<game::AppState>')).toBe(true);
    expect(isStateResource('bevy_state::NextState<game::AppState>')).toBe(true);
    expect(isStateResource('game::Score')).toBe(false);
    expect(isStateResource('game::StateMachine')).toBe(false);
  });
});
