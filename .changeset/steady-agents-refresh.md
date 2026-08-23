---
"agent-factory": patch
"@agent-factory/runtime-client": patch
"@agent-factory/web-ui": patch
---

Keep every native Draft window synchronized with authoritative Herdr snapshots,
recover through per-window polling when invalidations are missed, and group
persisted managed-session history beneath the Factory Run that created it.
