# Runtime IPC protocol v1

The Native-SDK host and Rust runtime communicate through one framed stdio
stream. Native forwards typed WebView requests without applying application
semantics.

## Framing

Each frame is:

1. Four-byte unsigned big-endian JSON payload length.
2. UTF-8 JSON payload of exactly that length.

The maximum payload is 1 MiB. Zero-length, oversized, truncated, non-UTF-8, and
invalid JSON frames are rejected. Runtime stdout is reserved for frames; logs
go to stderr.

## Envelopes

Request:

```json
{
  "kind": "request",
  "version": 1,
  "id": "opaque-request-id",
  "method": "snapshot.get",
  "params": {}
}
```

Successful response:

```json
{
  "kind": "response",
  "version": 1,
  "id": "opaque-request-id",
  "result": {}
}
```

Error response:

```json
{
  "kind": "response",
  "version": 1,
  "id": "opaque-request-id",
  "error": {
    "code": "invalid_params",
    "message": "Human-readable explanation"
  }
}
```

Event:

```json
{
  "kind": "event",
  "version": 1,
  "sequence": 1,
  "revision": 1,
  "topic": "project.changed",
  "payload": {}
}
```

Ready:

```json
{
  "kind": "ready",
  "version": 1,
  "runtime_name": "agent-factory-runtime",
  "runtime_version": "0.1.0"
}
```

## WebView bridge

Production JavaScript calls:

```ts
window.zero.invoke("runtime.invoke", request)
```

Only `zero://app` may call the production command. Development additionally
allows the exact loopback origin `http://127.0.0.1:3000`.

The frontend treats every response as untrusted boundary input. A WebView
reload, sequence gap, Rust restart, or invalid event triggers `snapshot.get`.

## Bootstrap methods

- `runtime.hello`
- `snapshot.get`
- `harness.list` — Herdr connectivity and the agent kinds it can launch
- `project.create`

`crates/runtime-contract` is the authority on the full method list; it generates
both the JSON Schema and the TypeScript bindings, so neither is hand-written.

Unknown methods return `method_not_found`. Invalid parameters return
`invalid_params`. A failed state precondition returns `conflict`. Internal
errors are redacted before crossing the bridge.
