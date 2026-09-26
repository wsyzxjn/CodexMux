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
  use the configured CPA Responses endpoint. There is no other upstream.
- Aggregate the official and CPA native Codex catalogs dynamically. The
  persisted snapshot preserves upstream model metadata; only namespace CPA slugs
  and adjust display ordering.
- The served catalog view may carry narrow, reversible client-side adjustments
  that the snapshot never does: advertised search support, the optional `ultra`
  preset for models that already declare reasoning levels, local context-window
  overrides, and one shared compaction-compatibility hash. The shared `comp_hash`
  keeps Codex from forcing a pre-turn compaction onto the model a user is
  switching away from, and it tracks the default official model so a genuine
  upstream format change still rotates every model together.
- When CPA is unreachable, serve the official catalog alone. This degraded view
  is memory-only and never replaces the routes of the last complete view; its
  routes are installed only while no complete view exists, so official models
  work before CPA has ever answered.
- CodexMux never translates Chat Completions or Anthropic Messages; CPA owns
  external-provider conversion.
- Codex's built-in image tool posts to `/v1/images/generations` and
  `/v1/images/edits`. These carry a `gpt-image-*` model that no catalog lists
  and that is unrelated to the conversation model, so they are not routed by
  slug. They are byte-preserving passthrough to the fixed official endpoint
  unless a user explicitly pins an image model, exactly like `codex-auto-review`.
  Only the `model` field of a parsed JSON body is rewritten when pinned;
  upstream failure never triggers an automatic route change.
- The two image upstreams disagree about `model`, so never assume it selects
  anything. The official endpoint ignores it entirely — it requires only
  `prompt` and generates even when `model` is absent, bogus, or paired with a
  bogus `size` or `input_fidelity`, and it echoes no model back. CPA does
  dispatch on it and rejects a model it cannot serve. That asymmetry is why the
  slug is rewritten only when a pin moves a request off the official route.
- Native Responses traffic is byte-preserving except when continuity requires
  `previous_response_id` to be replaced with replayable public history, or a CPA
  request's local `cpa/` model slug must be restored to its upstream model id,
  or the opt-in shared web search backend injects search results and removes
  the `web_search` tool it owns. Shared search runs only for turns that end
  with new user text, and only after routing and continuity checks pass; the
  backend receives just the latest user message, never the context messages
  Codex injects, and the injected results stay out of recorded replay history.
  Results are inserted before that message as untrusted reference material. A
  backend failure does not fail the turn: a short note tells the model search
  was unavailable. Official targets and slugs a real probe verified keep their
  native `web_search` untouched; the shared backend serves only models without
  confirmed native search.
- Upstream response headers reach the client only through an allow-list
  (`x-codex-*`, `x-ratelimit-*`, `openai-*`, `x-request-id`, `retry-after`,
  `x-models-etag`). Upstream redirects are returned, never followed, so no
  credential follows a redirect.
- Route by exact model slug from the last complete catalog view, or from the
  official-only view before any complete view exists. Official slugs are
  unchanged; CPA slugs use `cpa/`. Unknown models fail closed.
- `codex-auto-review` may be explicitly pinned to a CPA model. Without an
  explicit override it stays on the official route; upstream failure never
  triggers an automatic route change.
- Image models are user-declared, not catalog-derived: CPA serves image models
  its `/v1/models` never lists, and neither catalog exposes any image model. A
  pinned image slug is validated by the upstream that receives it, and its
  error is returned unchanged. Any list CodexMux shows for the picker is
  best-effort UI metadata, never routing truth, and it describes the CPA route
  only; pinning cannot change what the official endpoint generates.

## Security boundaries

- Bind loopback only and require the configured proxy token on every request.
- The CodexMux listener must be loopback-only; a remote CPA endpoint must use
  HTTPS. Plain HTTP is allowed only for loopback endpoints.
- Official ChatGPT OAuth may pass only to the fixed official Responses, model
  catalog, and image generation/edit endpoints.
- Persisted catalog state changes only after both upstream catalogs validate;
  persist the complete merged snapshot atomically and never install a partial
  refresh. The CPA-unavailable official-only view is served from memory and
  never persisted.
- CPA must never receive the incoming Authorization header; inject its distinct
  token from the private credential file instead.
- Official and CPA credentials must never cross routes.
- Credential-bearing files use mode 0600. Credentials are never returned by an
  API, logged, or written into the model catalog or Codex config.
- Config enable/disable must be reversible: preserve exact original bytes,
  detect conflicts, and never overwrite user-modified files by guesswork. Lines
  another writer places inside the managed block are user content; a repair or
  restore proceeds only when the parsed result equals the user's document
  without the CodexMux keys.

## Continuity boundary

- Provider response ids are not portable across exact route/model identities.
- A completed turn is recorded only when the request stores it (`store` is not
  `false`) or continues a chain (`previous_response_id`); the store is memory
  only. A turn too large to keep is recorded as a route-only stub: same-route
  follow-ups still pass, replay to another route fails closed.
- Every CPA follow-up replays public history because the target may be
  stateless or may have changed. Official same-model follow-ups may keep the
  ID.
- Replay only allow-listed public text messages, tool calls/results, and public
  compaction content, then remove `previous_response_id`. Drop images from the
  replayed history; the current turn's own input is kept, minus reasoning and
  provider-private fields. Every replayed tool call keeps its output.
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
