# Update runtime

The update trust root is a configured Ed25519 public key and key ID. A detached
signature covers the exact bytes of a version 1 JSON manifest. Parsing and
target selection happen only after signature verification. Each selected
architecture-specific artifact then has its declared byte length and SHA-256
verified before a durable rename into a private staging directory.

The verified zip is extracted by Rust with fixed entry/expanded-size limits.
Extraction requires exactly one top-level `.app`, rejects traversal, symlinks,
special entries, duplicate or case-colliding paths, and validates the bundle
identifier, version, minimum macOS version, and thin Mach-O architecture against
the signed manifest before the updater helper can receive its path.

The manifest format accepts the existing release workflow's per-architecture
macOS zip artifacts. Checksums and SBOMs remain release-adjacent assets; the
manifest authenticates the install zip's URL, size, digest, bundle ID, minimum
macOS, channel, architecture, and version.

The version 1 client config is loaded only from an explicit path or an exact
packaged executable-relative path. Missing, malformed, oversized, or symlinked
config always produces a disabled client. Local packages seal the disabled
fixture without trust material. Release packages seal the stable key ID,
decoded 32-byte Ed25519 public key, expected bundle ID, and the moving
`releases/latest/download` manifest and signature URLs. The config requires
explicit user confirmation; it does not enable automatic installation or make
Native-SDK responsible for update behavior.

GitHub release URLs redirect to immutable tag and asset URLs. The update client
follows at most three redirects only when the initial URL is an exact GitHub
update-manifest or Agent Factory zip path. Every hop is revalidated as HTTPS,
credential-free, fragment-free, and restricted to GitHub's explicit release
asset hosts. Generic downloads remain redirect-deny.

## Release manifest signing

The publisher reads a base64-encoded 32-byte Ed25519 seed from stdin, never from
argv. It deterministically hashes the two architecture zips, sorts targets, and
writes the exact manifest bytes plus detached signature and audit public key.
The published public key is not a dynamic trust root; the application must pin
the corresponding key and key ID at build time.

```bash
printf '%s' "$UPDATE_MANIFEST_ED25519_SEED_BASE64" \
  | cargo run --locked -p update-runtime --bin sign-update-manifest -- \
      --version "$version" \
      --channel stable \
      --minimum-macos 13.0 \
      --bundle-id app.agentfactory.desktop \
      --key-id "$UPDATE_MANIFEST_KEY_ID" \
      --base-url "https://github.com/OWNER/REPO/releases/download/v$version" \
      --arm64 "dist/macos/$version/arm64/$arm64_zip" \
      --x86-64 "dist/macos/$version/x86_64/$x86_64_zip" \
      --output-dir "dist/macos/$version/update"
```

Required CI configuration:

- secret `UPDATE_MANIFEST_ED25519_SEED_BASE64` — offline-generated Ed25519 seed;
- non-secret variable `UPDATE_MANIFEST_KEY_ID` — stable identifier for rotation;
- non-secret variable `UPDATE_MANIFEST_PUBLIC_KEY_BASE64` — expected public key
  derived from that seed.

Publish all three generated files without modification:
`agent-factory-update-manifest-v1.json`, its `.sig`, and
`agent-factory-update-ed25519.pub`.
