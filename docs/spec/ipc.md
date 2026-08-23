# Runtime IPC protocol v1

The Native-SDK host and Rust runtime communicate through one framed stdio
stream. Native forwards typed WebView requests without applying application
semantics.

## Framing

Each frame is:

1. Four-byte unsigned big-endian JSON payload length.
2. UTF-8 JSON payload of exactly that length.

`crates/ipc-contract` defines `MAX_FRAME_BYTES = 1 MiB` and `PROTOCOL_VERSION: u16 = 1`
(distinct from `crates/runtime-contract` `CONTRACT_VERSION: u32 = 1`). Frame
variants are `Request` / `Response` / `Event` / `Ready` / `Hello` / `Shutdown`.
Zero-length, oversized, truncated, non-UTF-8, and invalid JSON frames are
rejected. Runtime stdout is reserved for frames; logs go to stderr.

## Envelopes

Envelope `id` is a `Uuid` (not an opaque string). `version` is `PROTOCOL_VERSION`.

Request:

```json
{
  "kind": "request",
  "version": 1,
  "id": "123e4567-e89b-12d3-a456-426614174000",
  "method": "snapshot.get",
  "params": {}
}
```

Successful response:

```json
{
  "kind": "response",
  "version": 1,
  "id": "123e4567-e89b-12d3-a456-426614174000",
  "result": {}
}
```

Error response:

```json
{
  "kind": "response",
  "version": 1,
  "id": "123e4567-e89b-12d3-a456-426614174000",
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
both the JSON Schema and the TypeScript bindings into
`packages/shared/runtime-client`, so neither is hand-written.

Unknown methods return `method_not_found`. Invalid parameters return
`invalid_params`. A failed state precondition returns `conflict`. Internal
errors are redacted before crossing the bridge.
