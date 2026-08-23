#![forbid(unsafe_code)]
//! A deliberately narrow, session-scoped Anthropic Messages reverse proxy.

use std::fmt;
use std::net::Ipv4Addr;
use std::sync::{Arc, RwLock, mpsc};
use std::thread::JoinHandle;
use std::time::Duration;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::extract::State;
use axum::http::{HeaderMap, Method, Request, Response, StatusCode, Uri, header};
use axum::response::IntoResponse;
use axum::routing::any;
use futures_util::StreamExt;
use llm_provider_runtime::{LlmProviderConfiguration, LlmProviderKind};
use platform_secrets::SecretValue;
use reqwest::redirect::Policy;
use serde_json::Value;
use tokio::sync::oneshot;
use url::Url;

pub const MAX_REQUEST_BODY_BYTES: usize = 16 * 1024 * 1024;
const MAX_DISCOVERY_BODY_BYTES: usize = 4 * 1024 * 1024;
const MAX_MODELS: usize = 4_096;
const MAX_MODEL_ID_CHARS: usize = 256;
const SENTINEL_TOKEN: &str = "agent-factory-loopback";

#[derive(Debug, thiserror::Error)]
pub enum GatewayError {
    #[error("gateway configuration is invalid: {0}")]
    InvalidConfig(String),
    #[error("gateway startup failed: {0}")]
    Startup(String),
    #[error("provider request failed: {0}")]
    Provider(String),
}

pub struct GatewayConfig {
    pub provider: LlmProviderConfiguration,
    pub model_id: String,
    pub credential: Option<SecretValue>,
}

impl fmt::Debug for GatewayConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GatewayConfig")
            .field("provider_type", &self.provider.provider_type)
            .field("model_id", &self.model_id)
            .field(
                "credential",
                &self.credential.as_ref().map(|_| "[REDACTED]"),
            )
            .finish()
    }
}

struct GatewayState {
    upstream: Url,
    model_id: Arc<RwLock<String>>,
    credential: Option<SecretValue>,
    client: reqwest::Client,
}

impl fmt::Debug for GatewayState {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GatewayState")
            .field("upstream", &self.upstream.as_str())
            .field("model_id", &"[SESSION STATE]")
            .field(
                "credential",
                &self.credential.as_ref().map(|_| "[REDACTED]"),
            )
            .finish()
    }
}

pub struct GatewayHandle {
    origin: String,
    model_id: Arc<RwLock<String>>,
    shutdown: Option<oneshot::Sender<()>>,
    thread: Option<JoinHandle<()>>,
}

impl GatewayHandle {
    pub fn start(config: GatewayConfig) -> Result<Self, GatewayError> {
        let upstream = Url::parse(&config.provider.endpoint)
            .map_err(|error| GatewayError::InvalidConfig(error.to_string()))?;
        let client = provider_client()?;
        let model_id = Arc::new(RwLock::new(config.model_id));
        let state = Arc::new(GatewayState {
            upstream,
            model_id: Arc::clone(&model_id),
            credential: config.credential,
            client,
        });
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let (shutdown_tx, shutdown_rx) = oneshot::channel();
        let thread = std::thread::Builder::new()
            .name("agent-factory-llm-gateway".into())
            .spawn(move || {
                let runtime = match tokio::runtime::Builder::new_multi_thread()
                    .enable_all()
                    .worker_threads(2)
                    .build()
                {
                    Ok(runtime) => runtime,
                    Err(error) => {
                        let _ = ready_tx.send(Err(error.to_string()));
                        return;
                    }
                };
                runtime.block_on(async move {
                    let listener =
                        match tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await {
                            Ok(listener) => listener,
                            Err(error) => {
                                let _ = ready_tx.send(Err(error.to_string()));
                                return;
                            }
                        };
                    let address = match listener.local_addr() {
                        Ok(address) => address,
                        Err(error) => {
                            let _ = ready_tx.send(Err(error.to_string()));
                            return;
                        }
                    };
                    let app = Router::new().fallback(any(proxy)).with_state(state);
                    let _ = ready_tx.send(Ok(address));
                    let _ = axum::serve(listener, app)
                        .with_graceful_shutdown(async {
                            let _ = shutdown_rx.await;
                        })
                        .await;
                });
            })
            .map_err(|error| GatewayError::Startup(error.to_string()))?;
        let address = ready_rx
            .recv_timeout(Duration::from_secs(10))
            .map_err(|error| GatewayError::Startup(error.to_string()))?
            .map_err(GatewayError::Startup)?;
        Ok(Self {
            origin: format!("http://{address}"),
            model_id,
            shutdown: Some(shutdown_tx),
            thread: Some(thread),
        })
    }

    pub fn origin(&self) -> &str {
        &self.origin
    }

    pub fn set_model(&self, model_id: impl Into<String>) {
        *self.model_id.write().expect("gateway model lock poisoned") = model_id.into();
    }
    pub fn anthropic_environment(&self, model_id: &str) -> [(String, String); 3] {
        [
            ("ANTHROPIC_BASE_URL".into(), self.origin.clone()),
            ("ANTHROPIC_AUTH_TOKEN".into(), SENTINEL_TOKEN.into()),
            ("ANTHROPIC_MODEL".into(), model_id.to_owned()),
        ]
    }
}

impl Drop for GatewayHandle {
    fn drop(&mut self) {
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

async fn proxy(State(state): State<Arc<GatewayState>>, request: Request<Body>) -> Response<Body> {
    let (parts, body) = request.into_parts();
    if parts.method != Method::POST || !allowed_path(parts.uri.path()) {
        return StatusCode::METHOD_NOT_ALLOWED.into_response();
    }
    let body = match to_bytes(body, MAX_REQUEST_BODY_BYTES).await {
        Ok(body) => body,
        Err(_) => {
            return (StatusCode::PAYLOAD_TOO_LARGE, "request body exceeds limit").into_response();
        }
    };
    let model_id = state
        .model_id
        .read()
        .expect("gateway model lock poisoned")
        .clone();
    let rewritten = match force_model(&body, &model_id) {
        Ok(body) => body,
        Err(message) => return (StatusCode::BAD_REQUEST, message).into_response(),
    };
    let url = match join_upstream(&state.upstream, &parts.uri) {
        Ok(url) => url,
        Err(error) => return (StatusCode::BAD_GATEWAY, error.to_string()).into_response(),
    };
    let headers = outbound_headers(&parts.headers, state.credential.as_ref());
    let upstream = match state
        .client
        .post(url)
        .headers(headers)
        .body(rewritten)
        .send()
        .await
    {
        Ok(response) => response,
        Err(_) => return (StatusCode::BAD_GATEWAY, "provider request failed").into_response(),
    };
    let status = upstream.status();
    let mut response = Response::builder().status(status);
    for (name, value) in &filtered_headers(upstream.headers()) {
        response = response.header(name, value);
    }
    let stream = upstream
        .bytes_stream()
        .map(|chunk| chunk.map_err(std::io::Error::other));
    response
        .body(Body::from_stream(stream))
        .unwrap_or_else(|_| StatusCode::BAD_GATEWAY.into_response())
}

fn allowed_path(path: &str) -> bool {
    matches!(path, "/v1/messages" | "/v1/messages/count_tokens")
}

pub fn join_upstream(base: &Url, uri: &Uri) -> Result<Url, GatewayError> {
    let mut url = base.clone();
    let base_path = base.path().trim_end_matches('/');
    url.set_path(&format!("{base_path}{}", uri.path()));
    url.set_query(uri.query());
    Ok(url)
}

pub fn force_model(body: &[u8], model_id: &str) -> Result<Vec<u8>, &'static str> {
    let mut value: Value =
        serde_json::from_slice(body).map_err(|_| "request body must be valid JSON")?;
    let object = value
        .as_object_mut()
        .ok_or("request body must be a JSON object")?;
    object.insert("model".into(), Value::String(model_id.to_owned()));
    serde_json::to_vec(&value).map_err(|_| "request body could not be encoded")
}

fn filtered_headers(headers: &HeaderMap) -> HeaderMap {
    let mut result = headers.clone();
    for name in [
        "connection",
        "keep-alive",
        "proxy-authenticate",
        "proxy-authorization",
        "te",
        "trailer",
        "transfer-encoding",
        "upgrade",
        "host",
        "content-length",
    ] {
        result.remove(name);
    }
    result
}

fn outbound_headers(inbound: &HeaderMap, credential: Option<&SecretValue>) -> HeaderMap {
    let mut result = filtered_headers(inbound);
    for name in [
        "authorization",
        "x-api-key",
        "anthropic-api-key",
        "api-key",
        "cookie",
    ] {
        result.remove(name);
    }
    if let Some(secret) = credential
        && let Ok(text) = std::str::from_utf8(secret.expose())
        && let Ok(value) = header::HeaderValue::from_str(&format!("Bearer {}", text.trim()))
    {
        result.insert(header::AUTHORIZATION, value);
    }
    result
}

fn provider_client() -> Result<reqwest::Client, GatewayError> {
    reqwest::Client::builder()
        .redirect(Policy::none())
        .connect_timeout(Duration::from_secs(10))
        .build()
        .map_err(|error| GatewayError::Startup(error.to_string()))
}

pub fn discover_models(
    provider: &LlmProviderConfiguration,
    credential: Option<SecretValue>,
) -> Result<Vec<String>, GatewayError> {
    let provider = provider.clone();
    std::thread::scope(|scope| {
        scope
            .spawn(|| {
                tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .map_err(|error| GatewayError::Provider(error.to_string()))?
                    .block_on(discover_models_async(&provider, credential.as_ref()))
            })
            .join()
            .map_err(|_| GatewayError::Provider("model discovery thread failed".into()))?
    })
}

const OLLAMA_CLOUD_CATALOG_URL: &str = "https://ollama.com/api/tags";

async fn discover_models_async(
    provider: &LlmProviderConfiguration,
    credential: Option<&SecretValue>,
) -> Result<Vec<String>, GatewayError> {
    let base = Url::parse(&provider.endpoint)
        .map_err(|error| GatewayError::InvalidConfig(error.to_string()))?;
    // An endpoint is the provider's server root, never a versioned prefix: the
    // proxy forwards the caller's own `/v1/messages`, so a version written into
    // the endpoint would be sent twice. Every path here carries its own.
    let path = match provider.provider_type {
        LlmProviderKind::Ollama => "/api/tags",
        LlmProviderKind::Litellm | LlmProviderKind::Meta | LlmProviderKind::OpenAi => "/v1/models",
    };
    let url = join_upstream(&base, &path.parse().expect("static URI"))?;
    let mut request = provider_client()?.get(url);
    if let Some(secret) = credential {
        let token = std::str::from_utf8(secret.expose())
            .map_err(|_| GatewayError::Provider("credential is not UTF-8".into()))?;
        request = request.bearer_auth(token.trim());
    }
    let response = request
        .send()
        .await
        .map_err(|error| GatewayError::Provider(error.to_string()))?;
    if !response.status().is_success() {
        return Err(GatewayError::Provider(format!(
            "provider returned {}",
            response.status()
        )));
    }
    let bytes = read_capped_body(response).await?;
    let mut models = parse_models(provider.provider_type, &bytes)?;

    if provider.provider_type == LlmProviderKind::Ollama {
        // The public catalog lists every model ollama.com can run on your
        // behalf, independent of the configured endpoint or credential.
        // Best-effort: a signed-out or offline catalog fetch should not
        // fail discovery of the (already-succeeded) primary endpoint.
        if let Ok(cloud_models) = discover_ollama_cloud_catalog().await {
            models.extend(cloud_models);
            models.sort();
            models.dedup();
        }
    }

    Ok(models)
}

async fn discover_ollama_cloud_catalog() -> Result<Vec<String>, GatewayError> {
    let response = provider_client()?
        .get(OLLAMA_CLOUD_CATALOG_URL)
        .send()
        .await
        .map_err(|error| GatewayError::Provider(error.to_string()))?;
    if !response.status().is_success() {
        return Err(GatewayError::Provider(format!(
            "cloud catalog returned {}",
            response.status()
        )));
    }
    let bytes = read_capped_body(response).await?;
    let models = parse_models(LlmProviderKind::Ollama, &bytes)?;
    Ok(models.into_iter().map(ensure_cloud_suffix).collect())
}

fn ensure_cloud_suffix(name: String) -> String {
    if name.ends_with(":cloud") {
        name
    } else {
        format!("{name}:cloud")
    }
}

async fn read_capped_body(response: reqwest::Response) -> Result<Vec<u8>, GatewayError> {
    let mut bytes = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| GatewayError::Provider(error.to_string()))?;
        if bytes.len().saturating_add(chunk.len()) > MAX_DISCOVERY_BODY_BYTES {
            return Err(GatewayError::Provider(
                "model response exceeds limit".into(),
            ));
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

pub fn parse_models(kind: LlmProviderKind, bytes: &[u8]) -> Result<Vec<String>, GatewayError> {
    let value: Value = serde_json::from_slice(bytes)
        .map_err(|_| GatewayError::Provider("model response is invalid JSON".into()))?;
    let entries = match kind {
        LlmProviderKind::Ollama => value.get("models").and_then(Value::as_array),
        LlmProviderKind::Litellm | LlmProviderKind::Meta | LlmProviderKind::OpenAi => {
            value.get("data").and_then(Value::as_array)
        }
    }
    .ok_or_else(|| GatewayError::Provider("model response has an invalid shape".into()))?;
    let key = match kind {
        LlmProviderKind::Ollama => "name",
        LlmProviderKind::Litellm | LlmProviderKind::Meta | LlmProviderKind::OpenAi => "id",
    };
    let mut models = entries
        .iter()
        .filter_map(|entry| entry.get(key).and_then(Value::as_str))
        .filter(|id| {
            !id.is_empty()
                && id.trim() == *id
                && id.chars().count() <= MAX_MODEL_ID_CHARS
                && !id.chars().any(char::is_control)
        })
        .take(MAX_MODELS)
        .map(str::to_owned)
        .collect::<Vec<_>>();
    models.sort();
    models.dedup();
    Ok(models)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Bytes;
    use tokio::sync::{Mutex, oneshot};

    #[test]
    fn joins_path_and_preserves_query() {
        let base = Url::parse("https://gateway.example.com/root").unwrap();
        let uri: Uri = "/v1/messages?beta=true".parse().unwrap();
        assert_eq!(
            join_upstream(&base, &uri).unwrap().as_str(),
            "https://gateway.example.com/root/v1/messages?beta=true"
        );
    }

    /// An endpoint is a server root. Discovery and the proxy have to agree on
    /// that, or a provider written to satisfy one sends the other's traffic to
    /// a doubled path — which reads to the agent as a model it cannot reach.
    #[test]
    fn discovery_and_chat_resolve_under_one_server_root() {
        let base = Url::parse("https://api.meta.ai").unwrap();
        assert_eq!(
            join_upstream(&base, &"/v1/models".parse().unwrap())
                .unwrap()
                .as_str(),
            "https://api.meta.ai/v1/models"
        );
        // The proxy forwards the caller's own path, so the same root has to
        // produce the documented Messages endpoint.
        assert_eq!(
            join_upstream(&base, &"/v1/messages".parse().unwrap())
                .unwrap()
                .as_str(),
            "https://api.meta.ai/v1/messages"
        );
    }

    #[test]
    fn forces_model_and_rejects_malformed_json() {
        let body = force_model(br#"{"model":"incoming","messages":[]}"#, "selected").unwrap();
        assert_eq!(
            serde_json::from_slice::<Value>(&body).unwrap()["model"],
            "selected"
        );
        assert!(force_model(b"not-json", "selected").is_err());
    }

    #[test]
    fn parses_bounded_deduplicated_models() {
        let models = parse_models(
            LlmProviderKind::Ollama,
            br#"{"models":[{"name":"z"},{"name":"a"},{"name":"a"}]}"#,
        )
        .unwrap();
        assert_eq!(models, ["a", "z"]);
    }

    #[test]
    fn parses_openai_compatible_models_for_meta_and_openai() {
        let body = br#"{"object":"list","data":[{"id":"muse-spark-1.2","object":"model"},{"id":"muse-spark-1.1","object":"model"}]}"#;
        assert_eq!(
            parse_models(LlmProviderKind::Meta, body).unwrap(),
            ["muse-spark-1.1", "muse-spark-1.2"]
        );
        assert_eq!(
            parse_models(LlmProviderKind::OpenAi, body).unwrap(),
            ["muse-spark-1.1", "muse-spark-1.2"]
        );
    }

    #[test]
    fn provider_kind_wire_format_matches_frontend_contract() {
        assert_eq!(
            serde_json::to_string(&LlmProviderKind::Ollama).unwrap(),
            "\"ollama\""
        );
        assert_eq!(
            serde_json::to_string(&LlmProviderKind::Litellm).unwrap(),
            "\"litellm\""
        );
        assert_eq!(
            serde_json::to_string(&LlmProviderKind::Meta).unwrap(),
            "\"meta\""
        );
        assert_eq!(
            serde_json::to_string(&LlmProviderKind::OpenAi).unwrap(),
            "\"openai\""
        );
    }

    #[test]
    fn ensure_cloud_suffix_adds_suffix_once() {
        assert_eq!(ensure_cloud_suffix("glm-5.2".into()), "glm-5.2:cloud");
        assert_eq!(ensure_cloud_suffix("glm-5.2:cloud".into()), "glm-5.2:cloud");
    }

    #[test]
    fn debug_output_redacts_credentials() {
        let config = GatewayConfig {
            provider: provider(LlmProviderKind::Litellm, "https://example.com"),
            model_id: "model".into(),
            credential: Some(SecretValue::new("top-secret").unwrap()),
        };
        let debug = format!("{config:?}");
        assert!(debug.contains("[REDACTED]"));
        assert!(!debug.contains("top-secret"));
    }

    #[test]
    fn dropping_gateway_releases_its_loopback_listener() {
        let gateway = GatewayHandle::start(GatewayConfig {
            provider: provider(LlmProviderKind::Ollama, "http://127.0.0.1:11434"),
            model_id: "model".into(),
            credential: None,
        })
        .unwrap();
        let address = gateway.origin().strip_prefix("http://").unwrap().to_owned();
        gateway.set_model("confirmed-model");
        assert_eq!(
            gateway
                .model_id
                .read()
                .expect("gateway model lock poisoned")
                .as_str(),
            "confirmed-model"
        );
        assert!(std::net::TcpStream::connect(&address).is_ok());
        drop(gateway);
        assert!(std::net::TcpStream::connect(&address).is_err());
    }

    #[tokio::test]
    #[ignore = "requires network access to ollama.com"]
    async fn ollama_cloud_catalog_lists_cloud_suffixed_models() {
        let models = discover_ollama_cloud_catalog()
            .await
            .expect("cloud catalog fetch must succeed");
        assert!(!models.is_empty());
        assert!(models.iter().all(|model| model.ends_with(":cloud")));
    }

    #[tokio::test]
    async fn proxy_preserves_sse_forces_model_and_replaces_credentials() {
        let (capture_tx, capture_rx) = oneshot::channel();
        let capture_tx = Arc::new(Mutex::new(Some(capture_tx)));
        let app = Router::new().fallback(any({
            let capture_tx = capture_tx.clone();
            move |request: Request<Body>| {
                let capture_tx = capture_tx.clone();
                async move {
                    let (parts, body) = request.into_parts();
                    let body = to_bytes(body, MAX_REQUEST_BODY_BYTES).await.unwrap();
                    if let Some(sender) = capture_tx.lock().await.take() {
                        let _ = sender.send((parts.uri, parts.headers, body));
                    }
                    let chunks = futures_util::stream::iter([
                        Ok::<_, std::io::Error>(Bytes::from_static(
                            b"event: message\ndata: one\n\n",
                        )),
                        Ok(Bytes::from_static(b"event: done\ndata: two\n\n")),
                    ]);
                    Response::builder()
                        .status(StatusCode::OK)
                        .header(header::CONTENT_TYPE, "text/event-stream")
                        .body(Body::from_stream(chunks))
                        .unwrap()
                }
            }
        }));
        let listener = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .await
            .unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let gateway = GatewayHandle::start(GatewayConfig {
            provider: provider(LlmProviderKind::Litellm, &format!("http://{address}")),
            model_id: "selected-model".into(),
            credential: Some(SecretValue::new("upstream-token").unwrap()),
        })
        .unwrap();
        let response = provider_client()
            .unwrap()
            .post(format!("{}/v1/messages?beta=true", gateway.origin()))
            .header(header::AUTHORIZATION, "Bearer inbound-secret")
            .header("x-api-key", "inbound-api-key")
            .header(header::COOKIE, "session=inbound-cookie")
            .json(&serde_json::json!({"model":"incoming", "messages":[]}))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.bytes().await.unwrap(),
            Bytes::from_static(b"event: message\ndata: one\n\nevent: done\ndata: two\n\n")
        );
        let (uri, headers, body) = capture_rx.await.unwrap();
        assert_eq!(
            uri.path_and_query().unwrap().as_str(),
            "/v1/messages?beta=true"
        );
        assert_eq!(headers[header::AUTHORIZATION], "Bearer upstream-token");
        assert!(!headers.contains_key("x-api-key"));
        assert!(!headers.contains_key(header::COOKIE));
        assert!(!headers.values().any(|value| {
            value
                .as_bytes()
                .windows(14)
                .any(|window| window == b"inbound-secret")
        }));
        assert_eq!(
            serde_json::from_slice::<Value>(&body).unwrap()["model"],
            "selected-model"
        );
        drop(gateway);
        server.abort();
    }

    fn provider(provider_type: LlmProviderKind, endpoint: &str) -> LlmProviderConfiguration {
        LlmProviderConfiguration {
            provider_type,
            endpoint: endpoint.into(),
            credential_ref: None,
            allowed_models: vec!["selected-model".into()],
        }
    }
}
