---
"@agent-factory/runtime-client": minor
"@agent-factory/web-ui": minor
---

Make the native Herdr workspace terminal a Draft-scoped visibility toggle that
can be used before, during, or after a Run. Starting a Run now publishes a
stable Run identity immediately, shows a spinner and cancellable pending state,
and opens the native terminal automatically. Cancelling closes every managed
Herdr session, hides the terminal, and restores Start Run without a reload.

Continuously reconcile live Herdr agent topology through events with a bounded
authoritative snapshot poll as a fallback, while retaining durable session
history under the Run that created it.
