# ADR 0002: Next.js is a static WebView frontend

Status: accepted

## Decision

Use Next.js App Router with `output: "export"`. Native-SDK serves the generated
`apps/web-ui/out` tree from `zero://app` in production and the exact Portless
origin in development.

## Prohibited features

- API routes and route handlers
- Server Actions
- Middleware or proxy files
- Incremental static regeneration
- Dynamic request APIs
- A Node.js server in the packaged application

## Consequences

Every application operation crosses the typed native bridge. React cannot use a
server-only escape hatch to access files, processes, secrets, or persistence.
