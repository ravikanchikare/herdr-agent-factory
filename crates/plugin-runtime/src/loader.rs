use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::io::Read;
use std::net::IpAddr;
use std::path::{Path, PathBuf};

use http::{HeaderName, HeaderValue};
use serde_json::{Map, Value};
use url::{Host, Url};

use crate::error::{PluginError, Result};
use crate::model::{
    Diagnostic, DiagnosticBoundary, LoadedPlugin, MCP_SCHEMA_V1, McpComponent, McpServerDefinition,
    PLUGIN_SCHEMA_V1, PluginAuthor, PluginManifest, SkillDefinition,
};
use crate::path::{canonical_directory, contained_path, require_contained_existing};

const MAX_CONFIG_BYTES: u64 = 1024 * 1024;

pub fn load_plugin(root: &Path) -> Result<LoadedPlugin> {
    let root = canonical_directory(root)?;
    let mut diagnostics = Vec::new();
    let manifest_path = require_regular_component(&root, "plugin.json").map_err(|message| {
        PluginError::InvalidManifest(format!("plugin.json is unavailable: {message}"))
    })?;
    let manifest_value = read_json_bounded(&manifest_path)
        .map_err(|error| PluginError::InvalidManifest(error.to_string()))?;
    let manifest = validate_manifest(manifest_value, &mut diagnostics)?;
    let skills = discover_skills(&root, &mut diagnostics);
    let mcp = load_mcp(&root, &mut diagnostics);

    Ok(LoadedPlugin {
        root,
        manifest,
        skills,
        mcp,
        diagnostics,
    })
}

fn read_bounded(path: &Path) -> Result<Vec<u8>> {
    let file = File::open(path).map_err(|error| PluginError::io(path, error))?;
    let mut bytes = Vec::new();
    file.take(MAX_CONFIG_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| PluginError::io(path, error))?;
    if bytes.len() as u64 > MAX_CONFIG_BYTES {
        return Err(PluginError::UnsafePath(format!(
            "{} exceeds the {} byte configuration limit",
            path.display(),
            MAX_CONFIG_BYTES
        )));
    }
    Ok(bytes)
}

fn read_json_bounded(path: &Path) -> Result<Value> {
    let bytes = read_bounded(path)?;
    serde_json::from_slice(&bytes).map_err(|error| {
        PluginError::InvalidManifest(format!("{} is not valid JSON: {error}", path.display()))
    })
}

fn require_regular_component(root: &Path, name: &str) -> std::result::Result<PathBuf, String> {
    let path = root.join(name);
    let resolved = require_contained_existing(root, &path).map_err(|error| error.to_string())?;
    let metadata = std::fs::metadata(&resolved).map_err(|error| error.to_string())?;
    if !metadata.is_file() {
        return Err(format!("{} is not a regular file", path.display()));
    }
    Ok(resolved)
}

fn validate_manifest(value: Value, diagnostics: &mut Vec<Diagnostic>) -> Result<PluginManifest> {
    let object = value
        .as_object()
        .ok_or_else(|| PluginError::InvalidManifest("top-level value must be an object".into()))?;
    const ALLOWED: &[&str] = &[
        "$schema",
        "name",
        "version",
        "description",
        "author",
        "homepage",
        "repository",
        "license",
        "keywords",
        "extensions",
    ];
    for key in object.keys().filter(|key| !ALLOWED.contains(&key.as_str())) {
        diagnostics.push(Diagnostic {
            boundary: DiagnosticBoundary::Plugin,
            code: "unknownManifestField".into(),
            message: format!("ignored unknown plugin.json field {key:?}"),
        });
    }

    let schema = required_string(object, "$schema", "plugin.json")?;
    if schema != PLUGIN_SCHEMA_V1 {
        return Err(PluginError::InvalidManifest(format!(
            "unsupported $schema {schema:?}"
        )));
    }
    let name = required_string(object, "name", "plugin.json")?;
    if !valid_plugin_name(&name) {
        return Err(PluginError::InvalidManifest(format!(
            "plugin name {name:?} does not satisfy Agent Plugins 1.0"
        )));
    }

    let version = optional_string(object, "version", "plugin.json")?;
    let description = optional_string(object, "description", "plugin.json")?;
    let homepage = optional_string(object, "homepage", "plugin.json")?;
    let repository = optional_string(object, "repository", "plugin.json")?;
    let license = optional_string(object, "license", "plugin.json")?;
    let keywords = match object.get("keywords") {
        None => Vec::new(),
        Some(Value::Array(values)) => values
            .iter()
            .enumerate()
            .map(|(index, value)| {
                value.as_str().map(ToOwned::to_owned).ok_or_else(|| {
                    PluginError::InvalidManifest(format!(
                        "plugin.json keywords[{index}] must be a string"
                    ))
                })
            })
            .collect::<Result<Vec<_>>>()?,
        Some(_) => {
            return Err(PluginError::InvalidManifest(
                "plugin.json keywords must be an array".into(),
            ));
        }
    };
    let author = validate_author(object.get("author"))?;
    let extensions = validate_extensions(object.get("extensions"), diagnostics)?;

    Ok(PluginManifest {
        schema,
        name,
        version,
        description,
        author,
        homepage,
        repository,
        license,
        keywords,
        extensions,
    })
}

fn validate_author(value: Option<&Value>) -> Result<Option<PluginAuthor>> {
    let Some(value) = value else { return Ok(None) };
    let object = value.as_object().ok_or_else(|| {
        PluginError::InvalidManifest("plugin.json author must be an object".into())
    })?;
    const ALLOWED: &[&str] = &["name", "email", "url"];
    if let Some(key) = object.keys().find(|key| !ALLOWED.contains(&key.as_str())) {
        return Err(PluginError::InvalidManifest(format!(
            "plugin.json author contains unknown field {key:?}"
        )));
    }
    Ok(Some(PluginAuthor {
        name: optional_string(object, "name", "plugin.json author")?,
        email: optional_string(object, "email", "plugin.json author")?,
        url: optional_string(object, "url", "plugin.json author")?,
    }))
}

fn validate_extensions(
    value: Option<&Value>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<BTreeMap<String, Value>> {
    let Some(value) = value else {
        return Ok(BTreeMap::new());
    };
    let Some(object) = value.as_object() else {
        diagnostics.push(Diagnostic {
            boundary: DiagnosticBoundary::Plugin,
            code: "invalidExtensionsIgnored".into(),
            message: "ignored non-object plugin.json extensions field".into(),
        });
        return Ok(BTreeMap::new());
    };
    let mut result = BTreeMap::new();
    for (namespace, value) in object {
        if !value.is_object() {
            return Err(PluginError::InvalidManifest(format!(
                "plugin.json extension {namespace:?} must be an object"
            )));
        }
        result.insert(namespace.clone(), value.clone());
    }
    Ok(result)
}

fn required_string(object: &Map<String, Value>, key: &str, context: &str) -> Result<String> {
    object
        .get(key)
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .ok_or_else(|| {
            PluginError::InvalidManifest(format!("{context} requires string field {key:?}"))
        })
}

fn optional_string(
    object: &Map<String, Value>,
    key: &str,
    context: &str,
) -> Result<Option<String>> {
    match object.get(key) {
        None => Ok(None),
        Some(Value::String(value)) => Ok(Some(value.clone())),
        Some(_) => Err(PluginError::InvalidManifest(format!(
            "{context} field {key:?} must be a string"
        ))),
    }
}

pub(crate) fn valid_plugin_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    (1..=64).contains(&bytes.len())
        && ascii_lowercase_or_digit(bytes[0])
        && ascii_lowercase_or_digit(bytes[bytes.len() - 1])
        && bytes.iter().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'.')
        })
        && !name.contains("--")
        && !name.contains("..")
}

fn ascii_lowercase_or_digit(byte: u8) -> bool {
    byte.is_ascii_lowercase() || byte.is_ascii_digit()
}

fn discover_skills(root: &Path, diagnostics: &mut Vec<Diagnostic>) -> Vec<SkillDefinition> {
    let path = root.join("skills");
    if !path.exists() {
        return Vec::new();
    }
    let resolved = match require_contained_existing(root, &path) {
        Ok(path) => path,
        Err(error) => {
            component_diagnostic(
                diagnostics,
                "skills",
                "invalidComponentPath",
                error.to_string(),
            );
            return Vec::new();
        }
    };
    if !resolved.is_dir() {
        component_diagnostic(
            diagnostics,
            "skills",
            "invalidComponentKind",
            "skills is not a directory".into(),
        );
        return Vec::new();
    }
    let mut entries = match std::fs::read_dir(&resolved) {
        Ok(entries) => entries
            .filter_map(std::result::Result::ok)
            .collect::<Vec<_>>(),
        Err(error) => {
            component_diagnostic(diagnostics, "skills", "readFailed", error.to_string());
            return Vec::new();
        }
    };
    entries.sort_by_key(std::fs::DirEntry::file_name);
    let mut skills = Vec::new();
    for entry in entries {
        let candidate = entry.path();
        let Ok(directory) = require_contained_existing(root, &candidate) else {
            skill_diagnostic(
                diagnostics,
                &entry.file_name().to_string_lossy(),
                "pathEscape",
                "skill directory escapes plugin root".into(),
            );
            continue;
        };
        if !directory.is_dir() {
            continue;
        }
        let directory_name = entry.file_name().to_string_lossy().into_owned();
        match load_skill(root, &directory, &directory_name) {
            Ok(skill) => skills.push(skill),
            Err(error) => skill_diagnostic(
                diagnostics,
                &directory_name,
                "invalidSkill",
                error.to_string(),
            ),
        }
    }
    skills
}

fn load_skill(root: &Path, directory: &Path, directory_name: &str) -> Result<SkillDefinition> {
    let skill_file = require_contained_existing(root, &directory.join("SKILL.md"))?;
    if !skill_file.is_file() {
        return Err(PluginError::UnsafePath(
            "SKILL.md is not a regular file".into(),
        ));
    }
    let bytes = read_bounded(&skill_file)?;
    let text = std::str::from_utf8(&bytes)
        .map_err(|_| PluginError::UnsafePath("SKILL.md is not UTF-8".into()))?;
    let normalized = text
        .strip_prefix("---\r\n")
        .or_else(|| text.strip_prefix("---\n"));
    let normalized = normalized.ok_or_else(|| {
        PluginError::UnsafePath("SKILL.md must begin with YAML frontmatter".into())
    })?;
    let boundary = normalized
        .find("\n---\n")
        .or_else(|| normalized.find("\r\n---\r\n"))
        .ok_or_else(|| PluginError::UnsafePath("SKILL.md frontmatter is not closed".into()))?;
    let frontmatter = &normalized[..boundary];
    let value: serde_norway::Value = serde_norway::from_str(frontmatter).map_err(|error| {
        PluginError::UnsafePath(format!("SKILL.md frontmatter is invalid YAML: {error}"))
    })?;
    let object = value
        .as_mapping()
        .ok_or_else(|| PluginError::UnsafePath("SKILL.md frontmatter must be a mapping".into()))?;
    let name = object
        .get(serde_norway::Value::String("name".into()))
        .and_then(serde_norway::Value::as_str)
        .ok_or_else(|| PluginError::UnsafePath("SKILL.md requires name".into()))?;
    let description = object
        .get(serde_norway::Value::String("description".into()))
        .and_then(serde_norway::Value::as_str)
        .ok_or_else(|| PluginError::UnsafePath("SKILL.md requires description".into()))?;
    if !valid_skill_name(name) || name != directory_name {
        return Err(PluginError::UnsafePath(format!(
            "skill name {name:?} is invalid or does not match {directory_name:?}"
        )));
    }
    if description.is_empty() || description.chars().count() > 1024 {
        return Err(PluginError::UnsafePath(
            "skill description must contain 1-1024 characters".into(),
        ));
    }
    Ok(SkillDefinition {
        name: name.into(),
        description: description.into(),
        skill_file,
    })
}

fn valid_skill_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    (1..=64).contains(&bytes.len())
        && ascii_lowercase_or_digit(bytes[0])
        && ascii_lowercase_or_digit(bytes[bytes.len() - 1])
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'-')
        && !name.contains("--")
}

fn load_mcp(root: &Path, diagnostics: &mut Vec<Diagnostic>) -> McpComponent {
    let path = root.join("mcp.json");
    if !path.exists() {
        return McpComponent::Absent;
    }
    let resolved = match require_regular_component(root, "mcp.json") {
        Ok(path) => path,
        Err(reason) => return disable_mcp(diagnostics, reason),
    };
    let value = match read_json_bounded(&resolved) {
        Ok(value) => value,
        Err(error) => return disable_mcp(diagnostics, error.to_string()),
    };
    let Some(object) = value.as_object() else {
        return disable_mcp(diagnostics, "mcp.json must be an object".into());
    };
    if let Some(key) = object
        .keys()
        .find(|key| !matches!(key.as_str(), "$schema" | "mcpServers"))
    {
        return disable_mcp(
            diagnostics,
            format!("mcp.json contains unknown top-level field {key:?}"),
        );
    }
    if object.get("$schema").and_then(Value::as_str) != Some(MCP_SCHEMA_V1) {
        return disable_mcp(diagnostics, "mcp.json targets an unsupported schema".into());
    }
    let Some(servers) = object.get("mcpServers").and_then(Value::as_object) else {
        return disable_mcp(diagnostics, "mcpServers must be an object".into());
    };
    let mut valid = Vec::new();
    for (name, value) in servers {
        match validate_mcp_server(root, name, value) {
            Ok(server) => valid.push(server),
            Err(message) => diagnostics.push(Diagnostic {
                boundary: DiagnosticBoundary::McpServer(name.clone()),
                code: "invalidMcpServer".into(),
                message,
            }),
        }
    }
    McpComponent::Loaded(valid)
}

fn validate_mcp_server(
    root: &Path,
    name: &str,
    value: &Value,
) -> std::result::Result<McpServerDefinition, String> {
    if name.is_empty() {
        return Err("MCP server name cannot be empty".into());
    }
    let object = value
        .as_object()
        .ok_or_else(|| "MCP server must be an object".to_string())?;
    let transport = object
        .get("type")
        .and_then(Value::as_str)
        .ok_or_else(|| "MCP server requires a string type".to_string())?;
    match transport {
        "stdio" => validate_stdio_server(root, name, object),
        "streamable-http" => validate_http_server(name, object, false),
        "sse" => validate_http_server(name, object, true),
        _ => Err(format!("unsupported MCP transport {transport:?}")),
    }
}

fn validate_stdio_server(
    root: &Path,
    name: &str,
    object: &Map<String, Value>,
) -> std::result::Result<McpServerDefinition, String> {
    reject_unknown(object, &["type", "command", "args", "env", "cwd"])?;
    let command = object
        .get("command")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty() && !value.contains('\0'))
        .ok_or_else(|| "stdio command must be a non-empty string".to_string())?;
    validate_command(root, command).map_err(|error| error.to_string())?;
    let args = string_array(object.get("args"), "args")?;
    let env = string_object(object.get("env"), "env")?;
    if env.contains_key("PLUGIN_ROOT") || env.contains_key("PLUGIN_DATA") {
        return Err("env must not define PLUGIN_ROOT or PLUGIN_DATA".into());
    }
    let cwd = match object.get("cwd") {
        None => None,
        Some(Value::String(value)) => {
            validate_cwd_form(root, value).map_err(|error| error.to_string())?;
            Some(value.clone())
        }
        Some(_) => return Err("cwd must be a string".into()),
    };
    Ok(McpServerDefinition::Stdio {
        name: name.into(),
        command: command.into(),
        args,
        env,
        cwd,
    })
}

fn validate_http_server(
    name: &str,
    object: &Map<String, Value>,
    sse: bool,
) -> std::result::Result<McpServerDefinition, String> {
    reject_unknown(object, &["type", "url", "headers"])?;
    let url = object
        .get("url")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "remote MCP url must be a non-empty string".to_string())?;
    validate_remote_url(url)?;
    let headers = string_object(object.get("headers"), "headers")?;
    let mut names = BTreeSet::new();
    for (key, value) in &headers {
        let header_name = HeaderName::from_bytes(key.as_bytes())
            .map_err(|_| format!("invalid HTTP header name {key:?}"))?;
        HeaderValue::from_str(value)
            .map_err(|_| format!("invalid HTTP header value for {key:?}"))?;
        if !names.insert(header_name.as_str().to_ascii_lowercase()) {
            return Err(format!("duplicate case-insensitive HTTP header {key:?}"));
        }
    }
    if sse {
        Ok(McpServerDefinition::Sse {
            name: name.into(),
            url: url.into(),
            headers,
        })
    } else {
        Ok(McpServerDefinition::StreamableHttp {
            name: name.into(),
            url: url.into(),
            headers,
        })
    }
}

fn validate_remote_url(value: &str) -> std::result::Result<(), String> {
    let url = Url::parse(value).map_err(|error| format!("invalid MCP URL: {error}"))?;
    if url.username() != "" || url.password().is_some() || url.fragment().is_some() {
        return Err("MCP URL must not contain user information or a fragment".into());
    }
    let host = url
        .host()
        .ok_or_else(|| "MCP URL requires a host".to_string())?;
    let loopback = match host {
        Host::Domain(value) => value == "localhost",
        Host::Ipv4(value) => IpAddr::V4(value).is_loopback(),
        Host::Ipv6(value) => IpAddr::V6(value).is_loopback(),
    };
    match url.scheme() {
        "https" => Ok(()),
        "http" if loopback => Ok(()),
        "http" => Err("non-loopback MCP URLs must use HTTPS".into()),
        _ => Err("MCP URL must use HTTP or HTTPS".into()),
    }
}

fn validate_command(root: &Path, command: &str) -> Result<()> {
    if let Some(relative) = command.strip_prefix("./") {
        if relative.is_empty() {
            return Err(PluginError::UnsafePath(
                "empty plugin-relative command".into(),
            ));
        }
        contained_path(root, Path::new(relative))?;
        return Ok(());
    }
    if command.contains('/') || command.contains('\\') || Path::new(command).is_absolute() {
        return Err(PluginError::UnsafePath(format!(
            "command {command:?} is neither bare nor plugin-relative"
        )));
    }
    Ok(())
}

fn validate_cwd_form(root: &Path, cwd: &str) -> Result<()> {
    if let Some(relative) = cwd.strip_prefix("./") {
        contained_path(root, Path::new(relative))?;
        return Ok(());
    }
    if cwd == "${PLUGIN_ROOT}" {
        return Ok(());
    }
    if let Some(relative) = cwd.strip_prefix("${PLUGIN_ROOT}/") {
        contained_path(root, Path::new(relative))?;
        return Ok(());
    }
    if cwd == "${PLUGIN_DATA}" {
        return Ok(());
    }
    if let Some(relative) = cwd.strip_prefix("${PLUGIN_DATA}/") {
        crate::path::normalize_relative(Path::new(relative))?;
        return Ok(());
    }
    Err(PluginError::UnsafePath(format!("invalid cwd form {cwd:?}")))
}

fn reject_unknown(
    object: &Map<String, Value>,
    allowed: &[&str],
) -> std::result::Result<(), String> {
    if let Some(key) = object.keys().find(|key| !allowed.contains(&key.as_str())) {
        return Err(format!("unknown field {key:?}"));
    }
    Ok(())
}

fn string_array(value: Option<&Value>, field: &str) -> std::result::Result<Vec<String>, String> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let array = value
        .as_array()
        .ok_or_else(|| format!("{field} must be an array of strings"))?;
    array
        .iter()
        .enumerate()
        .map(|(index, value)| {
            value
                .as_str()
                .map(ToOwned::to_owned)
                .ok_or_else(|| format!("{field}[{index}] must be a string"))
        })
        .collect()
}

fn string_object(
    value: Option<&Value>,
    field: &str,
) -> std::result::Result<BTreeMap<String, String>, String> {
    let Some(value) = value else {
        return Ok(BTreeMap::new());
    };
    let object = value
        .as_object()
        .ok_or_else(|| format!("{field} must be an object of strings"))?;
    object
        .iter()
        .map(|(key, value)| {
            value
                .as_str()
                .map(|value| (key.clone(), value.to_owned()))
                .ok_or_else(|| format!("{field}.{key} must be a string"))
        })
        .collect()
}

fn disable_mcp(diagnostics: &mut Vec<Diagnostic>, reason: String) -> McpComponent {
    component_diagnostic(diagnostics, "mcp", "mcpDisabled", reason.clone());
    McpComponent::Disabled { reason }
}

fn component_diagnostic(
    diagnostics: &mut Vec<Diagnostic>,
    component: &str,
    code: &str,
    message: String,
) {
    diagnostics.push(Diagnostic {
        boundary: DiagnosticBoundary::Component(component.into()),
        code: code.into(),
        message,
    });
}

fn skill_diagnostic(diagnostics: &mut Vec<Diagnostic>, skill: &str, code: &str, message: String) {
    diagnostics.push(Diagnostic {
        boundary: DiagnosticBoundary::Skill(skill.into()),
        code: code.into(),
        message,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plugin_names_match_the_official_constraints() {
        for name in ["a", "my-plugin", "acme.tools", "lint3r"] {
            assert!(valid_plugin_name(name), "{name}");
        }
        for name in ["", "My-Plugin", "-start", "has--double", "bad..dots"] {
            assert!(!valid_plugin_name(name), "{name}");
        }
    }

    #[test]
    fn only_secure_or_loopback_http_urls_are_valid() {
        assert!(validate_remote_url("https://example.com/mcp").is_ok());
        assert!(validate_remote_url("http://localhost:3000/mcp").is_ok());
        assert!(validate_remote_url("http://127.0.0.1/mcp").is_ok());
        assert!(validate_remote_url("http://example.com/mcp").is_err());
        assert!(validate_remote_url("https://user@example.com/mcp").is_err());
        assert!(validate_remote_url("https://example.com/mcp#fragment").is_err());
    }
}
