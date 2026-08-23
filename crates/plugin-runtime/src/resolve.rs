use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use crate::error::{PluginError, Result};
use crate::model::{
    Diagnostic, DiagnosticBoundary, EnvironmentPluginSelection, ExecutableTrustClass, McpComponent,
    McpServerDefinition, ResolvedEnvironmentPlugins, ResolvedMcpServer, ResolvedSkill,
};
use crate::path::{canonical_directory, contained_path, normalize_relative, validate_identifier};
use crate::store::PluginStore;

impl PluginStore {
    pub fn resolve_environment_plugins(
        &self,
        selection: &EnvironmentPluginSelection,
    ) -> Result<ResolvedEnvironmentPlugins> {
        validate_identifier(&selection.environment_id, "environment id")?;
        validate_selection(selection)?;
        let mut mcp_servers = Vec::new();
        let mut default_skills = Vec::new();
        let mut diagnostics = Vec::new();
        let mut plugins = selection.plugins.iter().collect::<Vec<_>>();
        plugins.sort_by(|left, right| left.name.cmp(&right.name));

        for entry in plugins {
            let plugin = self.active_plugin(&entry.name)?;
            let plugin_data = self.plugin_data_directory(&selection.environment_id, &entry.name)?;
            let requested_mcp = entry
                .enabled_mcp_servers
                .as_ref()
                .map(|names| names.iter().cloned().collect::<BTreeSet<_>>());
            match &plugin.mcp {
                McpComponent::Absent
                    if requested_mcp.as_ref().is_some_and(|set| !set.is_empty()) =>
                {
                    return Err(PluginError::InvalidEnvironmentSelection(format!(
                        "plugin {:?} has no MCP component",
                        entry.name
                    )));
                }
                McpComponent::Disabled { reason }
                    if requested_mcp.as_ref().is_none_or(|set| !set.is_empty()) =>
                {
                    diagnostics.push(Diagnostic {
                        boundary: DiagnosticBoundary::Component(format!("{}:mcp", entry.name)),
                        code: "mcpDisabled".into(),
                        message: reason.clone(),
                    });
                }
                McpComponent::Loaded(servers) => {
                    if let Some(requested) = &requested_mcp {
                        let available = servers
                            .iter()
                            .map(McpServerDefinition::name)
                            .collect::<BTreeSet<_>>();
                        if let Some(name) = requested
                            .iter()
                            .find(|name| !available.contains(name.as_str()))
                        {
                            return Err(PluginError::InvalidEnvironmentSelection(format!(
                                "plugin {:?} has no valid MCP server {name:?}",
                                entry.name
                            )));
                        }
                    }
                    for server in servers {
                        if requested_mcp
                            .as_ref()
                            .is_some_and(|requested| !requested.contains(server.name()))
                        {
                            continue;
                        }
                        mcp_servers.push(resolve_server(
                            &entry.name,
                            &plugin.root,
                            &plugin_data,
                            server,
                        )?);
                    }
                }
                McpComponent::Absent | McpComponent::Disabled { .. } => {}
            }

            let available_skills = plugin
                .skills
                .iter()
                .map(|skill| (skill.name.as_str(), skill))
                .collect::<BTreeMap<_, _>>();
            for skill_name in &entry.default_skills {
                let skill = available_skills.get(skill_name.as_str()).ok_or_else(|| {
                    PluginError::InvalidEnvironmentSelection(format!(
                        "plugin {:?} has no valid skill {skill_name:?}",
                        entry.name
                    ))
                })?;
                default_skills.push(ResolvedSkill {
                    plugin_name: entry.name.clone(),
                    name: skill.name.clone(),
                    description: skill.description.clone(),
                    skill_file: skill.skill_file.clone(),
                });
            }
        }
        Ok(ResolvedEnvironmentPlugins {
            environment_id: selection.environment_id.clone(),
            mcp_servers,
            default_skills,
            diagnostics,
        })
    }
}

fn validate_selection(selection: &EnvironmentPluginSelection) -> Result<()> {
    let mut plugin_names = BTreeSet::new();
    for plugin in &selection.plugins {
        validate_identifier(&plugin.name, "plugin name")?;
        if !plugin_names.insert(&plugin.name) {
            return Err(PluginError::InvalidEnvironmentSelection(format!(
                "duplicate plugin {:?}",
                plugin.name
            )));
        }
        if let Some(servers) = &plugin.enabled_mcp_servers {
            reject_duplicates(servers, "MCP server", &plugin.name)?;
        }
        reject_duplicates(&plugin.default_skills, "default skill", &plugin.name)?;
    }
    Ok(())
}

fn reject_duplicates(values: &[String], label: &str, plugin_name: &str) -> Result<()> {
    let mut unique = BTreeSet::new();
    for value in values {
        if value.is_empty() || !unique.insert(value) {
            return Err(PluginError::InvalidEnvironmentSelection(format!(
                "empty or duplicate {label} {value:?} for plugin {plugin_name:?}"
            )));
        }
    }
    Ok(())
}

fn resolve_server(
    plugin_name: &str,
    plugin_root: &Path,
    plugin_data: &Path,
    server: &McpServerDefinition,
) -> Result<ResolvedMcpServer> {
    match server {
        McpServerDefinition::Stdio {
            name,
            command,
            args,
            env,
            cwd,
        } => {
            let (command, trust_class) = if let Some(relative) = command.strip_prefix("./") {
                (
                    contained_path(plugin_root, Path::new(relative))?,
                    ExecutableTrustClass::BundledExecutable,
                )
            } else {
                (PathBuf::from(command), ExecutableTrustClass::PathExecutable)
            };
            let plugin_root = canonical_directory(plugin_root)?;
            let plugin_data = canonical_directory(plugin_data)?;
            let root_text = path_text(&plugin_root)?;
            let data_text = path_text(&plugin_data)?;
            let args = args
                .iter()
                .map(|value| expand_placeholders(value, root_text, data_text))
                .collect();
            let mut env = env
                .iter()
                .map(|(key, value)| {
                    (
                        key.clone(),
                        expand_placeholders(value, root_text, data_text),
                    )
                })
                .collect::<BTreeMap<_, _>>();
            env.insert("PLUGIN_ROOT".into(), root_text.into());
            env.insert("PLUGIN_DATA".into(), data_text.into());
            let cwd = resolve_cwd(
                cwd.as_deref(),
                &plugin_root,
                &plugin_data,
                root_text,
                data_text,
            )?;
            Ok(ResolvedMcpServer::Stdio {
                plugin_name: plugin_name.into(),
                name: name.clone(),
                command,
                args,
                env,
                cwd,
                trust_class,
                requires_explicit_trust: true,
            })
        }
        McpServerDefinition::StreamableHttp { name, url, headers } => {
            Ok(ResolvedMcpServer::StreamableHttp {
                plugin_name: plugin_name.into(),
                name: name.clone(),
                url: url.clone(),
                headers: headers.clone(),
                trust_class: ExecutableTrustClass::NoLocalExecution,
            })
        }
        McpServerDefinition::Sse { name, url, headers } => Ok(ResolvedMcpServer::Sse {
            plugin_name: plugin_name.into(),
            name: name.clone(),
            url: url.clone(),
            headers: headers.clone(),
            trust_class: ExecutableTrustClass::NoLocalExecution,
        }),
    }
}

fn resolve_cwd(
    value: Option<&str>,
    plugin_root: &Path,
    plugin_data: &Path,
    root_text: &str,
    data_text: &str,
) -> Result<PathBuf> {
    let Some(value) = value else {
        return Ok(plugin_root.to_path_buf());
    };
    let expanded = expand_placeholders(value, root_text, data_text);
    let (root, relative, writable) = if let Some(relative) = value.strip_prefix("./") {
        (plugin_root, relative, false)
    } else if value == "${PLUGIN_ROOT}" {
        (plugin_root, "", false)
    } else if let Some(relative) = value.strip_prefix("${PLUGIN_ROOT}/") {
        (plugin_root, relative, false)
    } else if value == "${PLUGIN_DATA}" {
        (plugin_data, "", true)
    } else if let Some(relative) = value.strip_prefix("${PLUGIN_DATA}/") {
        (plugin_data, relative, true)
    } else {
        return Err(PluginError::UnsafePath(format!(
            "invalid expanded cwd {expanded:?}"
        )));
    };
    if relative.is_empty() {
        return Ok(root.to_path_buf());
    }
    let relative = normalize_relative(Path::new(relative))?;
    let path = root.join(relative);
    if writable {
        std::fs::create_dir_all(&path).map_err(|error| PluginError::io(&path, error))?;
    }
    let canonical = canonical_directory(&path)?;
    if !canonical.starts_with(root) {
        return Err(PluginError::UnsafePath(format!(
            "cwd {} escapes {}",
            canonical.display(),
            root.display()
        )));
    }
    Ok(canonical)
}

fn path_text(path: &Path) -> Result<&str> {
    path.to_str().ok_or_else(|| {
        PluginError::UnsafePath(format!("path is not valid UTF-8: {}", path.display()))
    })
}

fn expand_placeholders(input: &str, plugin_root: &str, plugin_data: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let mut rest = input;
    while let Some((index, replacement, token_len)) =
        next_placeholder(rest, plugin_root, plugin_data)
    {
        result.push_str(&rest[..index]);
        result.push_str(replacement);
        rest = &rest[index + token_len..];
    }
    result.push_str(rest);
    result
}

fn next_placeholder<'a>(
    input: &str,
    plugin_root: &'a str,
    plugin_data: &'a str,
) -> Option<(usize, &'a str, usize)> {
    const ROOT: &str = "${PLUGIN_ROOT}";
    const DATA: &str = "${PLUGIN_DATA}";
    match (input.find(ROOT), input.find(DATA)) {
        (Some(root), Some(data)) if root <= data => Some((root, plugin_root, ROOT.len())),
        (Some(_), Some(data)) => Some((data, plugin_data, DATA.len())),
        (Some(root), None) => Some((root, plugin_root, ROOT.len())),
        (None, Some(data)) => Some((data, plugin_data, DATA.len())),
        (None, None) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placeholder_expansion_is_single_pass() {
        let root = "/tmp/${PLUGIN_DATA}/root";
        assert_eq!(
            expand_placeholders("${PLUGIN_ROOT}/${PLUGIN_DATA}/$HOME", root, "/data"),
            "/tmp/${PLUGIN_DATA}/root//data/$HOME"
        );
    }
}
