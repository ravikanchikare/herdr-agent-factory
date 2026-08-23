use std::collections::BTreeMap;
use std::io::{Cursor, Write};
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use base64::Engine;
use ed25519_dalek::{Signer, SigningKey};
use flate2::Compression;
use flate2::write::GzEncoder;
use plugin_runtime::{
    ArchiveLimits, EnvironmentPluginEntry, EnvironmentPluginSelection, ExecutableTrustClass,
    MCP_SCHEMA_JSON, MCP_SCHEMA_V1, McpComponent, PLUGIN_SCHEMA_JSON, PLUGIN_SCHEMA_V1,
    PluginError, PluginStore, RegistryClient, RegistryDownloader, ResolvedMcpServer, load_plugin,
    verify_catalog,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tempfile::TempDir;

fn fixture(path: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("plugins/fixtures")
        .join(path)
}

fn limits() -> ArchiveLimits {
    ArchiveLimits {
        max_compressed_bytes: 1024 * 1024,
        max_expanded_bytes: 1024 * 1024,
        max_entries: 100,
    }
}

fn safe_archive(files: &[(&str, &[u8], u32)]) -> Vec<u8> {
    let encoder = GzEncoder::new(Vec::new(), Compression::default());
    let mut builder = tar::Builder::new(encoder);
    for (path, bytes, mode) in files {
        let mut header = tar::Header::new_gnu();
        header.set_size(bytes.len() as u64);
        header.set_mode(*mode);
        header.set_mtime(0);
        header.set_cksum();
        builder.append_data(&mut header, path, *bytes).unwrap();
    }
    builder.into_inner().unwrap().finish().unwrap()
}

fn raw_archive(path: &str, kind: u8, link: Option<&str>, data: &[u8]) -> Vec<u8> {
    let mut header = [0_u8; 512];
    header[..path.len()].copy_from_slice(path.as_bytes());
    write_octal(&mut header[100..108], 0o644);
    write_octal(&mut header[108..116], 0);
    write_octal(&mut header[116..124], 0);
    write_octal(&mut header[124..136], data.len() as u64);
    write_octal(&mut header[136..148], 0);
    header[148..156].fill(b' ');
    header[156] = kind;
    if let Some(link) = link {
        header[157..157 + link.len()].copy_from_slice(link.as_bytes());
    }
    header[257..263].copy_from_slice(b"ustar\0");
    header[263..265].copy_from_slice(b"00");
    let checksum = header.iter().map(|byte| u64::from(*byte)).sum();
    write_checksum(&mut header[148..156], checksum);

    let mut tar = Vec::from(header);
    tar.extend_from_slice(data);
    let padding = (512 - data.len() % 512) % 512;
    tar.resize(tar.len() + padding + 1024, 0);
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(&tar).unwrap();
    encoder.finish().unwrap()
}

fn write_octal(field: &mut [u8], value: u64) {
    field.fill(b'0');
    let octal = format!("{value:o}");
    let start = field.len() - octal.len() - 1;
    field[start..start + octal.len()].copy_from_slice(octal.as_bytes());
    field[field.len() - 1] = 0;
}

fn write_checksum(field: &mut [u8], value: u64) {
    field.fill(b' ');
    let octal = format!("{value:06o}");
    field[..6].copy_from_slice(octal.as_bytes());
    field[6] = 0;
    field[7] = b' ';
}

fn sign_catalog(catalog: Value) -> (Vec<u8>, String, [u8; 32]) {
    let bytes = serde_json::to_vec_pretty(&catalog).unwrap();
    let signing_key = SigningKey::from_bytes(&[7_u8; 32]);
    let signature = signing_key.sign(&bytes);
    (
        bytes,
        base64::engine::general_purpose::STANDARD.encode(signature.to_bytes()),
        signing_key.verifying_key().to_bytes(),
    )
}

fn verified_catalog(archive: &[u8], name: &str, version: &str) -> plugin_runtime::VerifiedCatalog {
    let catalog = json!({
        "schemaVersion": 1,
        "generatedAt": "2026-08-08T00:00:00.000Z",
        "plugins": [{
            "id": name,
            "name": name,
            "version": version,
            "description": null,
            "archiveUrl": format!("https://registry.example/{name}-{version}.tar.gz"),
            "sha256": hex::encode(Sha256::digest(archive)),
        }],
    });
    let (bytes, signature, public_key) = sign_catalog(catalog);
    verify_catalog(&bytes, &signature, &public_key).unwrap()
}

fn install(store: &PluginStore, archive: &[u8], name: &str, version: &str) {
    let catalog = verified_catalog(archive, name, version);
    let entry = catalog.plugin_by_id(name).unwrap();
    let staged = store.stage(Cursor::new(archive)).unwrap();
    store.install_and_activate(entry, &staged).unwrap();
}

#[derive(Clone)]
struct MemoryDownloader {
    responses: Arc<Mutex<BTreeMap<String, MemoryResponse>>>,
    calls: Arc<AtomicUsize>,
}

#[derive(Clone)]
enum MemoryResponse {
    Bytes(Vec<u8>),
    Redirect,
}

impl MemoryDownloader {
    fn new(responses: impl IntoIterator<Item = (String, MemoryResponse)>) -> Self {
        Self {
            responses: Arc::new(Mutex::new(responses.into_iter().collect())),
            calls: Arc::new(AtomicUsize::new(0)),
        }
    }
}

impl RegistryDownloader for MemoryDownloader {
    fn get(&self, url: &str, _limit: u64) -> plugin_runtime::Result<Vec<u8>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        match self.responses.lock().unwrap().get(url).cloned() {
            Some(MemoryResponse::Bytes(bytes)) => Ok(bytes),
            Some(MemoryResponse::Redirect) => Err(PluginError::RedirectDenied),
            None => Err(PluginError::Network),
        }
    }
}

fn plugin_archive(name: &str, version: &str, extra: &[(&str, &[u8], u32)]) -> Vec<u8> {
    let manifest = serde_json::to_vec_pretty(&json!({
        "$schema": "https://agent-plugins.org/schemas/1.0.0/plugin.schema.json",
        "name": name,
        "version": version,
    }))
    .unwrap();
    let mut files = vec![("plugin.json", manifest.as_slice(), 0o644)];
    files.extend_from_slice(extra);
    safe_archive(&files)
}

#[test]
fn canonical_manifest_mcp_and_skill_boundaries_are_applied() {
    assert_eq!(
        serde_json::from_str::<Value>(PLUGIN_SCHEMA_JSON).unwrap()["$id"],
        PLUGIN_SCHEMA_V1
    );
    assert_eq!(
        serde_json::from_str::<Value>(MCP_SCHEMA_JSON).unwrap()["$id"],
        MCP_SCHEMA_V1
    );
    let valid = load_plugin(&fixture("valid-minimal")).unwrap();
    assert_eq!(valid.manifest.name, "agent-factory-fixture");
    assert_eq!(valid.skills.len(), 1);

    let mixed = load_plugin(&fixture("malicious/mcp-mixed")).unwrap();
    let McpComponent::Loaded(servers) = mixed.mcp else {
        panic!("valid sibling should keep MCP loaded");
    };
    assert_eq!(servers.len(), 1);
    assert_eq!(mixed.diagnostics.len(), 5);

    let mismatch = load_plugin(&fixture("malicious/skill-mismatch")).unwrap();
    assert!(mismatch.skills.is_empty());
    assert_eq!(mismatch.diagnostics.len(), 1);

    let traversal = load_plugin(&fixture("path-traversal")).unwrap();
    assert!(matches!(traversal.mcp, McpComponent::Loaded(ref servers) if servers.is_empty()));
}

#[test]
fn manifest_exceptions_match_the_official_failure_boundaries() {
    let temp = TempDir::new().unwrap();
    let manifest = json!({
        "$schema": PLUGIN_SCHEMA_V1,
        "name": "manifest-rules",
        "unknown": true,
        "extensions": "ignored"
    });
    std::fs::write(
        temp.path().join("plugin.json"),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
    let loaded = load_plugin(temp.path()).unwrap();
    assert_eq!(loaded.diagnostics.len(), 2);
    assert!(loaded.manifest.extensions.is_empty());

    let invalid_extension = json!({
        "$schema": PLUGIN_SCHEMA_V1,
        "name": "manifest-rules",
        "extensions": { "com.example.client": 7 }
    });
    std::fs::write(
        temp.path().join("plugin.json"),
        serde_json::to_vec(&invalid_extension).unwrap(),
    )
    .unwrap();
    assert!(matches!(
        load_plugin(temp.path()),
        Err(PluginError::InvalidManifest(_))
    ));
}

#[test]
fn signatures_are_verified_before_catalog_parsing_and_duplicates_are_rejected() {
    let catalog = json!({
        "schemaVersion": 1,
        "generatedAt": "2026-08-08T00:00:00Z",
        "plugins": [
            {"id":"same","name":"same","version":"1.0.0","description":null,"archiveUrl":"https://x/1","sha256":"00".repeat(32)},
            {"id":"same","name":"same","version":"2.0.0","description":null,"archiveUrl":"https://x/2","sha256":"11".repeat(32)}
        ]
    });
    let (bytes, signature, public_key) = sign_catalog(catalog);
    assert!(matches!(
        verify_catalog(&bytes, &signature, &public_key),
        Err(PluginError::InvalidCatalog(_))
    ));
    let mut tampered = bytes;
    tampered.push(b' ');
    assert!(matches!(
        verify_catalog(&tampered, &signature, &public_key),
        Err(PluginError::InvalidSignature(_))
    ));
}

#[test]
fn signed_malicious_catalog_metadata_fails_before_download() {
    let base = json!({
        "schemaVersion": 1,
        "generatedAt": "2026-08-08T00:00:00Z",
        "plugins": [{
            "id":"safe-plugin",
            "name":"safe-plugin",
            "version":"1.0.0",
            "description":null,
            "archiveUrl":"https://registry.example/safe.tar.gz",
            "sha256":"00".repeat(32)
        }]
    });
    let mut cases = Vec::new();
    let mut bad_date = base.clone();
    bad_date["generatedAt"] = json!("not-a-date");
    cases.push(bad_date);
    let mut bad_version = base.clone();
    bad_version["plugins"][0]["version"] = json!("latest");
    cases.push(bad_version);
    let mut mismatched_id = base.clone();
    mismatched_id["plugins"][0]["id"] = json!("different");
    cases.push(mismatched_id);
    for url in [
        "http://registry.example/safe.tar.gz",
        "https://user@registry.example/safe.tar.gz",
        "https://registry.example/safe.tar.gz#fragment",
    ] {
        let mut bad_url = base.clone();
        bad_url["plugins"][0]["archiveUrl"] = json!(url);
        cases.push(bad_url);
    }
    for catalog in cases {
        let (bytes, signature, public_key) = sign_catalog(catalog);
        assert!(matches!(
            verify_catalog(&bytes, &signature, &public_key),
            Err(PluginError::InvalidCatalog(_))
        ));
    }
}

#[test]
fn registry_client_enforces_https_redirects_and_bounds_around_custom_downloaders() {
    let temp = TempDir::new().unwrap();
    let store = PluginStore::open(temp.path(), limits()).unwrap();
    let downloader = MemoryDownloader::new([]);
    let calls = downloader.calls.clone();
    let client = RegistryClient::new(downloader, store, [0_u8; 32]);
    assert!(matches!(
        client.fetch_catalog(
            "http://registry.example/catalog.json",
            "https://registry.example/catalog.sig"
        ),
        Err(PluginError::HttpsRequired)
    ));
    assert_eq!(calls.load(Ordering::SeqCst), 0);

    let temp = TempDir::new().unwrap();
    let store = PluginStore::open(temp.path(), limits()).unwrap();
    let downloader = MemoryDownloader::new([(
        "https://registry.example/catalog.json".into(),
        MemoryResponse::Redirect,
    )]);
    let client = RegistryClient::new(downloader, store, [0_u8; 32]);
    assert!(matches!(
        client.fetch_catalog(
            "https://registry.example/catalog.json",
            "https://registry.example/catalog.sig"
        ),
        Err(PluginError::RedirectDenied)
    ));
}

#[test]
fn registry_client_verifies_then_downloads_and_installs() {
    let archive = plugin_archive("downloaded", "1.0.0", &[]);
    let catalog = json!({
        "schemaVersion": 1,
        "generatedAt": "2026-08-08T00:00:00Z",
        "plugins": [{
            "id":"downloaded",
            "name":"downloaded",
            "version":"1.0.0",
            "description":null,
            "archiveUrl":"https://registry.example/downloaded.tar.gz",
            "sha256":hex::encode(Sha256::digest(&archive))
        }]
    });
    let (catalog_bytes, signature, public_key) = sign_catalog(catalog);
    let downloader = MemoryDownloader::new([
        (
            "https://registry.example/catalog.json".into(),
            MemoryResponse::Bytes(catalog_bytes),
        ),
        (
            "https://registry.example/catalog.sig".into(),
            MemoryResponse::Bytes(signature.into_bytes()),
        ),
        (
            "https://registry.example/downloaded.tar.gz".into(),
            MemoryResponse::Bytes(archive),
        ),
    ]);
    let temp = TempDir::new().unwrap();
    let store = PluginStore::open(temp.path(), limits()).unwrap();
    let client = RegistryClient::new(downloader, store, public_key);
    let catalog = client
        .fetch_catalog(
            "https://registry.example/catalog.json",
            "https://registry.example/catalog.sig",
        )
        .unwrap();
    client
        .download_and_install(catalog.plugin_by_id("downloaded").unwrap())
        .unwrap();
}

#[test]
fn registry_client_rechecks_custom_downloader_response_size() {
    let oversized = vec![0_u8; 9];
    let catalog = json!({
        "schemaVersion": 1,
        "generatedAt": "2026-08-08T00:00:00Z",
        "plugins": [{
            "id":"oversized",
            "name":"oversized",
            "version":"1.0.0",
            "description":null,
            "archiveUrl":"https://registry.example/oversized.tar.gz",
            "sha256":hex::encode(Sha256::digest(&oversized))
        }]
    });
    let (catalog_bytes, signature, public_key) = sign_catalog(catalog);
    let downloader = MemoryDownloader::new([
        (
            "https://registry.example/catalog.json".into(),
            MemoryResponse::Bytes(catalog_bytes),
        ),
        (
            "https://registry.example/catalog.sig".into(),
            MemoryResponse::Bytes(signature.into_bytes()),
        ),
        (
            "https://registry.example/oversized.tar.gz".into(),
            MemoryResponse::Bytes(oversized),
        ),
    ]);
    let temp = TempDir::new().unwrap();
    let store = PluginStore::open(
        temp.path(),
        ArchiveLimits {
            max_compressed_bytes: 8,
            ..limits()
        },
    )
    .unwrap();
    let client = RegistryClient::new(downloader, store, public_key);
    let catalog = client
        .fetch_catalog(
            "https://registry.example/catalog.json",
            "https://registry.example/catalog.sig",
        )
        .unwrap();
    assert!(matches!(
        client.download_and_install(catalog.plugin_by_id("oversized").unwrap()),
        Err(PluginError::DownloadTooLarge { limit: 8 })
    ));
}

#[test]
fn compressed_expanded_and_entry_limits_fail_closed() {
    let temp = TempDir::new().unwrap();
    let tiny = ArchiveLimits {
        max_compressed_bytes: 8,
        ..limits()
    };
    let store = PluginStore::open(temp.path(), tiny).unwrap();
    assert!(matches!(
        store.stage(Cursor::new(vec![0_u8; 9])),
        Err(PluginError::UnsafeArchive(_))
    ));

    let archive = plugin_archive("bounded", "1.0.0", &[("large", &[0_u8; 32], 0o644)]);
    let store = PluginStore::open(
        temp.path().join("expanded"),
        ArchiveLimits {
            max_expanded_bytes: 16,
            ..limits()
        },
    )
    .unwrap();
    let catalog = verified_catalog(&archive, "bounded", "1.0.0");
    let staged = store.stage(Cursor::new(&archive)).unwrap();
    assert!(matches!(
        store.install_and_activate(catalog.plugin_by_id("bounded").unwrap(), &staged),
        Err(PluginError::UnsafeArchive(_))
    ));

    let store = PluginStore::open(
        temp.path().join("entries"),
        ArchiveLimits {
            max_entries: 1,
            ..limits()
        },
    )
    .unwrap();
    let staged = store.stage(Cursor::new(&archive)).unwrap();
    assert!(matches!(
        store.install_and_activate(catalog.plugin_by_id("bounded").unwrap(), &staged),
        Err(PluginError::UnsafeArchive(_))
    ));
}

#[test]
fn traversal_absolute_links_and_special_files_are_never_extracted() {
    let cases = [
        raw_archive("../escape", b'0', None, b"x"),
        raw_archive("/absolute", b'0', None, b"x"),
        raw_archive("link", b'2', Some("../escape"), b""),
        raw_archive("hard", b'1', Some("plugin.json"), b""),
        raw_archive("device", b'3', None, b""),
    ];
    for (index, archive) in cases.iter().enumerate() {
        let temp = TempDir::new().unwrap();
        let store = PluginStore::open(temp.path(), limits()).unwrap();
        let catalog = verified_catalog(archive, "malicious", "1.0.0");
        let staged = store.stage(Cursor::new(archive)).unwrap();
        assert!(
            matches!(
                store.install_and_activate(catalog.plugin_by_id("malicious").unwrap(), &staged),
                Err(PluginError::UnsafeArchive(_))
            ),
            "case {index}"
        );
        assert!(!temp.path().join("escape").exists());
    }
}

#[test]
fn duplicate_archive_paths_are_rejected_before_overwrite() {
    let manifest = serde_json::to_vec(&json!({
        "$schema": PLUGIN_SCHEMA_V1,
        "name": "duplicate-path",
        "version": "1.0.0"
    }))
    .unwrap();
    let archive = safe_archive(&[
        ("plugin.json", &manifest, 0o644),
        ("plugin.json", &manifest, 0o644),
    ]);
    let temp = TempDir::new().unwrap();
    let store = PluginStore::open(temp.path(), limits()).unwrap();
    let catalog = verified_catalog(&archive, "duplicate-path", "1.0.0");
    let staged = store.stage(Cursor::new(&archive)).unwrap();
    assert!(matches!(
        store.install_and_activate(catalog.plugin_by_id("duplicate-path").unwrap(), &staged),
        Err(PluginError::UnsafeArchive(_))
    ));
}

#[test]
fn artifact_digest_and_identity_must_match_the_verified_catalog() {
    let archive = plugin_archive("actual", "1.0.0", &[]);
    let temp = TempDir::new().unwrap();
    let store = PluginStore::open(temp.path(), limits()).unwrap();
    let wrong_catalog = verified_catalog(&archive, "claimed", "1.0.0");
    let staged = store.stage(Cursor::new(&archive)).unwrap();
    assert!(matches!(
        store.install_and_activate(wrong_catalog.plugin_by_id("claimed").unwrap(), &staged),
        Err(PluginError::RegistryMismatch(_))
    ));

    let catalog = verified_catalog(&archive, "actual", "1.0.0");
    let other = store.stage(Cursor::new(b"not the archive")).unwrap();
    assert!(matches!(
        store.install_and_activate(catalog.plugin_by_id("actual").unwrap(), &other),
        Err(PluginError::Integrity(_))
    ));
}

#[test]
fn signed_artifact_inspection_projects_components_without_installing() {
    let mcp = br#"{
      "$schema":"https://agent-plugins.org/schemas/1.0.0/mcp.schema.json",
      "mcpServers": {
        "remote":{"type":"streamable-http","url":"https://example.com/mcp"}
      }
    }"#;
    let skill = br#"---
name: inspect
description: Inspect a signed plugin before installation.
---

Inspect it.
"#;
    let archive = plugin_archive(
        "inspectable",
        "1.0.0",
        &[
            ("mcp.json", mcp, 0o644),
            ("skills/inspect/SKILL.md", skill, 0o644),
        ],
    );
    let catalog = verified_catalog(&archive, "inspectable", "1.0.0");
    let entry = catalog.plugin_by_id("inspectable").unwrap();
    let temp = TempDir::new().unwrap();
    let store = PluginStore::open(temp.path(), limits()).unwrap();
    let staged = store.stage(Cursor::new(&archive)).unwrap();

    let inspected = store.inspect_registry_plugin(entry, &staged).unwrap();

    assert_eq!(inspected.manifest.name, "inspectable");
    assert_eq!(inspected.skills[0].name, "inspect");
    let McpComponent::Loaded(servers) = inspected.mcp else {
        panic!("expected inspected MCP metadata");
    };
    assert_eq!(servers[0].name(), "remote");
    assert!(store.list_installed().unwrap().is_empty());
}

#[test]
fn version_activation_is_atomic_and_rollback_swaps_to_the_previous_version() {
    let temp = TempDir::new().unwrap();
    let store = PluginStore::open(temp.path(), limits()).unwrap();
    let first = plugin_archive("versions", "1.0.0", &[]);
    let second = plugin_archive("versions", "2.0.0", &[]);
    install(&store, &first, "versions", "1.0.0");
    install(&store, &second, "versions", "2.0.0");
    assert_eq!(
        store
            .active_plugin("versions")
            .unwrap()
            .manifest
            .version
            .as_deref(),
        Some("2.0.0")
    );
    store.rollback("versions").unwrap();
    assert_eq!(
        store
            .active_plugin("versions")
            .unwrap()
            .manifest
            .version
            .as_deref(),
        Some("1.0.0")
    );
}

#[test]
fn uninstall_removes_every_installed_version_but_retains_plugin_data() {
    let temp = TempDir::new().unwrap();
    let store = PluginStore::open(temp.path(), limits()).unwrap();
    let first = plugin_archive("versions", "1.0.0", &[]);
    let second = plugin_archive("versions", "2.0.0", &[]);
    install(&store, &first, "versions", "1.0.0");
    install(&store, &second, "versions", "2.0.0");
    let data = store.plugin_data_directory("coding", "versions").unwrap();

    store.uninstall("versions").unwrap();

    assert!(matches!(
        store.active_plugin("versions"),
        Err(PluginError::NotInstalled(_))
    ));
    assert!(store.list_installed().unwrap().is_empty());
    assert!(data.is_dir());
}

#[test]
fn environment_resolution_is_scoped_validated_and_never_executes() {
    let mcp = br#"{
      "$schema":"https://agent-plugins.org/schemas/1.0.0/mcp.schema.json",
      "mcpServers": {
        "local": {
          "type":"stdio",
          "command":"./bin/tool",
          "args":["--data=${PLUGIN_DATA}","$HOME"],
          "env":{"CONFIG":"${PLUGIN_ROOT}/config.json"},
          "cwd":"${PLUGIN_DATA}/work"
        },
        "remote":{"type":"streamable-http","url":"https://example.com/mcp"}
      }
    }"#;
    let skill = br#"---
name: verify
description: Verify a target against explicit acceptance criteria.
---

Verify it.
"#;
    let archive = plugin_archive(
        "environment-plugin",
        "1.0.0",
        &[
            ("mcp.json", mcp, 0o644),
            ("bin/tool", b"#!/bin/sh\nexit 99\n", 0o755),
            ("config.json", b"{}", 0o644),
            ("skills/verify/SKILL.md", skill, 0o644),
        ],
    );
    let temp = TempDir::new().unwrap();
    let store = PluginStore::open(temp.path(), limits()).unwrap();
    install(&store, &archive, "environment-plugin", "1.0.0");
    let selection = EnvironmentPluginSelection {
        environment_id: "environment-1".into(),
        plugins: vec![EnvironmentPluginEntry {
            name: "environment-plugin".into(),
            enabled_mcp_servers: Some(vec!["local".into()]),
            default_skills: vec!["verify".into()],
        }],
    };
    let resolved = store.resolve_environment_plugins(&selection).unwrap();
    assert_eq!(resolved.mcp_servers.len(), 1);
    assert_eq!(resolved.default_skills.len(), 1);
    let ResolvedMcpServer::Stdio {
        trust_class,
        requires_explicit_trust,
        args,
        env,
        cwd,
        ..
    } = &resolved.mcp_servers[0]
    else {
        panic!("expected stdio");
    };
    assert_eq!(*trust_class, ExecutableTrustClass::BundledExecutable);
    assert!(*requires_explicit_trust);
    assert!(args[0].contains("environment-1/environment-plugin"));
    assert_eq!(args[1], "$HOME");
    assert!(env["PLUGIN_DATA"].contains("environment-1/environment-plugin"));
    assert!(cwd.ends_with("environment-1/environment-plugin/work"));

    let duplicate = EnvironmentPluginSelection {
        environment_id: "environment-1".into(),
        plugins: vec![selection.plugins[0].clone(), selection.plugins[0].clone()],
    };
    assert!(matches!(
        store.resolve_environment_plugins(&duplicate),
        Err(PluginError::InvalidEnvironmentSelection(_))
    ));
}

#[cfg(unix)]
#[test]
fn directory_loader_rejects_symlink_escape() {
    use std::os::unix::fs::symlink;

    let temp = TempDir::new().unwrap();
    let outside = temp.path().join("outside.json");
    std::fs::write(
        &outside,
        br#"{"$schema":"https://agent-plugins.org/schemas/1.0.0/plugin.schema.json","name":"escaped"}"#,
    )
    .unwrap();
    let root = temp.path().join("plugin");
    std::fs::create_dir(&root).unwrap();
    symlink(&outside, root.join("plugin.json")).unwrap();
    assert!(matches!(
        load_plugin(&root),
        Err(PluginError::InvalidManifest(_))
    ));
}
