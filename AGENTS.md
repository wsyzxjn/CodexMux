# ModelMux

Standalone, macOS-first local model router for Codex Desktop and Codex CLI.
It exposes an OpenAI Responses-compatible loopback endpoint, merges the live
official and CPA Codex catalogs, and routes each request by the exact `model`
value.

## Product boundary

- Keep this a small Rust service and CLI. Do not add Tauri, Electron, CDP,
  renderer injection, a plugin runtime, billing, failover, or a general API
  gateway.
- Official models use the canonical ChatGPT Codex Responses endpoint. CPA models
  use one loopback CPA Responses endpoint.
- Aggregate the official and CPA native Codex catalogs dynamically. Preserve
  upstream model metadata; only namespace CPA slugs and adjust display ordering.
- ModelMux never translates Chat Completions or Anthropic Messages; CPA owns
  external-provider conversion.
- Native Responses traffic is byte-preserving except when continuity requires
  `previous_response_id` to be replaced with replayable public history, or a CPA
  request's local `cpa/` model slug must be restored to its upstream model id.
- Route by exact model slug from the last complete catalog snapshot. Official
  slugs are unchanged; CPA slugs use `cpa/`. Unknown models fail closed.

## Security boundaries

- Bind loopback only and require the configured proxy token on every request.
- Require the configured CPA endpoint to use a loopback host.
- Official ChatGPT OAuth may pass only to the fixed official Responses and model
  catalog endpoints.
- Catalog state changes only after both upstream catalogs validate; persist the
  complete merged snapshot atomically and never install a partial refresh.
- CPA must never receive the incoming Authorization header; inject its distinct
  token from the private credential file instead.
- The CPA token must never reach the official route.
- Credentials live in a separate mode-0600 file and are never returned by an
  API, logged, or written into the model catalog or Codex config.
- Config enable/disable must be reversible: preserve exact original bytes,
  detect conflicts, and never overwrite user-modified files by guesswork.

## Continuity boundary

- Provider response ids are not portable across exact route/model identities.
- Every CPA follow-up replays public history because CPA may route to a stateless
  translated provider. Official same-model follow-ups may keep the ID.
- Replay only allow-listed public text messages, tool calls/results, and public
  compaction content, then remove `previous_response_id`. Drop images on handoff.
- Never replay reasoning, signatures, encrypted state, or provider-private
  fields.
- Unknown, ambiguous, non-completed, or incomplete chains are not rewritten;
  surface a clear error rather than silently sending an id to the wrong route.

## Development

```bash
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
```

- Rust edition 2024. Prefer explicit data types and small modules over framework
  abstractions.
- Every wire-format change needs JSON and SSE fixtures, including fragmented
  UTF-8 and tool-call deltas.
- Tests must assert that official OAuth and the CPA token cannot cross routes.
- Do not modify a remote Git repository without the user's explicit approval.
