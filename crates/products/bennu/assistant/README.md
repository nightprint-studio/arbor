# bennu-assistant

The pure core of Bennu's assistant: **no network, no filesystem**. A host plugs in the HTTP transport,
the credential and the tools; everything here is a rule or a translation, and is tested as one.

| Module | What it holds |
|---|---|
| `message` | The neutral conversation: roles, blocks (text, tool use, tool result), tool specs, a request. |
| `provider` | The `Provider` interface (where to send, how to authenticate, the body, a stream `Decoder`) and the neutral `Event`s a response stream is turned into. |
| `anthropic` | The first provider: the Messages API body (cache-marked system and tools, adaptive thinking, effort) and its server-sent-event decoder. |
| `effort` | `Low … Max`, and which models have the setting. |
| `access` | The policy every path-taking tool asks first: scope (file, project, workspace, disk), edits on/off, Temp and extra folders, secrets, `.git`. Deleting always asks. |

## Adding a provider

Implement `provider::Provider` — endpoint, `auth_header`, `body(&Request)`, a `decoder()` that yields
`Event`s, and the `efforts` of a model — and register it where the host chooses one. Nothing else in the
assistant knows which provider it is talking to.

## Not here

The HTTP client, the API key (it lives in its own vault, read by the host), the tool implementations
(Bennu's refactorings, search, files) and the loop that joins them: those need the outside world and live
in the backend. This crate is what they are checked against.
