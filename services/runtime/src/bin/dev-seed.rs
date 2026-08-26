//! Reset the development database and seed it with a working setup.
//!
//! Seeding drives the same dispatch the UI drives, so what lands is exactly
//! what the app expects to find — no hand-written rows, no schema knowledge
//! duplicated here, and nothing that validation would have refused.
//!
//! Order matters, because each step references the one before it: the current
//! development credential is synchronized into the platform keychain, the
//! provider points at it, the Environment selects the provider, and the Agent
//! is created against the Environment.

use std::sync::Arc;

use agent_factory_runtime::{EnvironmentServicePaths, Runtime};
use ipc_contract::{Frame, Request, Response, ResponseOutcome};
use project_store::ProjectStore;
use serde_json::{Value, json};

/// The labels of the development credentials. When the shell supplies a current
/// value, a clean seed writes it through the same runtime intent as Settings;
/// otherwise an existing Keychain value is reused.
const OLLAMA_SECRET_LABEL: &str = "OLLAMA_API_KEY";
const META_MUSE_SECRET_LABEL: &str = "META_MUSE_API_KEY";
/// The model to run on, as a person would name it. Ollama publishes dated and
/// preview builds rather than this exact string, so it is resolved against what
/// the provider actually offers instead of being written in blind.
const OLLAMA_MODEL: &str = "deepseek-v4-flash:cloud";
const META_MUSE_MODEL: &str = "muse-spark-1.2-contributor";
const DEFAULT_OLLAMA_ENDPOINT: &str = "https://ollama.com";
// A provider endpoint is the server root. Meta documents `https://api.meta.ai/v1`
// for its OpenAI-compatible surface, but Agent Factory speaks the Anthropic
// Messages endpoint, which the same docs reach from the bare host.
const DEFAULT_META_MUSE_ENDPOINT: &str = "https://api.meta.ai";

const OLLAMA_ENVIRONMENT: &str = "IPL Test";
const META_MUSE_ENVIRONMENT: &str = "IPL Test using Meta Muse";

const AGENT_NAME: &str = "IPL Analyst";
const DRAFT_NAME: &str = "IPL Analyst (DRAFT)";
const AGENT_ROOT: &str = "/Users/ravi/Documents/agent-factory-workspace/ IPL Analyst";
const OBJECTIVE: &str = "Build an IPL Analyst that accurately answers questions about IPL players, \
     teams, and player statistics, using web search to retrieve and verify up-to-date information \
     from trusted cricket sources such as ESPNcricinfo.";
const ACCEPTANCE_CRITERIA: [&str; 2] = [
    "Correctly identifies players across IPL teams and which team each player currently represents.",
    "Provides accurate IPL player statistics, including key batting and bowling performance metrics.",
];

fn main() {
    if let Err(error) = seed() {
        eprintln!("dev-seed: {error}");
        std::process::exit(1);
    }
}

#[cfg(target_os = "macos")]
fn secret_store() -> Result<Arc<dyn platform_secrets::SecretStore>, Box<dyn std::error::Error>> {
    Ok(Arc::new(platform_secrets::MacOsKeychain::new(
        "app.agentfactory.desktop",
    )?))
}

#[cfg(not(target_os = "macos"))]
fn secret_store() -> Result<Arc<dyn platform_secrets::SecretStore>, Box<dyn std::error::Error>> {
    Ok(Arc::new(platform_secrets::InMemorySecretStore::default()))
}

fn seed() -> Result<(), Box<dyn std::error::Error>> {
    let data_directory = agent_factory_runtime::application_data_directory()?;
    reset(&data_directory)?;

    let store = ProjectStore::open(data_directory.join("agent-factory.sqlite3"))?;
    let mut runtime = Runtime::with_environment_services(
        store,
        std::env::var_os("PATH")
            .map(|value| std::env::split_paths(&value).collect())
            .unwrap_or_default(),
        EnvironmentServicePaths {
            user_environments: data_directory.join("environments"),
            plugins: data_directory.join("plugins"),
        },
        secret_store()?,
    )?;

    // 1-2. Providers — each points at its own secret, and allows a model it
    //      really offers. Two exist so an Environment can be switched between
    //      them without adding a provider by hand first.
    let ollama = seed_provider(
        &mut runtime,
        ProviderSeed {
            name: "Ollama",
            kind: "ollama",
            secret_label: OLLAMA_SECRET_LABEL,
            endpoint_variable: "AGENT_FACTORY_SEED_OLLAMA_ENDPOINT",
            default_endpoint: DEFAULT_OLLAMA_ENDPOINT,
            requested_model: OLLAMA_MODEL,
        },
    )?;
    let meta_muse = seed_provider(
        &mut runtime,
        ProviderSeed {
            name: "Meta Muse",
            kind: "meta",
            secret_label: META_MUSE_SECRET_LABEL,
            endpoint_variable: "AGENT_FACTORY_SEED_META_MUSE_ENDPOINT",
            default_endpoint: DEFAULT_META_MUSE_ENDPOINT,
            requested_model: META_MUSE_MODEL,
        },
    )?;

    // 3. Environments — one per provider, each granting enough that a Factory
    //    Run advances without someone answering approval dialogs.
    seed_environment(&mut runtime, OLLAMA_ENVIRONMENT, &ollama)?;
    let meta_muse_environment = seed_environment(&mut runtime, META_MUSE_ENVIRONMENT, &meta_muse)?;

    // 4. Agent — in a trusted workspace, with the Meta Muse Environment
    //    recorded on its Draft. Both Environments are seeded, so the Ollama one
    //    stays one pick away in the Draft view.
    let agent = call(
        &mut runtime,
        "targetAgent.create",
        json!({
            "name": AGENT_NAME,
            "objective": OBJECTIVE,
            "acceptanceCriteria": ACCEPTANCE_CRITERIA,
            "repositoryRoot": AGENT_ROOT,
            "draftName": DRAFT_NAME,
            "trusted": true,
            "environmentId": meta_muse_environment,
        }),
    )?;
    println!(
        "agent      {AGENT_NAME} — draft `{DRAFT_NAME}` at {}",
        agent["draft"]["worktreePath"].as_str().unwrap_or("?")
    );
    Ok(())
}

/// Remove the durable state a fresh start must not inherit.
///
/// Environment descriptors go with the database: they reference provider ids
/// that live in it, so keeping them would restore Environments that can no
/// longer resolve. Installed plugins stay — they are fetched artifacts, not
/// state this seeds.
fn reset(data_directory: &std::path::Path) -> std::io::Result<()> {
    for name in [
        "agent-factory.sqlite3",
        "agent-factory.sqlite3-wal",
        "agent-factory.sqlite3-shm",
        agent_control::SOCKET_FILE_NAME,
    ] {
        match std::fs::remove_file(data_directory.join(name)) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
    }
    match std::fs::remove_dir_all(data_directory.join("environments")) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    println!("reset      {}", data_directory.display());
    Ok(())
}

/// What it takes to put one provider in the seeded database.
struct ProviderSeed {
    name: &'static str,
    /// The provider kind the runtime validates, such as `ollama` or `meta`.
    kind: &'static str,
    secret_label: &'static str,
    /// Overrides the endpoint, so a seed can point at a stand-in server.
    endpoint_variable: &'static str,
    default_endpoint: &'static str,
    requested_model: &'static str,
}

/// A provider as the rest of the seed refers to it.
struct SeededProvider {
    id: String,
    model: String,
}

/// Create one provider: synchronize its credential, settle on a model the
/// provider actually publishes, and record it.
fn seed_provider(
    runtime: &mut Runtime,
    seed: ProviderSeed,
) -> Result<SeededProvider, Box<dyn std::error::Error>> {
    let credential_ref = synchronize_secret(runtime, seed.secret_label)?;
    println!("secret     {} ({credential_ref})", seed.secret_label);

    let endpoint =
        std::env::var(seed.endpoint_variable).unwrap_or_else(|_| seed.default_endpoint.to_owned());
    let model = published_model(runtime, &seed, &endpoint, &credential_ref)?;
    let provider = call(
        runtime,
        "llmProvider.create",
        json!({
            "configuration": {
                "name": seed.name,
                "type": seed.kind,
                "endpoint": endpoint,
                "credentialRef": credential_ref,
                "allowedModels": [model],
            }
        }),
    )?;
    if model == seed.requested_model {
        println!("provider   {} @ {endpoint} → {model}", seed.name);
    } else {
        println!(
            "provider   {} @ {endpoint} → {model} (for {})",
            seed.name, seed.requested_model
        );
    }
    Ok(SeededProvider {
        id: field(&provider, "providerId")?,
        model,
    })
}

/// The model to allow, resolved against what the provider publishes.
///
/// A provider that cannot be reached is not a reason to refuse to seed: the
/// requested name is taken as given, and the resulting Environment reports its
/// own readiness. Only a reachable provider that publishes a catalog without a
/// match is an error worth stopping for.
fn published_model(
    runtime: &mut Runtime,
    seed: &ProviderSeed,
    endpoint: &str,
    credential_ref: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    let discovered = call(
        runtime,
        "llmProvider.models.list",
        json!({
            "provider": {
                "type": seed.kind,
                "endpoint": endpoint,
                "credentialRef": credential_ref,
            }
        }),
    );
    let available: Vec<String> = match discovered {
        Ok(discovered) => discovered["models"]
            .as_array()
            .map(|models| {
                models
                    .iter()
                    .filter_map(|model| model.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default(),
        Err(error) => {
            println!(
                "note       {} did not answer with a catalog ({error}); \
                 seeding `{}` as written",
                seed.name, seed.requested_model
            );
            return Ok(seed.requested_model.to_owned());
        }
    };
    if available.is_empty() {
        println!(
            "note       {} published no models; seeding `{}` as written",
            seed.name, seed.requested_model
        );
        return Ok(seed.requested_model.to_owned());
    }
    resolve_model(seed.requested_model, &available)
}

/// Create one Environment bound to a provider, granting enough that a Factory
/// Run advances without someone answering approval dialogs.
fn seed_environment(
    runtime: &mut Runtime,
    name: &str,
    provider: &SeededProvider,
) -> Result<String, Box<dyn std::error::Error>> {
    let environment = call(
        runtime,
        "environment.create",
        json!({
            "configuration": {
                "name": name,
                "environmentVariables": [],
                "llm": {
                    "providerId": provider.id,
                    "allowedModels": [provider.model],
                    "defaultModel": provider.model,
                },
                "plugins": [],
                "registries": [],
                "permissions": {
                    "trustedRead": "allow",
                    "trustedWrite": "allow",
                    "terminal": "allow",
                },
            }
        }),
    )?;
    let id = field(&environment, "environmentId")?;
    println!("environment {name} ({id}) — unattended");
    Ok(id)
}

/// Make the named development credential agree with the process launching the
/// clean seed. This is deliberately a seed-only policy: product Settings keeps
/// its existing write-only behavior, while `native:dev:clean` cannot silently
/// launch a provider with a stale Keychain value left by an older run.
fn synchronize_secret(
    runtime: &mut Runtime,
    label: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    let secrets = call(runtime, "secret.list", json!({}))?;
    let existing = secrets["secrets"]
        .as_array()
        .unwrap_or(&Vec::new())
        .iter()
        .find(|secret| {
            secret["label"]
                .as_str()
                .is_some_and(|existing| existing.eq_ignore_ascii_case(label))
        })
        .and_then(|secret| secret["secretRef"].as_str())
        .map(str::to_owned);
    let supplied = std::env::var(label).ok().filter(|value| !value.is_empty());

    match (existing, supplied) {
        (Some(secret_ref), Some(value)) => {
            call(
                runtime,
                "secret.replace",
                json!({"secretRef": secret_ref, "value": value}),
            )?;
            Ok(secret_ref)
        }
        (None, Some(value)) => {
            let result = call(
                runtime,
                "secret.create",
                json!({"label": label, "value": value}),
            )?;
            secret_ref_with_label(&result, label).ok_or_else(|| {
                "secret.create succeeded without returning the created credential".into()
            })
        }
        (Some(secret_ref), None) => Ok(secret_ref),
        (None, None) => Err({
            format!(
                "no `{label}` was supplied and no stored secret has that label. \
                 Export it for this command or add it in Settings → Secrets, then run this again."
            )
            .into()
        }),
    }
}

fn secret_ref_with_label(value: &Value, label: &str) -> Option<String> {
    value["secrets"]
        .as_array()?
        .iter()
        .find(|secret| secret["label"].as_str() == Some(label))?
        .get("secretRef")?
        .as_str()
        .map(str::to_owned)
}

/// Turn a model as a person names it into one the provider actually publishes.
///
/// Ollama ships dated and preview builds — `deepseek-v4-flash:0731:cloud` — so
/// a plain `deepseek-v4-flash:cloud` names a family and a channel rather than a
/// tag. Seeding a name that does not exist leaves a provider whose one allowed
/// model matches nothing in the list, which is invisible until someone opens
/// the page and finds no model selected.
fn resolve_model(
    requested: &str,
    available: &[String],
) -> Result<String, Box<dyn std::error::Error>> {
    if available.iter().any(|model| model == requested) {
        return Ok(requested.to_owned());
    }
    let (family, channel) = requested
        .rsplit_once(':')
        .ok_or_else(|| unavailable(requested, available))?;
    let mut candidates: Vec<&String> = available
        .iter()
        .filter(|model| {
            model.starts_with(&format!("{family}:")) && model.ends_with(&format!(":{channel}"))
        })
        .collect();
    // Deterministic, and dated builds sort ahead of previews.
    candidates.sort();
    candidates
        .first()
        .map(|model| (*model).clone())
        .ok_or_else(|| unavailable(requested, available))
}

fn unavailable(requested: &str, available: &[String]) -> Box<dyn std::error::Error> {
    format!(
        "the provider does not offer `{requested}`. It offers: {}",
        if available.is_empty() {
            "nothing — check the endpoint and the stored key".to_owned()
        } else {
            available.join(", ")
        }
    )
    .into()
}

fn call(
    runtime: &mut Runtime,
    method: &str,
    params: Value,
) -> Result<Value, Box<dyn std::error::Error>> {
    match runtime.handle_request(Request::new(method, params)).first() {
        Some(Frame::Response(Response {
            outcome: ResponseOutcome::Success { result },
            ..
        })) => Ok(result.clone()),
        Some(Frame::Response(Response {
            outcome: ResponseOutcome::Error { error },
            ..
        })) => Err(format!("{method} failed: {}", error.message).into()),
        other => Err(format!("{method} returned no response: {other:?}").into()),
    }
}

fn field(value: &Value, key: &str) -> Result<String, Box<dyn std::error::Error>> {
    value[key]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| format!("response is missing `{key}`").into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn catalog() -> Vec<String> {
        [
            "deepseek-v4-flash:0731",
            "deepseek-v4-flash:0731:cloud",
            "deepseek-v4-flash:preview",
            "deepseek-v4-flash:preview:cloud",
            "deepseek-v4-pro:preview",
            "deepseek-v4-pro:preview:cloud",
            "gemma4:31b",
            "gemma4:31b:cloud",
        ]
        .iter()
        .map(|model| (*model).to_owned())
        .collect()
    }

    /// The name a person uses is not a tag Ollama publishes, and seeding the
    /// literal string leaves a provider whose allowed model matches nothing.
    #[test]
    fn a_family_and_channel_resolve_to_a_published_build() {
        assert_eq!(
            resolve_model("deepseek-v4-flash:cloud", &catalog()).unwrap(),
            "deepseek-v4-flash:0731:cloud"
        );
        assert_eq!(
            resolve_model("deepseek-v4-pro:cloud", &catalog()).unwrap(),
            "deepseek-v4-pro:preview:cloud"
        );
    }

    #[test]
    fn an_exact_tag_is_taken_as_given() {
        assert_eq!(
            resolve_model("gemma4:31b:cloud", &catalog()).unwrap(),
            "gemma4:31b:cloud"
        );
    }

    #[test]
    fn a_model_no_provider_offers_says_what_is_available() {
        let error = resolve_model("llama9:cloud", &catalog())
            .unwrap_err()
            .to_string();
        assert!(error.contains("does not offer `llama9:cloud`"), "{error}");
        assert!(error.contains("gemma4:31b:cloud"), "{error}");

        let empty = resolve_model("deepseek-v4-flash:cloud", &[])
            .unwrap_err()
            .to_string();
        assert!(empty.contains("check the endpoint"), "{empty}");
    }

    /// A local build must not be mistaken for the cloud one.
    #[test]
    fn the_channel_has_to_match() {
        assert!(resolve_model("deepseek-v4-flash:local", &catalog()).is_err());
    }
}
