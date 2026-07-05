---
name: New provider request
about: Ask for (or propose building) support for an AI tool not yet listed
title: "[Provider] "
labels: provider
---

**Provider / tool name**

**Does it have an official usage/limits API?**
Link the docs if so — this determines the target `Confidence` tier
(see [docs/confidence.md](../../docs/confidence.md)).

**If no official API, what's the next-best source?**
- [ ] CLI tool with local logs/state (★★★☆☆)
- [ ] Web dashboard usable via a logged-in session scrape (★★☆☆☆)
- [ ] None known

**Are you planning to implement this yourself?**
If yes, see [docs/plugin-development.md](../../docs/plugin-development.md)
and use `crates/providers/openrouter` (simple API) or
`crates/providers/custom` (config-driven, OpenAI-compatible) as your
starting template.
