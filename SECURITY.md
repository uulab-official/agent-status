# Security Policy

## Scope

Agent Status runs entirely on the user's machine and, by design, reads
sensitive local material to do its job:
- API keys from environment variables (`OPENAI_API_KEY`, `OPENROUTER_API_KEY`, `GEMINI_API_KEY`, ...)
- Local CLI config/state directories (`~/.claude`, `~/.cursor`, `gh` auth state)
- For scraped providers (planned): a logged-in browser session's cookies

None of this data is ever meant to leave the machine — there is no backend
(see [docs/architecture.md](docs/architecture.md) for why), and the SQLite
database (`crates/database`) is local-only. Any change that introduces
network calls sending this data anywhere other than the provider it came
from should be treated as a security regression, not a feature.

## Reporting a vulnerability

Please **do not** open a public issue for:
- Credential handling bugs (a key/token logged, persisted in plaintext
  where it shouldn't be, or sent somewhere unexpected)
- Auth-bypass or session-hijacking concerns in a scraping-based plugin
- Any path where user-supplied config (e.g. `provider-custom`'s `base_url`)
  could be leveraged for SSRF or command injection

Instead, report it privately via GitHub's "Report a vulnerability" flow
(Security tab → Advisories) on this repository, or contact the maintainers
directly. Include:
- Which crate/plugin is affected
- Steps to reproduce
- What data or capability is exposed

We'll acknowledge within a reasonable timeframe and coordinate a fix and
disclosure timeline with you.

## Handling of provider credentials in plugin code

If you're writing or reviewing a plugin (see
[docs/plugin-development.md](docs/plugin-development.md)):
- Never write an API key or session token to the SQLite database or to any
  log line, including `ProviderStatus::detail`.
- Prefer reading credentials from environment variables or the provider's
  own config file over asking the user to paste one into this app's config.
- **Never open and parse another tool's credential store directly** — e.g.
  `~/.codex/auth.json`, `~/.cursor`'s stored session, `~/.claude`'s auth
  state. Even read-only inspection of a token file's *structure* (which
  fields exist, whether a key is present) is a real risk to avoid, not just
  exfiltrating the secret value itself. This isn't hypothetical: an actual
  attempt to inspect `~/.codex/auth.json`'s internal shape while building
  `provider-codex` was correctly blocked by the coding agent's own safety
  classifier. The fix was architectural, not a workaround: shell out to the
  CLI's own sanctioned status command instead (`codex login status`,
  `cursor-agent status`, `gh auth token` for Copilot) and parse *that*
  command's stdout/stderr for a human-readable "logged in" signal. This is
  also just better engineering — it tracks whatever auth flow the vendor's
  CLI actually implements instead of a credential file's internal format,
  which can change between CLI versions without notice.
- For `provider-custom`-style config-driven plugins, treat `base_url` as
  untrusted input from the user's own machine — it's `UserInput` confidence
  for a reason (see [docs/confidence.md](docs/confidence.md)) — but it is
  *not* attacker-controlled remote input, so standard SSRF concerns are
  reduced to "the user pointed their own app at their own server," which is
  the intended use case.
