# ModelMux

Standalone, macOS-first local model router for Codex Desktop and Codex CLI.
It exposes an OpenAI Responses-compatible loopback endpoint, merges custom
models into Codex's bundled catalog, and routes each request by the exact
`model` value.

## Product boundary

- Keep this a small Rust service and CLI. Do not add Tauri, Electron, CDP,
  renderer injection, a plugin runtime, billing, failover, or a general API
  gateway.
- Supported upstream dialects are a closed enum: native Responses, OpenAI Chat
  Completions, and Anthropic Messages.
- Native Responses traffic is byte-for-byte pass-through except when a
  cross-route `previous_response_id` must be replaced with replayable public
  history.
- Protocol translation lives only at the forwarder boundary. Never expose a
  generic adapter/plugin interface.
- Route by exact model slug. Unknown and ambiguous models fail closed.

## Security boundaries

- Bind loopback only and require the configured proxy token on every request.
- Official ChatGPT OAuth may pass only to an explicitly official provider.
- External providers must never receive the incoming Authorization header;
  inject their credential from the private credential file instead.
- Credentials live in a separate mode-0600 file and are never returned by an
  API, logged, or written into the model catalog or Codex config.
- Config enable/disable must be reversible: preserve exact original bytes,
  detect conflicts, and never overwrite user-modified files by guesswork.

## Continuity boundary

- Provider response ids are not portable. On a cross-route switch, replay only
  allow-listed public messages, tool calls/results, and compaction items, then
  remove `previous_response_id`.
- Never replay reasoning or encrypted/provider-private state.
- Unknown or incomplete chains are not rewritten; surface a clear error rather
  than silently sending an id to the wrong provider.

## Development

```bash
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
```

- Rust edition 2024. Prefer explicit data types and small modules over
  framework abstractions.
- Every wire-format change needs JSON and SSE fixtures, including fragmented
  UTF-8 and tool-call deltas.
- Tests must assert that OAuth and external credentials cannot cross routes.
- Do not modify a remote Git repository without the user's explicit approval.
