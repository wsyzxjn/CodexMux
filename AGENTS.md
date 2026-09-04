# CodexMux

Standalone, macOS-first local model router for Codex Desktop and Codex CLI.
It exposes an OpenAI Responses-compatible loopback endpoint, merges the live
official and CPA Codex catalogs, and routes each request by the exact `model`
value.

## Product boundary

- Keep this a small Rust service and CLI. Do not add Tauri, Electron, CDP,
  renderer injection, a plugin runtime, billing, failover, or a general API
  gateway.
- Official models use the canonical ChatGPT Codex Responses endpoint. CPA models
  normally use the configured CPA Responses endpoint. A user may explicitly
  route a `cpa/` slug to one native Responses endpoint (a direct route); direct
  routes declare their models and work with or without CPA installed. This is a
  narrow per-model override, not automatic failover or a general gateway.
- Aggregate the official and CPA native Codex catalogs dynamically. The
  persisted snapshot preserves upstream model metadata; only namespace CPA slugs
  and adjust display ordering.
- The served catalog view may carry narrow, reversible client-side adjustments
  that the snapshot does not: advertised search support, the optional `ultra`
  preset, and one shared compaction-compatibility hash. The shared `comp_hash`
  keeps Codex from forcing a pre-turn compaction onto the model a user is
  switching away from, and it tracks the default official model so a genuine
  upstream format change still rotates every model together.
- When CPA is unreachable, serve the official catalog merged with declared
  direct-route models. This degraded view is memory-only; the persisted
  snapshot still requires both upstream catalogs to validate.
- CodexMux never translates Chat Completions or Anthropic Messages; CPA owns
  external-provider conversion.
- Native Responses traffic is byte-preserving except when continuity requires
  `previous_response_id` to be replaced with replayable public history, or a CPA
  or direct request's local `cpa/` model slug must be restored to its upstream
  model id.
- Route by exact model slug from the last complete catalog snapshot, falling
  back to declared direct-route models when no snapshot exists. Official slugs
  are unchanged; CPA slugs use `cpa/`. Direct overrides apply only after that
  exact lookup succeeds. Unknown models fail closed.
- `codex-auto-review` may be explicitly pinned to a CPA model. Without an
  explicit override it stays on the official route; upstream failure never
  triggers an automatic route change.

## Security boundaries

- Bind loopback only and require the configured proxy token on every request.
- The CodexMux listener must be loopback-only; remote CPA and direct endpoints
  must use HTTPS. Plain HTTP is allowed only for loopback endpoints.
- Official ChatGPT OAuth may pass only to the fixed official Responses and model
  catalog endpoints.
- Persisted catalog state changes only after both upstream catalogs validate;
  persist the complete merged snapshot atomically and never install a partial
  refresh. The CPA-unavailable degraded view (official + declared direct
  models) is served from memory and never persisted.
- CPA must never receive the incoming Authorization header; inject its distinct
  token from the private credential file instead.
- Direct routes must never receive the incoming Authorization header or CPA
  token; inject the route's distinct token from the private routing file.
- Official, CPA, and direct credentials must never cross routes.
- Credential-bearing files use mode 0600. Credentials are never returned by an
  API, logged, or written into the model catalog or Codex config.
- Config enable/disable must be reversible: preserve exact original bytes,
  detect conflicts, and never overwrite user-modified files by guesswork.

## Continuity boundary

- Provider response ids are not portable across exact route/model identities.
- Every CPA and direct follow-up replays public history because the target may
  be stateless or may have changed. Official same-model follow-ups may keep the
  ID.
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
- Tests must assert that official OAuth, the CPA token, and direct-route tokens
  cannot cross routes.
- Do not modify a remote Git repository without the user's explicit approval.
