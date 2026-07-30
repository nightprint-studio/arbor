/**
 * The engines' own functions — the one part of the intelligence that is *not* read
 * from a connection.
 *
 * Everything else in this folder derives from the catalogue the server reported.
 * This does not, and cannot: a function's meaning is a property of the engine, not
 * of the database, and `pg_proc` describes PostgreSQL's built-ins in a form
 * ("internal", one row per overload, no prose) that answers none of the questions
 * a person hovering `date_trunc` is actually asking. So the vocabulary is written
 * down here, per dialect, as data.
 *
 * That makes this a **maintained list**, and the honesty rule that governs the rest
 * of the folder applies with more force rather than less: an entry that is here is
 * a claim, and a wrong claim about `NVL2`'s argument order is worse than no entry
 * at all. When in doubt about a signature, leave the function out — completion
 * still offers it as a bare name through the keyword vocabulary.
 *
 * ## Why the two engines are separate lists rather than one with flags
 *
 * Because the overlap is smaller than it looks and the differences are the point.
 * `SUBSTR` counts from 1 on both and takes a length on both, but Oracle's negative
 * start counts from the end and PostgreSQL's does not. `TO_CHAR` shares a name and
 * not a format vocabulary. A merged table would need an exception on most rows,
 * and each exception would be a place to write the wrong engine's answer.
 *
 * The `category` is what the popup groups by and the hover leads with; the
 * `summary` is one sentence, because a hover nobody finishes reading is a hover
 * that cost a frame for nothing.
 */

import type { Dialect } from '$lib/types/picus';

/** What kind of thing a built-in is, for grouping and for the hover's first line. */
export type BuiltinCategory =
  | 'aggregate'
  | 'string'
  | 'number'
  | 'date'
  | 'conditional'
  | 'conversion'
  | 'window'
  | 'sequence'
  | 'system';

export interface BuiltinFunction {
  /** Name as it is written, in the engine's own case convention. */
  name: string;
  /** Full call shape, arguments named as the engine's documentation names them. */
  signature: string;
  /** What it returns, in the engine's type vocabulary. */
  returns: string;
  category: BuiltinCategory;
  /** One sentence. What it does, not how it is spelled. */
  summary: string;
  /** A worked example, when the shape is the part people get wrong. */
  example?: string;
  /** The trap, when there is one. This is the field worth reading. */
  note?: string;
  /** True for values written without parentheses — `SYSDATE`, not `SYSDATE()`. */
  bare?: boolean;
}

// ── Oracle ────────────────────────────────────────────────────────────────────

const ORACLE: BuiltinFunction[] = [
  // Aggregates
  { name: 'COUNT', signature: 'COUNT(expr | *)', returns: 'NUMBER', category: 'aggregate',
    summary: 'Rows in the group.',
    note: 'COUNT(col) skips NULLs; COUNT(*) does not. On a nullable column the two answer different questions.' },
  { name: 'SUM', signature: 'SUM(expr)', returns: 'NUMBER', category: 'aggregate',
    summary: 'Total of the non-NULL values.',
    note: 'NULL, not 0, when every value in the group is NULL.' },
  { name: 'AVG', signature: 'AVG(expr)', returns: 'NUMBER', category: 'aggregate',
    summary: 'Mean of the non-NULL values.',
    note: 'NULLs are excluded from the divisor as well as the sum — wrap in NVL first if they should count as zero.' },
  { name: 'MIN', signature: 'MIN(expr)', returns: 'same as expr', category: 'aggregate',
    summary: 'Smallest value in the group.' },
  { name: 'MAX', signature: 'MAX(expr)', returns: 'same as expr', category: 'aggregate',
    summary: 'Largest value in the group.' },
  { name: 'LISTAGG', signature: "LISTAGG(expr, separator) WITHIN GROUP (ORDER BY ...)", returns: 'VARCHAR2',
    category: 'aggregate', summary: 'Joins the values of a group into one string.',
    example: "LISTAGG(NOME, ', ') WITHIN GROUP (ORDER BY NOME)",
    note: 'Raises ORA-01489 past 4000 bytes unless the column is a CLOB.' },

  // Conditional / NULL handling
  { name: 'NVL', signature: 'NVL(expr, replacement)', returns: 'type of expr', category: 'conditional',
    summary: 'The second value when the first is NULL.',
    note: 'Both arguments are evaluated, so an expensive replacement costs even when it is not used. COALESCE short-circuits.' },
  { name: 'NVL2', signature: 'NVL2(expr, if_not_null, if_null)', returns: 'type of the branches',
    category: 'conditional', summary: 'Picks between two values by whether the first is NULL.',
    note: 'The order is the opposite of what most people expect: the NOT-NULL branch comes first.' },
  { name: 'COALESCE', signature: 'COALESCE(expr1, expr2, ...)', returns: 'type of the first non-NULL',
    category: 'conditional', summary: 'The first argument that is not NULL.',
    note: 'Short-circuits — unlike NVL, later arguments are not evaluated once one answers.' },
  { name: 'DECODE', signature: 'DECODE(expr, search1, result1, ..., default)', returns: 'type of the results',
    category: 'conditional', summary: 'Value-by-value mapping, like a simple CASE.',
    note: 'The one place in Oracle where NULL = NULL is true. CASE does not do that.' },
  { name: 'GREATEST', signature: 'GREATEST(expr1, expr2, ...)', returns: 'type of the arguments',
    category: 'conditional', summary: 'The largest of its arguments.',
    note: 'NULL if ANY argument is NULL — it is not a NULL-skipping MAX.' },
  { name: 'LEAST', signature: 'LEAST(expr1, expr2, ...)', returns: 'type of the arguments',
    category: 'conditional', summary: 'The smallest of its arguments.', note: 'NULL if any argument is NULL.' },

  // Strings
  { name: 'SUBSTR', signature: 'SUBSTR(string, start [, length])', returns: 'VARCHAR2', category: 'string',
    summary: 'Part of a string, counting from 1.',
    note: 'A negative start counts back from the end. Position 0 behaves as 1.' },
  { name: 'INSTR', signature: 'INSTR(string, substring [, start [, occurrence]])', returns: 'NUMBER',
    category: 'string', summary: 'Position of a substring, or 0 when it is not there.',
    note: '0 means absent — there is no NULL here, so a check must compare against 0.' },
  { name: 'LENGTH', signature: 'LENGTH(string)', returns: 'NUMBER', category: 'string',
    summary: 'Characters in a string.',
    note: 'Characters, not bytes. LENGTHB gives bytes, and on a multibyte charset they differ.' },
  { name: 'UPPER', signature: 'UPPER(string)', returns: 'VARCHAR2', category: 'string', summary: 'Upper case.' },
  { name: 'LOWER', signature: 'LOWER(string)', returns: 'VARCHAR2', category: 'string', summary: 'Lower case.' },
  { name: 'INITCAP', signature: 'INITCAP(string)', returns: 'VARCHAR2', category: 'string',
    summary: 'First letter of each word upper-cased, the rest lower.' },
  { name: 'TRIM', signature: 'TRIM([LEADING|TRAILING|BOTH] [char FROM] string)', returns: 'VARCHAR2',
    category: 'string', summary: 'Removes a character from the ends of a string.' },
  { name: 'LTRIM', signature: 'LTRIM(string [, chars])', returns: 'VARCHAR2', category: 'string',
    summary: 'Removes leading characters.',
    note: 'The second argument is a SET of characters, not a prefix — LTRIM(x, \'ab\') strips any run of a and b.' },
  { name: 'RTRIM', signature: 'RTRIM(string [, chars])', returns: 'VARCHAR2', category: 'string',
    summary: 'Removes trailing characters.', note: 'A set of characters, not a suffix.' },
  { name: 'LPAD', signature: 'LPAD(string, length [, pad])', returns: 'VARCHAR2', category: 'string',
    summary: 'Pads on the left to a given length.',
    note: 'TRUNCATES when the string is already longer than the length asked for.' },
  { name: 'RPAD', signature: 'RPAD(string, length [, pad])', returns: 'VARCHAR2', category: 'string',
    summary: 'Pads on the right to a given length.', note: 'Truncates a string that is already longer.' },
  { name: 'REPLACE', signature: 'REPLACE(string, search [, replacement])', returns: 'VARCHAR2',
    category: 'string', summary: 'Replaces every occurrence of a substring.',
    note: 'With no replacement it deletes. An empty-string search returns the input unchanged.' },
  { name: 'REGEXP_REPLACE', signature: 'REGEXP_REPLACE(string, pattern [, replacement [, start [, occurrence [, flags]]]])',
    returns: 'VARCHAR2', category: 'string', summary: 'Replaces what a regular expression matches.',
    example: "REGEXP_REPLACE(CODICE, '[^0-9]', '')" },
  { name: 'REGEXP_SUBSTR', signature: 'REGEXP_SUBSTR(string, pattern [, start [, occurrence [, flags [, group]]]])',
    returns: 'VARCHAR2', category: 'string', summary: 'The part of a string a regular expression matches.' },
  { name: 'REGEXP_LIKE', signature: 'REGEXP_LIKE(string, pattern [, flags])', returns: 'BOOLEAN',
    category: 'string', summary: 'Whether a regular expression matches.',
    note: 'A condition, not a value: it goes in WHERE, never in a SELECT list.' },

  // Numbers
  { name: 'ROUND', signature: 'ROUND(number [, decimals])', returns: 'NUMBER', category: 'number',
    summary: 'Rounds to a number of decimal places.',
    note: 'A negative second argument rounds to tens, hundreds and so on.' },
  { name: 'TRUNC', signature: 'TRUNC(number [, decimals]) | TRUNC(date [, format])', returns: 'NUMBER or DATE',
    category: 'number', summary: 'Cuts off rather than rounding — and on a DATE, cuts off the time.',
    example: "TRUNC(SYSDATE) -- midnight today",
    note: "TRUNC(date) is how you compare 'the same day' in Oracle, and forgetting it is why a BETWEEN on dates misses the last day." },
  { name: 'CEIL', signature: 'CEIL(number)', returns: 'NUMBER', category: 'number', summary: 'Rounds up.' },
  { name: 'FLOOR', signature: 'FLOOR(number)', returns: 'NUMBER', category: 'number', summary: 'Rounds down.' },
  { name: 'ABS', signature: 'ABS(number)', returns: 'NUMBER', category: 'number', summary: 'Absolute value.' },
  { name: 'MOD', signature: 'MOD(dividend, divisor)', returns: 'NUMBER', category: 'number',
    summary: 'Remainder of a division.',
    note: 'Follows the sign of the dividend, which differs from REMAINDER.' },

  // Dates
  { name: 'SYSDATE', signature: 'SYSDATE', returns: 'DATE', category: 'date', bare: true,
    summary: "The database server's current date and time.",
    note: 'No parentheses. It is the SERVER clock, not the client\'s, and it carries a time even though it is called a date.' },
  { name: 'SYSTIMESTAMP', signature: 'SYSTIMESTAMP', returns: 'TIMESTAMP WITH TIME ZONE', category: 'date',
    bare: true, summary: "The server's current timestamp, with fractional seconds and time zone." },
  { name: 'ADD_MONTHS', signature: 'ADD_MONTHS(date, months)', returns: 'DATE', category: 'date',
    summary: 'Moves a date by whole months.',
    note: 'Clamps to the end of the month: 31 January plus one month is 28 (or 29) February.' },
  { name: 'MONTHS_BETWEEN', signature: 'MONTHS_BETWEEN(later, earlier)', returns: 'NUMBER', category: 'date',
    summary: 'Months between two dates, fractional.',
    note: 'The later date comes FIRST. Reversing them gives a negative answer, silently.' },
  { name: 'LAST_DAY', signature: 'LAST_DAY(date)', returns: 'DATE', category: 'date',
    summary: 'The last day of that date\'s month.', note: 'Keeps the time of day it was given.' },
  { name: 'NEXT_DAY', signature: 'NEXT_DAY(date, weekday)', returns: 'DATE', category: 'date',
    summary: 'The next occurrence of a named weekday.',
    note: 'The weekday name depends on the session language — a script that hardcodes it is not portable between sessions.' },
  { name: 'EXTRACT', signature: 'EXTRACT(field FROM date)', returns: 'NUMBER', category: 'date',
    summary: 'One component of a date or interval.', example: 'EXTRACT(YEAR FROM DATA_ORDINE)' },

  // Conversion
  { name: 'TO_CHAR', signature: 'TO_CHAR(value [, format [, nls]])', returns: 'VARCHAR2', category: 'conversion',
    summary: 'Formats a date or a number as text.',
    example: "TO_CHAR(DATA, 'DD/MM/YYYY')",
    note: 'Without a format the session\'s NLS settings decide, so the same script gives different text on two machines.' },
  { name: 'TO_DATE', signature: 'TO_DATE(text [, format [, nls]])', returns: 'DATE', category: 'conversion',
    summary: 'Reads a date out of text.',
    example: "TO_DATE('31/12/2026', 'DD/MM/YYYY')",
    note: 'Always pass the format. Relying on the session default is how a script that works here fails there.' },
  { name: 'TO_NUMBER', signature: 'TO_NUMBER(text [, format [, nls]])', returns: 'NUMBER', category: 'conversion',
    summary: 'Reads a number out of text.',
    note: 'Raises on anything it cannot read — there is no silent NULL.' },
  { name: 'CAST', signature: 'CAST(expr AS type)', returns: 'the named type', category: 'conversion',
    summary: 'Converts a value to another type.' },

  // Window
  { name: 'ROW_NUMBER', signature: 'ROW_NUMBER() OVER ([PARTITION BY ...] ORDER BY ...)', returns: 'NUMBER',
    category: 'window', summary: 'Position within the window, always distinct.',
    note: 'Ties get different numbers, chosen arbitrarily. Use RANK when ties should share a number.' },
  { name: 'RANK', signature: 'RANK() OVER ([PARTITION BY ...] ORDER BY ...)', returns: 'NUMBER',
    category: 'window', summary: 'Rank within the window, with gaps after ties.' },
  { name: 'DENSE_RANK', signature: 'DENSE_RANK() OVER ([PARTITION BY ...] ORDER BY ...)', returns: 'NUMBER',
    category: 'window', summary: 'Rank within the window, without gaps after ties.' },
  { name: 'LAG', signature: 'LAG(expr [, offset [, default]]) OVER (ORDER BY ...)', returns: 'type of expr',
    category: 'window', summary: 'The value from an earlier row of the window.' },
  { name: 'LEAD', signature: 'LEAD(expr [, offset [, default]]) OVER (ORDER BY ...)', returns: 'type of expr',
    category: 'window', summary: 'The value from a later row of the window.' },

  // Sequences and session
  { name: 'NEXTVAL', signature: 'sequence.NEXTVAL', returns: 'NUMBER', category: 'sequence', bare: true,
    summary: 'Advances a sequence and returns the new value.',
    note: 'Advances even inside a transaction that is rolled back — sequence gaps are normal and not a fault.' },
  { name: 'CURRVAL', signature: 'sequence.CURRVAL', returns: 'NUMBER', category: 'sequence', bare: true,
    summary: 'The value this session last took from a sequence.',
    note: 'Raises ORA-08002 until NEXTVAL has been called in this session at least once.' },
  { name: 'USER', signature: 'USER', returns: 'VARCHAR2', category: 'system', bare: true,
    summary: 'The schema the session is connected as.' },
  { name: 'ROWNUM', signature: 'ROWNUM', returns: 'NUMBER', category: 'system', bare: true,
    summary: 'Row number as rows are produced, before ORDER BY.',
    note: 'Assigned BEFORE the sort, which is why `ROWNUM <= 10` with an ORDER BY does not give the top ten. Wrap the ordered query in a subquery.' },
];

// ── PostgreSQL ────────────────────────────────────────────────────────────────

const POSTGRES: BuiltinFunction[] = [
  // Aggregates
  { name: 'count', signature: 'count(expr | *)', returns: 'bigint', category: 'aggregate',
    summary: 'Rows in the group.', note: 'count(col) skips NULLs; count(*) does not.' },
  { name: 'sum', signature: 'sum(expr)', returns: 'numeric | bigint', category: 'aggregate',
    summary: 'Total of the non-NULL values.', note: 'NULL, not 0, on an empty group.' },
  { name: 'avg', signature: 'avg(expr)', returns: 'numeric', category: 'aggregate',
    summary: 'Mean of the non-NULL values.' },
  { name: 'min', signature: 'min(expr)', returns: 'same as expr', category: 'aggregate',
    summary: 'Smallest value in the group.' },
  { name: 'max', signature: 'max(expr)', returns: 'same as expr', category: 'aggregate',
    summary: 'Largest value in the group.' },
  { name: 'string_agg', signature: 'string_agg(expr, delimiter [ORDER BY ...])', returns: 'text',
    category: 'aggregate', summary: 'Joins the values of a group into one string.',
    example: "string_agg(nome, ', ' ORDER BY nome)" },
  { name: 'array_agg', signature: 'array_agg(expr [ORDER BY ...])', returns: 'anyarray',
    category: 'aggregate', summary: 'Collects the values of a group into an array.' },
  { name: 'json_agg', signature: 'json_agg(expr)', returns: 'json', category: 'aggregate',
    summary: 'Collects the values of a group into a JSON array.' },

  // Conditional
  { name: 'coalesce', signature: 'coalesce(expr1, expr2, ...)', returns: 'type of the first non-NULL',
    category: 'conditional', summary: 'The first argument that is not NULL.', note: 'Short-circuits.' },
  { name: 'nullif', signature: 'nullif(value, match)', returns: 'type of value', category: 'conditional',
    summary: 'NULL when the two are equal, the first value otherwise.',
    example: "nullif(divisore, 0) -- turns a division by zero into NULL" },
  { name: 'greatest', signature: 'greatest(expr1, expr2, ...)', returns: 'type of the arguments',
    category: 'conditional', summary: 'The largest of its arguments.',
    note: 'Unlike Oracle, PostgreSQL SKIPS NULLs here — the same call answers differently on the two engines.' },
  { name: 'least', signature: 'least(expr1, expr2, ...)', returns: 'type of the arguments',
    category: 'conditional', summary: 'The smallest of its arguments.', note: 'NULLs are skipped, not propagated.' },

  // Strings
  { name: 'substring', signature: 'substring(string FROM start FOR length)', returns: 'text', category: 'string',
    summary: 'Part of a string, counting from 1.',
    note: 'Also spelled substring(string, start, length). A negative start is not the end of the string here.' },
  { name: 'position', signature: 'position(substring IN string)', returns: 'integer', category: 'string',
    summary: 'Position of a substring, or 0 when it is not there.' },
  { name: 'length', signature: 'length(string)', returns: 'integer', category: 'string',
    summary: 'Characters in a string.', note: 'Characters. octet_length gives bytes.' },
  { name: 'upper', signature: 'upper(string)', returns: 'text', category: 'string', summary: 'Upper case.' },
  { name: 'lower', signature: 'lower(string)', returns: 'text', category: 'string', summary: 'Lower case.' },
  { name: 'initcap', signature: 'initcap(string)', returns: 'text', category: 'string',
    summary: 'First letter of each word upper-cased.' },
  { name: 'trim', signature: 'trim([LEADING|TRAILING|BOTH] [chars FROM] string)', returns: 'text',
    category: 'string', summary: 'Removes characters from the ends of a string.' },
  { name: 'lpad', signature: 'lpad(string, length [, fill])', returns: 'text', category: 'string',
    summary: 'Pads on the left to a given length.', note: 'Truncates a string that is already longer.' },
  { name: 'rpad', signature: 'rpad(string, length [, fill])', returns: 'text', category: 'string',
    summary: 'Pads on the right to a given length.', note: 'Truncates a string that is already longer.' },
  { name: 'replace', signature: 'replace(string, from, to)', returns: 'text', category: 'string',
    summary: 'Replaces every occurrence of a substring.',
    note: 'All three arguments are required here — unlike Oracle, there is no two-argument delete form.' },
  { name: 'split_part', signature: 'split_part(string, delimiter, n)', returns: 'text', category: 'string',
    summary: 'The nth field of a delimited string, counting from 1.',
    note: 'Empty string, not NULL, when there is no nth field.' },
  { name: 'concat_ws', signature: 'concat_ws(separator, expr1, expr2, ...)', returns: 'text', category: 'string',
    summary: 'Joins values with a separator, skipping NULLs.',
    note: 'The NULL-skipping is the reason to prefer it over `||`, which turns the whole expression NULL.' },
  { name: 'regexp_replace', signature: 'regexp_replace(string, pattern, replacement [, flags])', returns: 'text',
    category: 'string', summary: 'Replaces what a regular expression matches.',
    note: "Replaces only the FIRST match unless the 'g' flag is passed — the opposite of what replace() does." },
  { name: 'regexp_matches', signature: 'regexp_matches(string, pattern [, flags])', returns: 'setof text[]',
    category: 'string', summary: 'The capture groups a regular expression matched.',
    note: 'Returns a SET: used in a SELECT list it multiplies rows, and drops rows that did not match.' },

  // Numbers
  { name: 'round', signature: 'round(number [, decimals])', returns: 'numeric', category: 'number',
    summary: 'Rounds to a number of decimal places.',
    note: 'The two-argument form needs numeric — round(double, 2) does not exist and needs a cast.' },
  { name: 'trunc', signature: 'trunc(number [, decimals])', returns: 'numeric', category: 'number',
    summary: 'Cuts off rather than rounding.',
    note: 'Numbers only. For dates it is date_trunc, which is a different function with a different argument order.' },
  { name: 'ceil', signature: 'ceil(number)', returns: 'numeric', category: 'number', summary: 'Rounds up.' },
  { name: 'floor', signature: 'floor(number)', returns: 'numeric', category: 'number', summary: 'Rounds down.' },
  { name: 'abs', signature: 'abs(number)', returns: 'same as input', category: 'number', summary: 'Absolute value.' },
  { name: 'mod', signature: 'mod(dividend, divisor)', returns: 'same as input', category: 'number',
    summary: 'Remainder of a division.' },

  // Dates
  { name: 'now', signature: 'now()', returns: 'timestamptz', category: 'date',
    summary: 'The time the current transaction started.',
    note: 'The TRANSACTION\'s start, not the statement\'s: it does not move inside a transaction. clock_timestamp() does.' },
  { name: 'CURRENT_DATE', signature: 'CURRENT_DATE', returns: 'date', category: 'date', bare: true,
    summary: "Today's date, with no time.", note: 'No parentheses.' },
  { name: 'CURRENT_TIMESTAMP', signature: 'CURRENT_TIMESTAMP', returns: 'timestamptz', category: 'date',
    bare: true, summary: 'The time the current transaction started.', note: 'No parentheses. Same value as now().' },
  { name: 'date_trunc', signature: "date_trunc(field, timestamp)", returns: 'timestamp', category: 'date',
    summary: 'Cuts a timestamp down to a unit — the day, the month, the hour.',
    example: "date_trunc('day', creato_il)",
    note: 'The unit comes FIRST and is a string. This is the PostgreSQL answer to Oracle\'s TRUNC(date).' },
  { name: 'age', signature: 'age(later [, earlier])', returns: 'interval', category: 'date',
    summary: 'The interval between two timestamps.',
    note: 'With one argument it measures against the current transaction time, not against now.' },
  { name: 'extract', signature: 'extract(field FROM source)', returns: 'numeric', category: 'date',
    summary: 'One component of a timestamp or interval.', example: 'extract(year FROM data_ordine)' },
  { name: 'generate_series', signature: 'generate_series(start, stop [, step])', returns: 'setof',
    category: 'date', summary: 'Produces a row per value in a range — numbers, dates or timestamps.',
    example: "generate_series(date '2026-01-01', date '2026-12-01', interval '1 month')" },

  // Conversion
  { name: 'to_char', signature: 'to_char(value, format)', returns: 'text', category: 'conversion',
    summary: 'Formats a date or a number as text.', example: "to_char(data, 'DD/MM/YYYY')",
    note: 'The format is required here — there is no one-argument form as on Oracle.' },
  { name: 'to_date', signature: 'to_date(text, format)', returns: 'date', category: 'conversion',
    summary: 'Reads a date out of text.',
    note: 'Lenient: it accepts nonsense like month 13 and rolls it over rather than raising.' },
  { name: 'to_number', signature: 'to_number(text, format)', returns: 'numeric', category: 'conversion',
    summary: 'Reads a number out of text with an explicit format.' },
  { name: 'to_timestamp', signature: 'to_timestamp(text, format) | to_timestamp(epoch)', returns: 'timestamptz',
    category: 'conversion', summary: 'Reads a timestamp out of text, or out of a Unix epoch.' },
  { name: 'jsonb_build_object', signature: 'jsonb_build_object(key1, value1, ...)', returns: 'jsonb',
    category: 'conversion', summary: 'Builds a JSON object from alternating keys and values.' },

  // Window
  { name: 'row_number', signature: 'row_number() OVER ([PARTITION BY ...] ORDER BY ...)', returns: 'bigint',
    category: 'window', summary: 'Position within the window, always distinct.' },
  { name: 'rank', signature: 'rank() OVER ([PARTITION BY ...] ORDER BY ...)', returns: 'bigint',
    category: 'window', summary: 'Rank within the window, with gaps after ties.' },
  { name: 'dense_rank', signature: 'dense_rank() OVER ([PARTITION BY ...] ORDER BY ...)', returns: 'bigint',
    category: 'window', summary: 'Rank within the window, without gaps after ties.' },
  { name: 'lag', signature: 'lag(expr [, offset [, default]]) OVER (ORDER BY ...)', returns: 'type of expr',
    category: 'window', summary: 'The value from an earlier row of the window.' },
  { name: 'lead', signature: 'lead(expr [, offset [, default]]) OVER (ORDER BY ...)', returns: 'type of expr',
    category: 'window', summary: 'The value from a later row of the window.' },

  // Sequences
  { name: 'nextval', signature: "nextval('sequence')", returns: 'bigint', category: 'sequence',
    summary: 'Advances a sequence and returns the new value.',
    note: 'The sequence name is a STRING, not an identifier. Advances even in a transaction that rolls back.' },
  { name: 'currval', signature: "currval('sequence')", returns: 'bigint', category: 'sequence',
    summary: 'The value this session last took from a sequence.',
    note: 'Errors until nextval has been called in this session.' },
  { name: 'setval', signature: "setval('sequence', value [, is_called])", returns: 'bigint', category: 'sequence',
    summary: 'Sets a sequence to a value.',
    note: 'With is_called false the NEXT nextval returns the value given, not the one after it.' },
  { name: 'CURRENT_USER', signature: 'CURRENT_USER', returns: 'name', category: 'system', bare: true,
    summary: 'The role the session is executing as.', note: 'No parentheses.' },
];

const TABLE: Record<Dialect, BuiltinFunction[]> = { oracle: ORACLE, postgres: POSTGRES };

/** The whole vocabulary for one engine. */
export function builtinsFor(dialect: Dialect): BuiltinFunction[] {
  return TABLE[dialect];
}

/**
 * One built-in by name, case-insensitively.
 *
 * Case-insensitively because SQL folds it and the two engines write their own
 * conventions: a script that says `COALESCE` on PostgreSQL is asking about the
 * same function as `coalesce`, and a hover that appears for one spelling and not
 * the other reads as broken rather than as pedantic.
 */
export function builtinNamed(dialect: Dialect, name: string): BuiltinFunction | null {
  if (!name) return null;
  const upper = name.toUpperCase();
  return TABLE[dialect].find((f) => f.name.toUpperCase() === upper) ?? null;
}

/** What the hover shows under the title — the category, then the return type. */
export function builtinMeta(fn: BuiltinFunction): string[] {
  return [fn.category, `returns ${fn.returns}`];
}
