import { describe, expect, it, vi } from 'vitest';

vi.mock('../rpc', () => ({ bennu: vi.fn(async () => undefined) }));

import { canDo, type LiveTargetInfo } from './brp';

const game = (protocol: string, capabilities: string[]): LiveTargetInfo => ({ kind: 'game', protocol, label: protocol, capabilities });

describe('canDo', () => {
  it('asks the capability, never the protocol name', () => {
    const dev = game('dev', ['screenshot', 'command']);
    const brp = game('brp', ['screenshot', 'ecs']);
    expect(canDo(dev, 'command')).toBe(true);
    expect(canDo(brp, 'command')).toBe(false);
    expect(canDo(brp, 'ecs')).toBe(true);
    expect(canDo(dev, 'ecs')).toBe(false);
  });

  it('says no for nothing attached', () => {
    expect(canDo(undefined, 'screenshot')).toBe(false);
  });

  it('keeps pausing a debugger thing', () => {
    const jvm: LiveTargetInfo = { kind: 'debugger', protocol: 'jdwp', label: 'JVM', capabilities: ['pause', 'stack'], session: 'run-1' };
    expect(canDo(jvm, 'pause')).toBe(true);
    expect(canDo(game('dev', ['screenshot']), 'pause')).toBe(false);
  });
});
