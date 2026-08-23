# ADR 0004: Agent Plugins are Environment-scoped interoperability packages

Status: accepted

## Decision

Implement Agent Plugins 1.0.0 using locally embedded schemas. A plugin may
provide Skills and standard MCP server entries in their fixed specification
locations. Each Environment independently selects installed plugins and enabled
components; the Environment chosen for a launch determines that agent's plugin
plan.

Agent Factory does not expose standalone MCP or Skills management and does not
support plugin-hosted applications or WebView code.

## Security requirements

- Never retrieve a declared schema while loading a plugin.
- Resolve every package path within the canonical plugin root.
- Reject traversal, escaping symlinks, special files, and unsafe archives.
- Expand only `${PLUGIN_ROOT}` and `${PLUGIN_DATA}` in specification-approved
  fields, once and non-recursively.
- Ask for trust before enabling an executable stdio entry.
- Treat a broken component as a component failure rather than silently granting
  broader access or rejecting unrelated valid components.
