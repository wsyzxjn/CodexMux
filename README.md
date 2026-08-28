# ModelMux

ModelMux is a small, macOS-first Responses router for Codex Desktop and Codex
CLI. Codex uses ModelMux as its single model provider. ModelMux fetches and merges
two native Codex model catalogs:

- the current account's official ChatGPT Codex catalog;
- CPA's (`router-for-me/CLIProxyAPI`) Codex catalog.

Official model slugs remain unchanged. CPA models receive a stable `cpa/`
namespace, so the official `gpt-5.6` and CPA's `cpa/gpt-5.6` can coexist in the
Codex model picker. Requests route by the exact selected slug.

ModelMux does not translate Chat Completions or Anthropic Messages. CPA owns all
external-provider protocol conversion and compatibility behavior. ModelMux stays
focused on catalog aggregation, credential isolation, exact routing, and safe
conversation handoff when the selected model changes.

## Build

```bash
cargo build --release
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
```

## Configure CPA

Run CPA on loopback and configure provider credentials, aliases, and models there.
CPA's top-level `api-keys` must include the same token stored as `cpa_token` in
ModelMux:

```yaml
host: 127.0.0.1
port: 8317
api-keys:
  - replace-with-a-private-cpa-token
```

Provider credentials remain in CPA. ModelMux never stores final provider API
keys.

## Configure ModelMux

Initialize the data directory:

```bash
cargo run -- init
```

On macOS it defaults to `~/Library/Application Support/ModelMux/`. Set
`MODELMUX_HOME` to use another root.

`config.toml` only configures the loopback services:

```toml
listen = "127.0.0.1:48682"

[cpa]
base_url = "http://127.0.0.1:8317/v1"
```

Models are not configured manually. CPA's live catalog is the source of truth.

Put only the two local access tokens in `credentials.json`; ModelMux enforces
mode 0600:

```json
{
  "proxy_token": "generated-by-modelmux-init",
  "cpa_token": "replace-with-the-token-in-cpa-api-keys"
}
```

`proxy_token` authenticates Codex to ModelMux. `cpa_token` authenticates ModelMux
to CPA. They must be different. `init` generates both random values; copy the
generated `cpa_token` into CPA or replace both sides with the same private value.

Enable ModelMux and run the service:

```bash
cargo run -- enable
cargo run -- serve
```

`enable` installs a reversible Codex provider block. It does not set
`model_catalog_json`; Codex fetches its catalog dynamically from ModelMux. Restart
Codex after enabling. Restore the exact prior Codex configuration with:

```bash
cargo run -- disable
```

## Dynamic model catalog

Codex requests:

```http
GET http://127.0.0.1:48682/v1/models?client_version=<Codex version>
```

ModelMux forwards the same `client_version` in parallel to:

```text
https://chatgpt.com/backend-api/codex/models
http://127.0.0.1:8317/v1/models
```

The official request receives the incoming ChatGPT OAuth and account headers.
The CPA request receives only the configured CPA bearer token. Credentials cannot
cross routes.

Both upstream responses must be valid native Codex catalogs before ModelMux
replaces its state. CPA model objects are preserved, including unknown future
fields; ModelMux only:

- changes `slug` from `<model>` to `cpa/<model>`;
- appends ` · CPA` to `display_name`;
- places CPA entries after official entries;
- excludes entries with `visibility = "hide"` or `supported_in_api = false`.

The complete merged catalog is atomically saved as `model-catalog.json`. It is
also the exact route table. A daemon restart restores it, and unknown model slugs
fail closed. If either catalog refresh fails, ModelMux serves the last complete
snapshot instead of installing a partial result. Before the first successful
refresh, model requests fail clearly because no route is known.

When forwarding a CPA request, only its top-level model identity changes:

```text
cpa/gpt-5.6 → gpt-5.6
```

The remaining native Responses request is unchanged unless conversation
continuity also requires public-history replay.

## macOS LaunchAgent

Place the compiled binary at a stable path, then run:

```bash
./modelmux install
./modelmux status
```

`install` records the current executable's absolute path. Move or replace the
binary before installing, not afterwards. Remove the service with
`./modelmux uninstall`; use `./modelmux disable` separately to restore Codex's
configuration.

## Switching models in one thread

Response IDs belong to the exact backend and model that issued them.

- A follow-up on the same official model keeps `previous_response_id` and is
  forwarded unchanged.
- Every CPA follow-up removes `previous_response_id` and replays locally recorded
  public history. CPA may translate Responses to a stateless Chat or Messages
  provider.
- Switching official models, switching CPA models, or switching between official
  and CPA also removes the ID and replays public history.

Only allow-listed text messages, tool calls/results, and public compaction
content are replayed. Images are deliberately dropped during a handoff because a
URL cannot be proven public without allowing DNS or redirect-based local access.
Reasoning, signatures, encrypted content, provider item IDs, statuses, and
unknown item types never cross routes.

The history store is bounded and in-memory. Restarting ModelMux removes it. An
unknown, ambiguous, evicted, incomplete, or non-completed response chain is
rejected rather than forwarding an ID to the wrong backend.

## Security

- ModelMux binds loopback only and requires `x-modelmux-token` on every route.
- CPA's configured endpoint must also be loopback.
- Official routes use only incoming Codex OAuth credentials and the fixed
  canonical ChatGPT Codex endpoint.
- CPA routes remove incoming authorization, cookies, account, organization,
  project, API-key, proxy-auth, and ModelMux-token headers before injecting the
  private CPA bearer token.
- Credentials remain in a separate mode-0600 file and are never written into the
  model catalog, Codex configuration, or API responses.
- `enable` refuses to overwrite user-owned Codex provider settings, and `disable`
  restores exact original bytes or removes only the unchanged managed block.

## Attribution

CPA (`router-for-me/CLIProxyAPI`) provides external-provider protocol conversion
and its native Codex model catalog. ModelMux does not include CPA source code.
