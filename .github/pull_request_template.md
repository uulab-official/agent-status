## What this changes

## Why

## Checklist
- [ ] `cargo test --workspace` passes
- [ ] `cargo build -p agent-status` passes
- [ ] If a provider changed: its README's confidence table is accurate and
      every `LimitWindow`/`CostSnapshot` sets an honest `Confidence`
      (see [docs/confidence.md](../docs/confidence.md))
- [ ] If scope changed vs. [ROADMAP.md](../ROADMAP.md), the roadmap is updated in this PR
- [ ] No credentials/tokens logged or persisted beyond what [SECURITY.md](../SECURITY.md) allows
