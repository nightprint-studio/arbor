# bennu-generate

Bennu's code generators: a registry of **pure** functions `options + context -> files`.

No generator reads a file or the index. The backend (`bennu-be`, `code_generate.rs`) fills a
[`GenContext`](src/context.rs) — the project's dependencies and Java level, the class at the caret, the
Spring request mappings, the shapes of the types an option names — and writes the [`Output`](src/output.rs)
(new files, members for the open class, annotations added to existing code, snippets for other files,
notes). That is what keeps every generator testable with a hand-built context.

| Module | Role |
|---|---|
| `code` | indenting line buffer (`Lines`), import set (`Imports`), compilation unit (`JavaFile`, with the common JDK imports added automatically) |
| `names` | case conversion, type-text helpers (`payload_type`, `generic_args`, …) |
| `context` | `GenContext`, `ProjectInfo` (Spring Boot / Security versions, `jakarta` vs `javax`, Lombok, Java level), `ClassInfo`, `EndpointInfo` |
| `output` | `Output`, `GenFile`, `Members`, `Wiring`, option validation helpers |
| `wire` | text-based insertion of an annotation above a field or a type |
| `layout` | where a generated file lands (`src/main/java`, `src/test/java`, package folders) |
| `registry` | the `Generator` trait (`meta`, `defaults`, `generate`, `wanted_types`), `all()`, `find()`, `run()` |
| `generators/*` | one module per generator, each with its options struct and tests |

## Generators

`spring-security`, `attribute-converter`, `jackson-serde`, `http-client` (RestClient, WebClient, RestTemplate,
`@HttpExchange`, OpenFeign, `java.net.http`, OkHttp, Retrofit), `dto-mapper`, `builder`, `value-members`,
`test-skeleton`, `tauri-command`, `bevy-skeleton`, `rust-http-client` (reqwest async/blocking, ureq), and the ratatui set: `ratatui-app`, `ratatui-widget`, `ratatui-list`, `ratatui-keymap`, `ratatui-dialog`, and the web-service converters `soap-call` (SOAP envelope + WSDL → Java call, read with `bennu-xml`) and `cxf-rest` (JAX-RS client → modern Java or Rust client, XML kept).

## Adding one

1. A module in `generators/` with a unit struct implementing `Generator`; an options struct with
   `#[serde(default)]` and a `Default`.
2. Register it in `registry::all()`.
3. A modal in `src/lib/components/bennu/generate/` built on `GeneratorShell`, and its line in
   `GeneratorModals.svelte`. The palette, the editor menu and the folder menu pick it up from the registry.
