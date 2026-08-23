# plugin-runtime

Rust-owned Agent Plugins 1.0 loading, registry verification, installation, and
environment resolution for Agent Factory.

The crate deliberately has no MCP client and never launches a process. Its
output is a typed plan for the runtime's standard MCP transport layer. Local
stdio entries are always marked as requiring explicit trust. Plugin archives
are untrusted input until their catalog signature, artifact digest, extraction,
manifest, and registry identity have all been verified.

## Store layout

```text
<store>/
  .staging/                         # private temporary artifacts and extraction
  plugins/<name>/
    state.json                      # atomic active/previous version pointer
    versions/<sha256(version)>/     # immutable plugin package root
  data/<environment-id>/<plugin-name>/    # persistent PLUGIN_DATA
```

`PLUGIN_DATA` is environment-scoped and remains stable across plugin upgrades and
rollbacks. The package root is never writable through this API.

## Registry boundary

`verify_catalog` verifies the exact catalog bytes with a raw 32-byte Ed25519
public key before parsing them. It returns an opaque `VerifiedCatalog`.
`PluginStore::install_and_activate` accepts only an entry borrowed from that
verified value, so unsigned catalog DTOs cannot cross the install boundary.

`RegistryClient<HttpsRegistryDownloader>` is the production network boundary.
It requires HTTPS URLs without credentials or fragments, denies redirects, uses
a global timeout, bounds catalog/signature/artifact responses, verifies the
catalog before exposing an installable entry, and verifies the artifact before
activation. Tests may provide a `RegistryDownloader`; the client still enforces
URL and response-size checks around custom implementations.
