use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const PLUGIN_SCHEMA_V1: &str = "https://agent-plugins.org/schemas/1.0.0/plugin.schema.json";
pub const MCP_SCHEMA_V1: &str = "https://agent-plugins.org/schemas/1.0.0/mcp.schema.json";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginManifest {
    #[serde(rename = "$schema")]
    pub schema: String,
    pub name: String,
    pub version: Option<String>,
    pub description: Option<String>,
    pub author: Option<PluginAuthor>,
    pub homepage: Option<String>,
    pub repository: Option<String>,
    pub license: Option<String>,
    pub keywords: Vec<String>,
    pub extensions: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginAuthor {
    pub name: Option<String>,
    pub email: Option<String>,
    pub url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostic {
    pub boundary: DiagnosticBoundary,
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "id", rename_all = "camelCase")]
pub enum DiagnosticBoundary {
    Plugin,
    Component(String),
    McpServer(String),
    Skill(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillDefinition {
    pub name: String,
    pub description: String,
    pub skill_file: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum McpServerDefinition {
    Stdio {
        name: String,
        command: String,
        args: Vec<String>,
        env: BTreeMap<String, String>,
        cwd: Option<String>,
    },
    StreamableHttp {
        name: String,
        url: String,
        headers: BTreeMap<String, String>,
    },
    Sse {
        name: String,
        url: String,
        headers: BTreeMap<String, String>,
    },
}

impl McpServerDefinition {
    pub fn name(&self) -> &str {
        match self {
            Self::Stdio { name, .. }
            | Self::StreamableHttp { name, .. }
            | Self::Sse { name, .. } => name,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum McpComponent {
    Absent,
    Disabled { reason: String },
    Loaded(Vec<McpServerDefinition>),
}

#[derive(Debug, Clone)]
pub struct LoadedPlugin {
    pub root: PathBuf,
    pub manifest: PluginManifest,
    pub skills: Vec<SkillDefinition>,
    pub mcp: McpComponent,
    pub diagnostics: Vec<Diagnostic>,
}

/// A validated registry artifact inspected without activating it in the local
/// plugin store. Paths inside Skills are valid only while inspection runs, so
/// callers should project the portable metadata immediately.
#[derive(Debug, Clone)]
pub struct InspectedPlugin {
    pub manifest: PluginManifest,
    pub skills: Vec<SkillDefinition>,
    pub mcp: McpComponent,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ExecutableTrustClass {
    NoLocalExecution,
    BundledExecutable,
    PathExecutable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolvedMcpServer {
    Stdio {
        plugin_name: String,
        name: String,
        command: PathBuf,
        args: Vec<String>,
        env: BTreeMap<String, String>,
        cwd: PathBuf,
        trust_class: ExecutableTrustClass,
        requires_explicit_trust: bool,
    },
    StreamableHttp {
        plugin_name: String,
        name: String,
        url: String,
        headers: BTreeMap<String, String>,
        trust_class: ExecutableTrustClass,
    },
    Sse {
        plugin_name: String,
        name: String,
        url: String,
        headers: BTreeMap<String, String>,
        trust_class: ExecutableTrustClass,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedSkill {
    pub plugin_name: String,
    pub name: String,
    pub description: String,
    pub skill_file: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedEnvironmentPlugins {
    pub environment_id: String,
    pub mcp_servers: Vec<ResolvedMcpServer>,
    pub default_skills: Vec<ResolvedSkill>,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentPluginSelection {
    pub environment_id: String,
    pub plugins: Vec<EnvironmentPluginEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentPluginEntry {
    pub name: String,
    /// `None` enables every valid MCP entry; an empty list enables none.
    pub enabled_mcp_servers: Option<Vec<String>>,
    /// Skill names injected into the agent's default skill catalog.
    pub default_skills: Vec<String>,
}
