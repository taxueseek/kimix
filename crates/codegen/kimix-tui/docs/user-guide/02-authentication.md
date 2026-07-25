# Authentication

Kimix supports several ways to authenticate, so you can use different model
providers without treating any single vendor as the product identity.

Credentials live in **`~/.kimix/auth.json`**. Config lives in
**`~/.kimix/config.toml`**. Kimix does not use `~/.kimix` as its home.

---

## Interactive Login

```bash
kimix login              # default interactive login for your primary provider
kimix login --xai        # native xAI device-code OIDC → ~/.kimix/auth.json
kimix logout             # clear the active cached session
```

Device-code login prints a URL and code; complete sign-in in the browser, then
return to the terminal. Tokens refresh when possible; when they cannot, Kimix
asks you to sign in again.

For Grok / xAI models configured with `auth_source = "xai_session"`, prefer
`kimix login --xai` so the session is owned by Kimix (not borrowed from another
CLI’s auth file).

See [Custom Models](11-custom-models.md) and
[provider profiles](../../../docs/provider-profiles.md) for model-side
`auth_source` settings.

---

## API Keys

For CI, automation, or headless environments, set the environment variable
named in your model’s `env_key` (or `api_key_env`) field in config.toml, for
example:

```bash
export LONGCAT_API_KEY="..."
export DEEPSEEK_API_KEY="..."
export XAI_API_KEY="..."
kimix
```

When both a session token and an API key are available for a model, the model
entry’s own credential resolution rules apply (BYOK / `env_key` usually wins
when set).

---

## Multi-Provider Setup

Kimix is multi-model. Typical pattern:

1. Put model entries under `[model.<name>]` in `~/.kimix/config.toml`
2. Point each at the right `base_url`, `api_backend`, and `env_key` / `auth_source`
3. Sign in or export keys as needed
4. Switch with `/model` or `kimix -m <name>`

List what the CLI currently sees:

```bash
kimix models
```

---

## Where Credentials Live

| Path | Role |
|------|------|
| `~/.kimix/auth.json` | Session tokens (login flows) |
| `~/.kimix/config.toml` | Model definitions, `env_key`, `auth_source` |
| Environment variables | API keys referenced by `env_key` |

Do not put secrets into git. Prefer env vars or the local auth store.

---

## Troubleshooting

| Symptom | What to try |
|---------|-------------|
| “No API key” / unauthorized | Check `kimix models`, `env_key`, and `echo $VAR` |
| xAI / Grok session missing | `kimix login --xai` |
| Wrong account | `kimix logout` then login again |
| Config not applied | Confirm edits are in `~/.kimix/config.toml`, not another product’s home |

For enterprise SSO / custom OIDC gateways, configure issuer and client id in
config or environment as documented in your deployment notes; the CLI still
stores resulting tokens under `~/.kimix/auth.json`.
