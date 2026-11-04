# hirundo-core

The headless state of Hirundo (`HirundoState`: event egress + the reverse channel to the shell) and the
`.hirundo/` **store**: collections, environments, environment types and local values as files in the project.

```
<project>/.hirundo/
  collections/<collection>/collection.toml     name, auth, shared headers, variables
                          /<request>.toml      one readable file per request
                          /<folder>/folder.toml, …
  environments/<id>.toml
  env-types.toml                               absent: local / dev / staging / prod
  local.toml (+ .gitignore)                    the active environment and this machine's variables
```

Every path the window sends is cleaned (`clean_rel`) before use: no `..`, drive letters or empty parts.
Writes go to a temp file renamed over the real one. A request file that does not parse is still listed (with its
error) so it can be opened and fixed.

`secrets` derives the keyring account of a `@secret` variable (`<project>|<layer>:<id>|<name>`) and asks the
shell for it over the reverse channel (`__hirundo_secret`); nothing secret is ever held or cached here.
