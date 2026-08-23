# Updater helper

The updater helper validates the staged `.app` bundle's identifier, Developer
ID signature, and Gatekeeper assessment before installation. It then swaps the
whole bundle by same-volume rename; it never edits the running bundle in place.

A durably fsynced journal makes every interruption recoverable. The old bundle
is retained as `.agent-factory-rollback.app`. A second install is rejected until
the application has health-checked the new version and explicitly cleans up or
rolls back that preserved version.

The binary accepts one bounded JSON install request on stdin and emits one JSON
result on stdout. Tests use an injected validator and fault checkpoints, so they
do not require signing credentials or modify an installed application.

