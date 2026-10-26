# bennu-mybatis

A MyBatis mapper XML, **coloured for what it is**.

## The idea

To the XML grammar a mapper is elements and attributes, and the part that matters — the SQL in a
statement, the bindings in it, the dynamic tags that decide which pieces of it exist — is one flat run
of text. A misspelled keyword, a `${}` that should have been a `#{}`, and a `test="…"` condition all read
exactly like the prose around them.

## What is coloured

| Kind | Spans |
|---|---|
| `mybatis.sql.keyword` / `.function` / `.string` / `.number` / `.comment` | The SQL of a `<select>`, `<insert>`, `<update>`, `<delete>`, `<sql>`, `<selectKey>` — also inside `CDATA` and inside the dynamic tags nested in it. |
| `mybatis.param` | `#{name}` — a bound parameter. |
| `mybatis.param.raw` | `${name}` — spliced into the statement text, the one that can be an injection, so it is drawn apart. Found inside a string literal too (`'%${name}%'`). |
| `mybatis.dynamic` | The name of `if`, `choose`, `when`, `otherwise`, `where`, `set`, `trim`, `foreach`, `bind`, `include`, on the opening and the closing tag. |
| `mybatis.expr` | An OGNL `test="…"`, a `<bind value="…">`, a `<foreach collection="…">`. |
| `mybatis.decl` | The `id` of a statement, `<sql>` fragment, `<resultMap>` or `<parameterMap>`. |
| `mybatis.ref` | `refid`, `resultMap`, `select`, and a `<resultMap extends>` — the references Ctrl+B follows. |
| `mybatis.type` | `parameterType`, `resultType`, `type`, `javaType`, `ofType`, `typeHandler` and the mapper `namespace`. |
| `mybatis.property` | The `property` of a result mapping (the Java side; `column` stays plain). |

## Notes

* A **closed keyword set**: `user`, `value`, `type`, `index` are columns somewhere. A word right after
  `from` / `join` / `into` / `update` / `table`, or after a `.`, is a name, never a keyword.
* A file is a mapper only with a `<mapper namespace="…">` root — `mybatis-config.xml` also has `<mapper>`
  elements (one resource pointer each) and is left alone.
* The markup scan is forgiving on purpose: the buffer is mid-edit, and a colouring pass that gave up on
  the first unclosed tag would blank the file mid-keystroke.
* Everything is a per-buffer question, so there is no project model; `reindex` only records that the host
  has been through once.
