# Platform secrets

`platform-secrets` is the only persistence boundary for credentials. Application
records store `SecretRef`; they never store or serialize `SecretValue`.

- Production macOS uses `MacOsKeychain` under the application bundle ID.
- Tests inject `InMemorySecretStore` and never touch the user's Keychain.
- Metadata listing returns opaque references, labels, and timestamps only.
- Secret buffers are redacted in `Debug` and zeroized on drop where practical.

The runtime should construct one store and pass it to environment/plugin services as a
trait object. UI contracts must never expose the `read` result.

