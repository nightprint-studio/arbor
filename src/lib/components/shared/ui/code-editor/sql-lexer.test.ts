/**
 * The shared SQL lexer, pinned on the shapes the stock legacy mode got wrong (or never had): a name
 * that collides with a keyword, calls vs types, parameters, and each engine's string rules.
 */

import { describe, expect, it } from 'vitest';
import { StringStream } from '@codemirror/language';
import { classifyWord, sqlStream } from './sql-lexer';
import type { SqlVocabularyDialect } from './sql-vocabulary';

/** `[text, token]` for every coloured piece of `source`, line by line, state carried across lines. */
function lex(dialect: SqlVocabularyDialect, source: string): [string, string][] {
  const parser = sqlStream(dialect);
  const state = parser.startState!(2);
  const out: [string, string][] = [];
  for (const line of source.split('\n')) {
    const stream = new StringStream(line, 2, 2);
    while (!stream.eol()) {
      stream.start = stream.pos;
      const token = parser.token(stream, state);
      if (stream.pos <= stream.start) stream.pos = stream.start + 1;
      if (token) out.push([stream.current(), token]);
    }
  }
  return out;
}

const kinds = (dialect: SqlVocabularyDialect, source: string, kind: string) =>
  lex(dialect, source).filter(([, k]) => k === kind).map(([t]) => t);

describe('keywords, names and positions', () => {
  it('colours statement words, not the project\'s own vocabulary after from/join', () => {
    expect(kinds('postgres', 'select a from orders o where o.total > 1', 'keyword')).toEqual(['select', 'from', 'where']);
    // `order` and `group` are legal table names; after `from`/`join` they are names, not keywords.
    expect(kinds('postgres', 'select * from order o join group g on g.id = o.g', 'keyword')).toEqual(['select', 'from', 'join', 'on']);
  });

  it('keeps a reserved clause word a keyword even where a name is expected', () => {
    expect(kinds('postgres', 'select * from (select 1) t where x', 'keyword')).toEqual(['select', 'from', 'select', 'where']);
  });

  it('reads a name after a dot as a property, whatever the word is', () => {
    expect(kinds('postgres', 'select o.total, o.order from t o', 'property')).toEqual(['total', 'order']);
  });

  it('follows a dotted table name to its last segment', () => {
    expect(kinds('postgres', 'select 1 from public.order x where 1', 'keyword')).toEqual(['select', 'from', 'where']);
  });
});

describe('calls, types and constants', () => {
  it('tells a call from a keyword and from a type', () => {
    expect(kinds('postgres', 'select count(*), coalesce(a, 0), my_func(x) from t', 'callee')).toEqual(['count', 'coalesce', 'my_func']);
    expect(kinds('postgres', 'create table t (id bigint, name varchar(20))', 'type')).toEqual(['bigint', 'varchar']);
    expect(kinds('postgres', 'select 1 where x in (1) and exists (select 1)', 'keyword')).toContain('in');
  });

  it('reads the type after a cast, including a call-shaped one', () => {
    expect(kinds('postgres', "select x::jsonb, y::varchar(10)", 'type')).toEqual(['jsonb', 'varchar']);
  });

  it('separates char(65) the call from `name char(10)` the type', () => {
    expect(classifyWord('char', { afterDot: false, afterCast: false, afterWord: false, nameMode: false, nameIsType: false, call: true }, 'postgres')).toBe('callee');
    expect(classifyWord('char', { afterDot: false, afterCast: false, afterWord: true, nameMode: false, nameIsType: false, call: true }, 'postgres')).toBe('type');
  });

  it('colours null/true/current_date as constants and rownum on Oracle', () => {
    expect(kinds('oracle', 'select * from t where rownum < 3 and a is null', 'atom')).toEqual(['rownum', 'null']);
  });
});

describe('parameters', () => {
  it('finds named, numbered and positional binds', () => {
    expect(kinds('postgres', 'where a = :min and b = $1 and c = ? and d = ?2', 'meta')).toEqual([':min', '$1', '?', '?2']);
  });

  it('does not take a cast for a parameter', () => {
    expect(kinds('postgres', 'select x::text', 'meta')).toEqual([]);
  });

  it('finds Oracle substitution variables', () => {
    expect(kinds('oracle', 'select * from &schema..t where a = &&val', 'meta')).toEqual(['&schema', '&&val']);
  });
});

describe('strings and comments', () => {
  it('treats `\'\'` as a quote inside the literal', () => {
    expect(kinds('postgres', "select 'L''Aquila', 2", 'string').join('')).toBe("'L''Aquila'");
    expect(kinds('postgres', "select 'L''Aquila', 2", 'number')).toEqual(['2']);
  });

  it('has no backslash escape in a plain literal, so a Windows path does not eat the file', () => {
    const tokens = lex('oracle', "select 'C:\\temp\\' from dual\nwhere 1 = 1");
    expect(tokens.filter(([, k]) => k === 'string')).toEqual([["'C:\\temp\\'", 'string']]);
    expect(tokens.some(([t, k]) => t === 'where' && k === 'keyword')).toBe(true);
  });

  it('honours the backslash of a PostgreSQL E\'…\' literal', () => {
    expect(kinds('postgres', "select E'it\\'s' , 1", 'string')).toEqual(["E'it\\'s'"]);
  });

  it('reads Oracle q-quoting', () => {
    expect(kinds('oracle', "select q'[it's [fine]]' from dual", 'string')).toEqual(["q'[it's [fine]]'"]);
  });

  it('keeps a multi-line literal and a multi-line comment open across lines', () => {
    expect(kinds('postgres', "select 'a\nb' , 1", 'string')).toEqual(["'a", "b'"]);
    expect(kinds('postgres', 'select 1 /* one\ntwo */ , 2', 'comment')).toEqual(['/* one', 'two */']);
  });

  it('nests block comments on PostgreSQL only', () => {
    expect(kinds('postgres', '/* a /* b */ still */ select', 'keyword')).toEqual(['select']);
    expect(kinds('oracle', '/* a /* b */ select', 'keyword')).toEqual(['select']);
  });

  it('reads an optimizer hint as code, not a comment', () => {
    expect(kinds('oracle', 'select /*+ index(t i) */ a from t', 'meta')).toEqual(['/*+ index(t i) */']);
  });

  it('colours a line comment to the end of the line', () => {
    expect(kinds('postgres', 'select 1 -- why\nfrom t', 'comment')).toEqual(['-- why']);
  });

  it('does not swallow a PL/pgSQL body as one string', () => {
    const body = lex('postgres', 'do $$\nbegin\n  raise notice \'x\';\nend\n$$;');
    expect(body.filter(([, k]) => k === 'meta').map(([t]) => t)).toEqual(['$$', '$$']);
    expect(body.some(([t, k]) => t === 'begin' && k === 'keyword')).toBe(true);
  });
});

describe('JPQL', () => {
  it('colours JPQL words and leaves an entity called Order alone', () => {
    expect(kinds('jpql', 'select o from Order o where o.total > :min order by o.id', 'keyword'))
      .toEqual(['select', 'from', 'where', 'order', 'by']);
    expect(kinds('jpql', 'select new com.acme.Dto(o.id) from Order o', 'type')).toEqual(['com', 'acme', 'Dto']);
  });
});
