import { describe, expect, it } from 'vitest';
import { profileTargetOf } from './profiler-target';

const cfg = (over: Partial<{ kind: string; name: string; cargoCommand: string }> = {}) => ({
  kind: 'application',
  name: 'App',
  cargoCommand: '',
  ...over,
});
const jfr = (available: boolean | null = true, reason = '') => ({ available, reason });

describe('profileTargetOf', () => {
  it('needs a configuration', () => {
    expect(profileTargetOf(null, jfr()).ok).toBe(false);
  });

  it('records a JVM application or a Spring Boot one with Flight Recorder', () => {
    expect(profileTargetOf(cfg(), jfr())).toMatchObject({ ok: true, how: 'jfr' });
    expect(profileTargetOf(cfg({ kind: 'springboot' }), jfr())).toMatchObject({ ok: true, how: 'jfr' });
  });

  it('does not block on Flight Recorder before it has been checked, but does once it is known to be missing', () => {
    expect(profileTargetOf(cfg(), jfr(null)).ok).toBe(true);
    const t = profileTargetOf(cfg(), jfr(false, 'This JDK has no jdk.jfr module.'));
    expect(t.ok).toBe(false);
    expect(t.note).toContain('jdk.jfr');
  });

  it('records a Cargo run with samply, and nothing else Cargo does', () => {
    expect(profileTargetOf(cfg({ kind: 'cargo', cargoCommand: 'run' }), jfr())).toMatchObject({ ok: true, how: 'samply' });
    const t = profileTargetOf(cfg({ kind: 'cargo', cargoCommand: 'check' }), jfr());
    expect(t.ok).toBe(false);
    expect(t.note).toContain('cargo check');
  });

  it('refuses a script and a JUnit run, saying which', () => {
    expect(profileTargetOf(cfg({ kind: 'script' }), jfr())).toMatchObject({ ok: false });
    expect(profileTargetOf(cfg({ kind: 'junit' }), jfr()).note).toContain('JUnit');
  });
});
