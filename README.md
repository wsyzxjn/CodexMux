# ModelMux

ModelMux is a small local Responses router for Codex Desktop and Codex CLI. It
keeps Codex on one global `model_provider`, adds explicitly configured external
models to the bundled model catalog, and selects the real upstream from each
request's exact `model` value.

It is deliberately not a general LLM gateway. The only upstream dialects are:

- `responses`: native OpenAI Responses, passed through unchanged;
- `openai_chat`: Responses requests converted to Chat Completions and JSON/SSE
  responses converted back;
- `anthropic_messages`: Responses requests converted to Messages and JSON/SSE
  responses converted back.

## Status

The initial implementation includes loopback admission, official OAuth versus
external credential isolation, merged model catalogs, reversible Codex config
management, Chat and Messages translation, and in-memory cross-provider public
history replay. `responses/compact` is currently supported only by native
Responses providers; translated providers receive an explicit 501 response.

## Build

```bash
cargo build --release
cargo test --all-targets
cargo clippy --all-targets -- -D warnings
```

## Configure

Initialize the data directory:

```bash
cargo run -- init
```

On macOS it defaults to `~/Library/Application Support/ModelMux/`. Set
`MODELMUX_HOME` to use another root.

`config.toml` starts with the official provider. Add external providers using
unique model slugs:

```toml
schema_version = 1
listen = "127.0.0.1:48682"

[[providers]]
id = "openai"
name = "OpenAI"
kind = "official"
dialect = "responses"
base_url = "https://chatgpt.com/backend-api/codex"
enabled = true
allow_cross_model_previous_response_id = true

[[providers]]
id = "deepseek"
name = "DeepSeek"
kind = "external"
dialect = "openai_chat"
base_url = "https://api.deepseek.com/v1"
enabled = true
allow_cross_model_previous_response_id = false

[[providers.models]]
slug = "deepseek-chat"
display_name = "DeepSeek Chat"
context_window = 128000
accepts_images = false

[[providers]]
id = "anthropic"
name = "Anthropic"
kind = "external"
dialect = "anthropic_messages"
base_url = "https://api.anthropic.com/v1"
credential_header = "x-api-key"
headers = { anthropic-version = "2023-06-01" }
enabled = true
allow_cross_model_previous_response_id = false

[[providers.models]]
slug = "claude-custom"
display_name = "Claude Custom"
context_window = 200000
accepts_images = true
```

Put only secrets in `credentials.json`; ModelMux enforces mode 0600:

```json
{
  "schema_version": 1,
  "proxy_token": "generated-by-modelmux-init",
  "providers": {
    "deepseek": "provider-api-key",
    "anthropic": "provider-api-key"
  }
}
```

Generate the merged catalog and install the reversible block in Codex's
`config.toml`:

```bash
cargo run -- enable
cargo run -- serve
```

To run the compiled binary as a per-user macOS LaunchAgent, place it at a
stable path and run:

```bash
./modelmux install
./modelmux status
```

`install` records the current executable's absolute path. Move or replace the
binary before installing, not afterwards. Remove the agent with
`./modelmux uninstall`; this does not alter Codex's configuration, so use
`./modelmux disable` separately when you want to restore that file.

Restart Codex after changing the model catalog. To restore the original Codex
configuration:

```bash
cargo run -- disable
```

## Switching models in one thread

Response ids belong to the provider that issued them. When a request references
an id created on another route, ModelMux removes `previous_response_id` and
replays only public messages and supported tool call/result items. Reasoning and
encrypted provider state never cross providers. The store is currently bounded
and in-memory, so restarting ModelMux removes this switching history.

## Security

- The server refuses non-loopback listen addresses.
- Every Responses request requires `x-modelmux-token`.
- Official routes require and retain the incoming Codex OAuth header.
- External routes remove OAuth, cookies, account headers, and organization
  headers before injecting the provider's private credential.
- Credentials are separate from provider configuration and the generated model
  catalog.
- `enable` refuses to replace user-owned model provider settings, and `disable`
  restores exact original bytes or removes only the unchanged managed block.

## Attribution

The Anthropic Messages translator, continuity store, catalog shaping rules, and
reversible config manager were extracted from the ModelMux implementation in
CodexLoader and then reduced for this standalone project. CC Switch was studied
for its separation of request conversion, SSE state, and bounded history; no CC
Switch source code is included.
