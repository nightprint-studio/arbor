import { describe, expect, it } from 'vitest';
import { filterFlame, hiddenShare, type FlameLike } from './profiler-flame';

const node = (name: string, cls: string, value: number, children: FlameLike[] = []): FlameLike => ({
  name,
  value,
  self: value - children.reduce((s, c) => s + c.value, 0),
  meta: { kind: 'method', class: cls },
  children,
});

const root = (children: FlameLike[]): FlameLike => ({
  name: 'all',
  value: children.reduce((s, c) => s + c.value, 0),
  self: 0,
  meta: { kind: 'root', class: '' },
  children,
});

// all(100) → Tomcat(100) → Service(60) → Hikari(40) → Jdbc(40)   and   Tomcat → Other(30) → Service(10)
const tree = (): FlameLike =>
  root([
    node('Tomcat.run', 'org.tomcat.Tomcat', 100, [
      node('Service.load', 'it.acme.Service', 60, [node('Hikari.get', 'com.zaxxer.Hikari', 40, [node('Jdbc.read', 'org.pg.Jdbc', 40)])]),
      node('Other.go', 'it.acme.Other', 30, [node('Service.load', 'it.acme.Service', 10)]),
    ]),
  ]);

const mine = (cls: string) => cls.startsWith('it.acme');

describe('filterFlame', () => {
  it('removes the frames that do not pass and closes the stack around the gap', () => {
    const f = filterFlame(tree(), mine);
    expect(f.children!.map((c) => c.name)).toEqual(['Service.load', 'Other.go']);
    const service = f.children![0];
    // What it called is gone as boxes, but its time stays its own.
    expect(service.children).toEqual([]);
    expect(service.value).toBe(60);
    expect(service.self).toBe(60);
  });

  it('keeps a nested call when both ends pass', () => {
    const other = filterFlame(tree(), mine).children!.find((c) => c.name === 'Other.go')!;
    expect(other.children!.map((c) => c.name)).toEqual(['Service.load']);
    expect(other.self).toBe(20);
  });

  it('merges stacks that became the same once the frames between them and their parent were removed', () => {
    const t = root([
      node('A.go', 'org.a.A', 50, [node('Svc.x', 'it.acme.Svc', 20)]),
      node('B.go', 'org.b.B', 30, [node('Svc.x', 'it.acme.Svc', 10)]),
    ]);
    const f = filterFlame(t, mine);
    expect(f.children!.map((c) => [c.name, c.value])).toEqual([['Svc.x', 30]]);
  });

  it('keeps the root at the recording’s weight, with what was hidden as its remainder', () => {
    const f = filterFlame(tree(), (cls) => cls === 'it.acme.Service');
    expect(f.value).toBe(100);
    // Service is 60 under Tomcat plus 10 under Other: 70 survive, 30 are hidden.
    expect(f.children!.map((c) => c.value)).toEqual([70]);
    expect(hiddenShare(f)).toBeCloseTo(30);
  });

  it('never hides structure that has no class (the root, a thread)', () => {
    const t = root([{ name: 'main', value: 10, self: 0, meta: { kind: 'thread', class: '' }, children: [node('X.y', 'it.acme.X', 10)] }]);
    const f = filterFlame(t, mine);
    expect(f.children![0].name).toBe('main');
    expect(f.children![0].children![0].name).toBe('X.y');
  });
});
