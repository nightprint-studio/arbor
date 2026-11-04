# imber-transport

The one place Imber touches the network: `HttpTransport` implements `imber_core::prelude::Transport` as a streamed
POST (connect timeout 20 s, 120 s of silence is a dead connection, no overall timeout), classifies failures
(`offline`, `timeout`, `stream_broke`, HTTP status) for the retry rules in `imber-core`, and drops the connection
on cancel so the provider stops billing.

`ARBOR_IMBER_FAKE_API_FAILURE=kind:count` (`offline:3`, `429:2`, `500:5`, `401:1`, …) simulates failures for the
first requests, to try the retry and error messages by hand.

Used by `bennu-be` and `corvus-be`; the credential is never held here — the host passes it per request.
