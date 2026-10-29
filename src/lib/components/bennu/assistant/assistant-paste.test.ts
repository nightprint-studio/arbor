import { describe, expect, it } from 'vitest';
import { detectLanguage, languageFromPath, looksLikeCode, MAX_PASTE_CHARS, planPaste } from './assistant-paste';

const JAVA = `public class Foo {
    private final int x;

    public Foo(int x) {
        this.x = x;
    }
}`;
const RUST = `fn main() {
    let mut total = 0;
    for i in 0..3 { total += i; }
    println!("{}", total);
}`;
const PROSE = `I think the problem is that the build fails when the cache is stale.
Could you look at why it happens and tell me what you would change first?
Thanks a lot for the help with this one.`;

const plan = (pasted: string, extra: Partial<Parameters<typeof planPaste>[0]> = {}) =>
  planPaste({ pasted, before: '', after: '', activePath: null, enabled: true, ...extra });

describe('languageFromPath', () => {
  it('reads the extension', () => {
    expect(languageFromPath('C:\\p\\src\\Main.java')).toBe('java');
    expect(languageFromPath('/a/b/lib.rs')).toBe('rust');
    expect(languageFromPath('pom.xml')).toBe('xml');
  });
  it('says nothing for the unknown', () => {
    expect(languageFromPath('Makefile')).toBe('');
    expect(languageFromPath(null)).toBe('');
  });
});

describe('detectLanguage', () => {
  it('recognises the common ones', () => {
    expect(detectLanguage(JAVA)).toBe('java');
    expect(detectLanguage(RUST)).toBe('rust');
    expect(detectLanguage('{\n  "a": 1\n}')).toBe('json');
    expect(detectLanguage('<?xml version="1.0"?>\n<a><b/></a>')).toBe('xml');
    expect(detectLanguage('SELECT id, name\nFROM users\nWHERE id = 1;')).toBe('sql');
    expect(detectLanguage('server.port=8080\nspring.application.name=x')).toBe('properties');
    expect(detectLanguage('name: app\nversion: 1\nitems:\n  - a')).toBe('yaml');
    expect(detectLanguage('const a = (b) => b + 1;\nconsole.log(a(1));')).toBe('javascript');
    expect(detectLanguage('let a: number = 1;\ninterface Foo { x: string }')).toBe('typescript');
    expect(detectLanguage('$ mvn -q clean install')).toBe('bash');
  });
  it('is empty when nothing is distinctive', () => {
    expect(detectLanguage('hello world')).toBe('');
  });
});

describe('looksLikeCode', () => {
  it('accepts source', () => {
    expect(looksLikeCode(JAVA)).toBe(true);
    expect(looksLikeCode(RUST)).toBe(true);
    expect(looksLikeCode('{\n  "a": [1, 2]\n}')).toBe(true);
  });
  it('leaves prose, one-liners and lists alone', () => {
    expect(looksLikeCode(PROSE)).toBe(false);
    expect(looksLikeCode('let x = 1;')).toBe(false);
    expect(looksLikeCode('- first thing\n- second thing\n- third thing')).toBe(false);
  });
});

describe('planPaste', () => {
  it('wraps code in a fence with its language', () => {
    const p = plan(JAVA);
    expect(p).toMatchObject({ action: 'insert', language: 'java', truncated: false });
    if (p.action === 'insert') {
      expect(p.text.startsWith('```java\npublic class Foo')).toBe(true);
      expect(p.text.endsWith('}\n```\n')).toBe(true);
    }
  });

  it('puts blank lines around the block at the caret', () => {
    const p = planPaste({ pasted: RUST, before: 'Why does this fail?', after: 'Thanks', activePath: null, enabled: true });
    expect(p.action === 'insert' && p.text.startsWith('\n\n```rust\n')).toBe(true);
    expect(p.action === 'insert' && p.text.endsWith('```\n\n')).toBe(true);
  });

  it('falls back to the open file’s language', () => {
    const code = 'foo(a, b);\nbar(c);\nbaz(d);\n';
    const p = plan(code, { activePath: 'C:\\p\\Thing.java' });
    expect(p).toMatchObject({ action: 'insert', language: 'java' });
  });

  it('does nothing when off, for prose, or inside a fence', () => {
    expect(plan(JAVA, { enabled: false })).toEqual({ action: 'native' });
    expect(plan(PROSE)).toEqual({ action: 'native' });
    const before = 'see:\n```java\nint a = 1;\n';
    expect(planPaste({ pasted: JAVA, before, after: '\n```\n', activePath: null, enabled: true })).toEqual({ action: 'native' });
  });

  it('does not wrap text that holds a fence of its own', () => {
    expect(plan('```js\nconst a = 1;\n```\nmore {\n}')).toEqual({ action: 'native' });
  });

  it('cuts a very large paste at a line', () => {
    const big = Array.from({ length: 20_000 }, (_, i) => `    int v${i} = ${i};`).join('\n');
    const p = plan(big);
    expect(p.action === 'insert' && p.truncated).toBe(true);
    expect(p.action === 'insert' && p.text.length < MAX_PASTE_CHARS + 100).toBe(true);
  });
});
