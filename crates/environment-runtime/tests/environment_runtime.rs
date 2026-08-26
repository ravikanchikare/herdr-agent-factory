use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use environment_runtime::{
    CatalogLimits, EnvironmentCatalog, EnvironmentDraft, EnvironmentError, EnvironmentValue,
    LiteralEnvironmentValue, RejectedEnvironment, ResolvedEnvironmentValueRef,
    derive_environment_id_base, validate_environment_id,
};
use platform_secrets::{InMemorySecretStore, SecretStore, SecretValue};
use plugin_runtime::{ArchiveLimits, PluginStore};
use serde_json::{Value, json};
use tempfile::TempDir;

fn supported() -> BTreeSet<String> {
    ["claude".to_owned()].into_iter().collect()
}

fn descriptor(id: &str) -> Value {
    json!({
        "$schema": "../schema.json",
        "schemaVersion": 1,
        "id": id,
        "name": format!("Environment {id}"),
        "harnesses": {
            "coding": "claude",
            "evaluation": "claude"
        },
        "plugins": [],
        "permissions": {
            "trustedRead": "allow",
            "trustedWrite": "ask",
            "terminal": "deny"
        },
        "environmentVariables": {},
        "registries": []
    })
}

fn write_environment(root: &Path, directory: &str, value: &Value) {
    let directory = root.join(directory);
    fs::create_dir_all(&directory).unwrap();
    fs::write(
        directory.join("environment.json"),
        serde_json::to_vec_pretty(value).unwrap(),
    )
    .unwrap();
}

fn load_temp(root: &Path) -> Result<EnvironmentCatalog, EnvironmentError> {
    EnvironmentCatalog::load(root, &supported(), CatalogLimits::default())
}

/// One unreadable Environment must not stop the catalog — and with it the app —
/// from loading. It is reported instead.
fn assert_rejected(root: &Path) -> RejectedEnvironment {
    let catalog = load_temp(root).expect("the catalog loads despite a bad descriptor");
    assert!(catalog.is_empty(), "a rejected Environment is not loaded");
    catalog
        .rejected()
        .first()
        .cloned()
        .expect("the rejection is reported")
}

fn draft(name: &str) -> EnvironmentDraft {
    EnvironmentDraft {
        name: name.to_owned(),
        ..EnvironmentDraft::default()
    }
}

#[test]
fn iterates_deterministically() {
    let root = TempDir::new().unwrap();
    write_environment(root.path(), "zulu", &descriptor("zulu"));
    write_environment(root.path(), "alpha", &descriptor("alpha"));
    let catalog = load_temp(root.path()).unwrap();
    assert_eq!(
        catalog.iter().map(|(id, _)| id).collect::<Vec<_>>(),
        ["alpha", "zulu"]
    );
    assert!(catalog.get("alpha").unwrap().descriptor_path.is_absolute());
}

#[test]
fn a_missing_root_is_an_empty_catalog() {
    // First boot: no Environment has been created yet, and there is no application
    // Environment to fall back on. That must load, not fail.
    let temp = TempDir::new().unwrap();
    let catalog = load_temp(&temp.path().join("missing")).unwrap();
    assert!(catalog.is_empty());
}

#[test]
fn configuration_saves_atomically_and_renaming_keeps_the_id() {
    let root = TempDir::new().unwrap();
    write_environment(root.path(), "ollama", &descriptor("ollama"));
    let mut catalog = load_temp(root.path()).unwrap();

    let saved = catalog
        .save_configuration(
            "ollama",
            EnvironmentDraft {
                name: "Renamed Ollama".into(),
                environment_variables: BTreeMap::from([(
                    "CUSTOM_MODEL_LABEL".to_owned(),
                    EnvironmentValue::Literal(LiteralEnvironmentValue {
                        literal: "gemma4:cloud".into(),
                    }),
                )]),
                llm: None,
                plugins: vec![environment_runtime::EnvironmentPlugin {
                    name: "review-tools".into(),
                    enabled_mcp_servers: vec!["review-http".into()],
                    default_skills: vec!["review".into()],
                }],
                registries: vec!["trusted-registry".into()],
                permissions: Default::default(),
            },
        )
        .unwrap();

    // Renaming never re-derives the id, so nothing that references the Environment
    // has to be rewritten.
    assert_eq!(saved.id, "ollama");
    assert_eq!(saved.name, "Renamed Ollama");

    let persisted: Value =
        serde_json::from_slice(&fs::read(root.path().join("ollama/environment.json")).unwrap())
            .unwrap();
    assert_eq!(persisted["id"], json!("ollama"));
    assert_eq!(persisted["name"], json!("Renamed Ollama"));
    assert_eq!(
        persisted["environmentVariables"]["CUSTOM_MODEL_LABEL"],
        json!({"literal": "gemma4:cloud"})
    );
    assert_eq!(persisted["registries"], json!(["trusted-registry"]));
    assert_eq!(persisted["plugins"][0]["name"], json!("review-tools"));

    // The temp file lives outside the Environment directory, so a crashed save can
    // never leave debris a later load has to interpret.
    assert_eq!(
        fs::read_dir(root.path().join("ollama"))
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect::<Vec<_>>(),
        ["environment.json"]
    );
}

#[test]
fn creates_a_environment_from_a_complete_draft() {
    let root = TempDir::new().unwrap();
    let mut catalog = load_temp(root.path()).unwrap();

    let created = catalog
        .create_user_environment(
            "new-environment".into(),
            EnvironmentDraft {
                name: "New Environment".into(),
                environment_variables: BTreeMap::from([(
                    "TOKEN".to_owned(),
                    EnvironmentValue::Literal(LiteralEnvironmentValue {
                        literal: "value".into(),
                    }),
                )]),
                llm: None,
                plugins: vec![],
                registries: vec!["trusted-registry".into()],
                permissions: Default::default(),
            },
        )
        .unwrap();
    assert_eq!(created.id, "new-environment");
    assert_eq!(created.name, "New Environment");

    // One write, not create-then-configure: the descriptor on disk is complete
    // the moment it exists.
    let persisted: Value = serde_json::from_slice(
        &fs::read(root.path().join("new-environment/environment.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(persisted["schemaVersion"], json!(1));
    assert_eq!(persisted["registries"], json!(["trusted-registry"]));
    assert_eq!(
        persisted["environmentVariables"]["TOKEN"],
        json!({"literal": "value"})
    );

    assert!(matches!(
        catalog.create_user_environment("new-environment".into(), draft("Duplicate")),
        Err(EnvironmentError::DuplicateEnvironment(id)) if id == "new-environment"
    ));
}

#[test]
fn delete_removes_the_directory_and_leaves_the_catalog_loadable() {
    let root = TempDir::new().unwrap();
    write_environment(root.path(), "keep", &descriptor("keep"));
    write_environment(root.path(), "remove", &descriptor("remove"));
    let mut catalog = load_temp(root.path()).unwrap();

    catalog.delete_user_environment("remove").unwrap();
    assert!(catalog.get("remove").is_none());
    assert!(!root.path().join("remove").exists());
    assert!(matches!(
        catalog.delete_user_environment("remove"),
        Err(EnvironmentError::NotFound(id)) if id == "remove"
    ));

    // No tombstone survives as something a reload would try to read.
    let reloaded = load_temp(root.path()).unwrap();
    assert_eq!(
        reloaded.iter().map(|(id, _)| id).collect::<Vec<_>>(),
        ["keep"]
    );
}

#[test]
fn load_skips_stray_entries_instead_of_failing() {
    // The root is user-writable and there is no fallback Environment, so a stray file
    // must not be able to stop the application from starting.
    let root = TempDir::new().unwrap();
    write_environment(root.path(), "environment", &descriptor("environment"));
    fs::write(root.path().join(".DS_Store"), b"junk").unwrap();
    fs::write(
        root.path().join(".environment-environment.json.tmp-4711"),
        b"{",
    )
    .unwrap();
    fs::create_dir(root.path().join(".deleted-old-4711")).unwrap();
    fs::create_dir(root.path().join("Bad_Name")).unwrap();
    fs::write(root.path().join("Bad_Name/environment.json"), b"{}").unwrap();
    fs::create_dir(root.path().join("no-descriptor")).unwrap();
    // A crashed save leaves a temp file inside the Environment directory; siblings of
    // the descriptor are ignored and swept up.
    fs::write(
        root.path().join("environment/environment.json.tmp-4711"),
        b"{",
    )
    .unwrap();

    let catalog = load_temp(root.path()).unwrap();
    assert_eq!(
        catalog.iter().map(|(id, _)| id).collect::<Vec<_>>(),
        ["environment"]
    );
    assert!(
        !root
            .path()
            .join("environment/environment.json.tmp-4711")
            .exists()
    );
}

#[test]
fn derives_ids_from_names_that_always_validate() {
    for (name, expected) in [
        ("Local Ollama", "local-ollama"),
        ("Ollama — cloud", "ollama-cloud"),
        ("  spaced  out  ", "spaced-out"),
        ("2024 Review", "environment-2024-review"),
        ("\u{1f30d}\u{1f30e}", "environment"),
        ("", "environment"),
        ("-leading", "leading"),
        ("42", "environment-42"),
    ] {
        assert_eq!(derive_environment_id_base(name), expected, "name: {name:?}");
    }

    // Whatever the name, the derived base is a usable id: the caller only has
    // to make it unique.
    for name in [
        "",
        "---",
        "999",
        "A",
        "Ünïcödé",
        "日本語",
        "x".repeat(200).as_str(),
        "Trailing hyphen -",
        "a-b-c 1 2 3",
    ] {
        let derived = derive_environment_id_base(name);
        assert!(
            validate_environment_id(&derived).is_ok(),
            "{name:?} derived invalid id {derived:?}"
        );
    }
}

#[test]
fn vendored_schema_matches_the_rust_model() {
    let schema: Value = serde_json::from_str(environment_runtime::ENVIRONMENT_SCHEMA_JSON).unwrap();
    assert_eq!(
        schema["properties"]["schemaVersion"]["const"],
        json!(environment_runtime::ENVIRONMENT_SCHEMA_VERSION)
    );

    assert!(schema["properties"]["llm"]["properties"]["providerId"].is_object());
    assert!(schema.to_string().contains("providerId"));
    assert!(!schema.to_string().contains("credentialRef"));
    assert!(!schema.to_string().contains("endpoint"));
}

#[test]
fn rejects_unknown_fields_duplicate_ids_and_unsupported_harnesses() {
    let bundled = TempDir::new().unwrap();
    let mut unknown = descriptor("unknown");
    unknown["surprise"] = json!(true);
    write_environment(bundled.path(), "unknown", &unknown);
    assert_rejected(bundled.path());

    let bundled = TempDir::new().unwrap();
    let mut value = descriptor("other-agent");
    value["harnesses"]["evaluation"] = json!("codex");
    write_environment(bundled.path(), "other-agent", &value);
    let rejection = assert_rejected(bundled.path());
    assert!(rejection.reason.contains("codex"), "{}", rejection.reason);
}

#[test]
fn rejects_schema_shape_duplicates_and_malformed_references() {
    for mutate in [
        |value: &mut Value| value["schemaVersion"] = json!(2),
        |value: &mut Value| value["id"] = json!("Bad_ID"),
        |value: &mut Value| value["registries"] = json!(["good", "good"]),
        |value: &mut Value| value["registries"] = json!(["https://bad.example"]),
        |value: &mut Value| value["environmentVariables"] = json!({"PATH": {"literal": "bad"}}),
        |value: &mut Value| {
            value["environmentVariables"] = json!({"DYLD_INSERT_LIBRARIES": {"literal": "bad"}})
        },
        |value: &mut Value| {
            value["environmentVariables"] =
                json!({"TOKEN": {"secretRef": "secret_ABCDEF0123456789ABCDEF012345678G"}})
        },
        |value: &mut Value| {
            value["plugins"] = json!([
                {"name":"p", "enabledMcpServers":[], "defaultSkills":[]},
                {"name":"p", "enabledMcpServers":[], "defaultSkills":[]}
            ])
        },
        |value: &mut Value| {
            value["plugins"] = json!([
                {"name":"p", "enabledMcpServers":["server", "server"], "defaultSkills":[]}
            ])
        },
    ] {
        let bundled = TempDir::new().unwrap();
        let mut value = descriptor("invalid");
        mutate(&mut value);
        write_environment(bundled.path(), "invalid", &value);
        let catalog = load_temp(bundled.path()).expect("catalog still loads");
        assert_eq!(
            catalog.rejected().len(),
            1,
            "mutation should be rejected: {value}"
        );
        assert!(catalog.is_empty(), "mutation should be rejected: {value}");
    }
}

#[test]
fn validates_environment_owned_llm_policy_and_default_model() {
    let valid_policy = json!({
        "providerId": "00000000-0000-4000-8000-000000000010",
        "allowedModels": ["claude-sonnet-4", "evaluation-model"],
        "defaultModel": "claude-sonnet-4"
    });
    let bundled = TempDir::new().unwrap();
    let mut value = descriptor("configured");
    value["llm"] = valid_policy.clone();
    write_environment(bundled.path(), "configured", &value);
    let catalog = load_temp(bundled.path()).unwrap();
    let serialized = serde_json::to_value(&catalog.get("configured").unwrap().descriptor).unwrap();
    assert_eq!(serialized["llm"], valid_policy);
}

#[test]
fn rejects_invalid_llm_policy_and_reserved_environment() {
    type Mutation = Box<dyn Fn(&mut Value)>;
    let mutations: Vec<Mutation> = vec![
        Box::new(|value| {
            value["llm"] = json!({
                "providerId": "00000000-0000-4000-8000-000000000010",
                "allowedModels": []
            })
        }),
        Box::new(|value| {
            value["llm"] = json!({
                "providerId": "00000000-0000-4000-8000-000000000010",
                "allowedModels":["model", "model"], "defaultModel":"model"
            })
        }),
        Box::new(|value| {
            value["llm"] = json!({
                "providerId": "not-a-uuid"
            })
        }),
        Box::new(|value| {
            value["llm"] = json!({
                "providerId": "00000000-0000-4000-8000-000000000010",
                "allowedModels":["model"], "defaultModel":"other"
            })
        }),
        Box::new(|value| {
            value["environmentVariables"] = json!({"ANTHROPIC_MODEL": {"literal":"bad"}})
        }),
        Box::new(|value| {
            value["environmentVariables"] = json!({"CLAUDE_CODE_USE_BEDROCK": {"literal":"1"}})
        }),
    ];
    for mutate in mutations {
        let bundled = TempDir::new().unwrap();
        let mut value = descriptor("invalid-llm");
        mutate(&mut value);
        write_environment(bundled.path(), "invalid-llm", &value);
        let catalog = load_temp(bundled.path()).expect("catalog still loads");
        assert_eq!(
            catalog.rejected().len(),
            1,
            "mutation should be rejected: {value}"
        );
        assert!(catalog.is_empty(), "mutation should be rejected: {value}");
    }
}

#[test]
fn duplicate_environment_keys_are_not_last_value_wins() {
    let bundled = TempDir::new().unwrap();
    let directory = bundled.path().join("duplicate-env");
    fs::create_dir(&directory).unwrap();
    let raw = descriptor("duplicate-env").to_string().replace(
        "\"environmentVariables\":{}",
        "\"environmentVariables\":{\"TOKEN\":{\"literal\":\"one\"},\"TOKEN\":{\"literal\":\"two\"}}",
    );
    fs::write(directory.join("environment.json"), raw).unwrap();
    assert_rejected(bundled.path());
}

#[test]
fn enforces_file_catalog_and_collection_bounds() {
    let bundled = TempDir::new().unwrap();
    write_environment(bundled.path(), "one", &descriptor("one"));
    let tiny = CatalogLimits {
        max_descriptor_bytes: 8,
        ..CatalogLimits::default()
    };
    assert!(matches!(
        EnvironmentCatalog::load(bundled.path(), &supported(), tiny),
        Err(EnvironmentError::LimitExceeded(_))
    ));

    let bundled = TempDir::new().unwrap();
    write_environment(bundled.path(), "one", &descriptor("one"));
    write_environment(bundled.path(), "two", &descriptor("two"));
    let one_environment = CatalogLimits {
        max_environments: 1,
        ..CatalogLimits::default()
    };
    assert!(matches!(
        EnvironmentCatalog::load(bundled.path(), &supported(), one_environment),
        Err(EnvironmentError::LimitExceeded(_))
    ));

    let bundled = TempDir::new().unwrap();
    let mut value = descriptor("plugins");
    value["plugins"] = json!([
        {"name":"one", "enabledMcpServers":[], "defaultSkills":[]},
        {"name":"two", "enabledMcpServers":[], "defaultSkills":[]}
    ]);
    write_environment(bundled.path(), "plugins", &value);
    let one_plugin = CatalogLimits {
        max_plugins_per_environment: 1,
        ..CatalogLimits::default()
    };
    let catalog = EnvironmentCatalog::load(bundled.path(), &supported(), one_plugin)
        .expect("a per-Environment limit rejects that Environment, not the catalog");
    assert_eq!(catalog.rejected().len(), 1);
}

#[cfg(unix)]
#[test]
fn rejects_symlinked_roots_directories_and_descriptors() {
    use std::os::unix::fs::symlink;

    let actual = TempDir::new().unwrap();
    let holder = TempDir::new().unwrap();
    let linked_root = holder.path().join("environments");
    symlink(actual.path(), &linked_root).unwrap();
    assert!(matches!(
        load_temp(&linked_root),
        Err(EnvironmentError::UnsafePath(_))
    ));

    let bundled = TempDir::new().unwrap();
    let outside = TempDir::new().unwrap();
    write_environment(outside.path(), "evil", &descriptor("evil"));
    symlink(outside.path().join("evil"), bundled.path().join("evil")).unwrap();
    assert!(matches!(
        load_temp(bundled.path()),
        Err(EnvironmentError::UnsafePath(_))
    ));

    let bundled = TempDir::new().unwrap();
    let outside = TempDir::new().unwrap();
    let directory = bundled.path().join("evil");
    fs::create_dir(&directory).unwrap();
    fs::write(
        outside.path().join("environment.json"),
        descriptor("evil").to_string(),
    )
    .unwrap();
    symlink(
        outside.path().join("environment.json"),
        directory.join("environment.json"),
    )
    .unwrap();
    // A symlinked descriptor is scoped to its own Environment, so it is rejected
    // rather than fatal. A symlinked root still is not: the whole tree is suspect.
    let catalog = load_temp(bundled.path()).expect("the catalog loads");
    assert_eq!(catalog.rejected().len(), 1);
    assert!(
        catalog.rejected()[0]
            .reason
            .contains("must be a regular file"),
        "{}",
        catalog.rejected()[0].reason
    );
}

#[test]
fn rejects_a_descriptor_that_disagrees_with_its_directory() {
    // The directory name is the Environment's identity everywhere else, so a
    // descriptor claiming a different id is a genuine inconsistency — but it
    // disqualifies only that Environment.
    let root = TempDir::new().unwrap();
    write_environment(root.path(), "directory", &descriptor("different"));
    let rejection = assert_rejected(root.path());
    assert!(matches!(
        (),
        () if rejection.reason.contains("does not match directory")
    ));
}

#[test]
fn resolves_secrets_without_debug_or_serializable_projections() {
    let bundled = TempDir::new().unwrap();
    let store = InMemorySecretStore::default();
    let metadata = store
        .create(
            "API token",
            SecretValue::new(b"top-secret".to_vec()).unwrap(),
        )
        .unwrap();
    let mut value = descriptor("secrets");
    value["environmentVariables"] = json!({
        "ENDPOINT": {"literal": "https://example.invalid"},
        "TOKEN": {"secretRef": metadata.reference.as_str()}
    });
    write_environment(bundled.path(), "secrets", &value);
    let environment = load_temp(bundled.path()).unwrap();
    let resolver = environment_runtime::StoredSecretResolver::new(&store);
    let environment = environment
        .get("secrets")
        .unwrap()
        .descriptor
        .resolve_environment(&resolver)
        .unwrap();
    assert_eq!(environment.len(), 2);
    assert!(matches!(
        environment.get("ENDPOINT"),
        Some(ResolvedEnvironmentValueRef::Literal(
            "https://example.invalid"
        ))
    ));
    assert!(matches!(
        environment.get("TOKEN"),
        Some(ResolvedEnvironmentValueRef::Secret {
            value: "top-secret",
            ..
        })
    ));
    let debug = format!("{environment:?}");
    assert!(debug.contains("[REDACTED]"));
    assert!(!debug.contains("top-secret"));
}

#[test]
fn secret_resolution_errors_are_redacted_and_non_utf8_is_rejected() {
    struct InvalidUtf8;
    impl environment_runtime::SecretResolver for InvalidUtf8 {
        fn resolve(
            &self,
            _reference: &platform_secrets::SecretRef,
        ) -> Result<SecretValue, platform_secrets::SecretError> {
            SecretValue::new(vec![0xff])
        }
    }

    let bundled = TempDir::new().unwrap();
    let mut value = descriptor("secret-error");
    value["environmentVariables"] = json!({
        "TOKEN": {"secretRef": "secret_00000000000000000000000000000000"}
    });
    write_environment(bundled.path(), "secret-error", &value);
    let catalog = load_temp(bundled.path()).unwrap();
    assert!(matches!(
        catalog
            .get("secret-error")
            .unwrap()
            .descriptor
            .resolve_environment(&InvalidUtf8),
        Err(EnvironmentError::SecretNotUtf8(_))
    ));
}

#[test]
fn creates_non_executing_plugin_plans() {
    let bundled = TempDir::new().unwrap();
    write_environment(bundled.path(), "plugins", &descriptor("plugins"));
    let catalog = load_temp(bundled.path()).unwrap();
    let descriptor = &catalog.get("plugins").unwrap().descriptor;
    let selection = descriptor.plugin_selection();
    assert_eq!(selection.environment_id, "plugins");
    assert!(selection.plugins.is_empty());

    let store_root = TempDir::new().unwrap();
    let store = PluginStore::open(
        store_root.path(),
        ArchiveLimits {
            max_compressed_bytes: 1024,
            max_expanded_bytes: 1024,
            max_entries: 16,
        },
    )
    .unwrap();
    let plan = descriptor.resolve_plugin_plan(&store).unwrap();
    assert_eq!(plan.environment_id, "plugins");
    assert!(plan.mcp_servers.is_empty());
    assert!(plan.default_skills.is_empty());
}
