# bennu-lombok

What Lombok generates, **as data**. One catalogue of the annotations and one import gate, shared by
every part of Bennu that has to reason about members which exist in the compiled class and nowhere
in the source. Zero dependencies — names and import paths in, verdicts out.

## Why it is its own crate

Lombok moves declarations out of the `.java` file: `@Data` writes the getters, `@Builder` writes the
all-args constructor its builder calls, `@Slf4j` writes the `log` field. Any check that reads only
the source therefore has to know what is missing, or it reports a page of errors on a class that
compiles. Before this crate, five call sites each carried their own list — and they drifted: the
blank-final check knew `@Data` and `@Value` but not `@Builder`, so

```java
@Getter @Builder
public class QueryResult {
    private final String query;   // "Blank final field `query` is never initialized"
}
```

was reported as broken, while the index three crates away modelled `@Builder` correctly. The
knowledge is one thing; it belongs in one place.

The crate deliberately knows nothing about *how* a file was read, so both representations feed the
same functions: `bennu-check` maps tree-sitter `import_declaration` nodes onto `ImportPath` via
`parse_import_declaration`, `bennu-intel` maps its symbol-model `Import` onto it with a field copy.

## API

| Item | Answers |
|---|---|
| `generates_members(simple)` | does this annotation add members to the annotated type? A `true` means the type's member list is partly invisible — a reason to stay silent, never to report. |
| `generates_constructor(simple)` | does Lombok emit a constructor for it? `@NoArgsConstructor` / `@RequiredArgsConstructor` / `@AllArgsConstructor`, the bundles `@Data` / `@Value`, and `@Builder` / `@SuperBuilder`. |
| `initializes_blank_finals(simple, args)` | does that constructor **assign** the blank `final` fields? Narrower: `@NoArgsConstructor` qualifies only with `force = true`. |
| `is_inference_keyword(simple)` | is this `val` or `var`, which parse as ordinary type names? |
| `flag_is_true(args, key)` | is a boolean annotation argument set — `force = true`, `fluent=true`? |
| `ImportPath` / `parse_import_declaration(text)` | the borrowed import shape, and the parser from source text. |
| `file_uses_lombok(imports)` | the coarse gate: does the file import Lombok at all? |
| `annotation_is_lombok(ann, imports)` | the precise gate: does *this* annotation resolve to Lombok here? |
| `imports_keyword(keyword, imports)` | is `val` / `var` the inference keyword in this file, or a class of that name? |

## The import gate

Lombok only *does* anything when the annotation resolves to it, which requires the import — which in
turn can only compile with `org.projectlombok:lombok` on the classpath. So one test covers both "is
this really `@Data`" and "is Lombok even a dependency", and a project's own `@Data` in another
package correctly generates nothing and silences nothing.

## Public API: use the `prelude`

```rust
use bennu_lombok::prelude::*;
```
