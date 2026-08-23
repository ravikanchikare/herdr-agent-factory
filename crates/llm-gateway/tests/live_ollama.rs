use llm_gateway::{GatewayConfig, GatewayHandle, discover_models};
use llm_provider_runtime::{LlmProviderConfiguration, LlmProviderKind};

fn local_ollama() -> LlmProviderConfiguration {
    LlmProviderConfiguration {
        provider_type: LlmProviderKind::Ollama,
        endpoint: "http://127.0.0.1:11434".into(),
        credential_ref: None,
        allowed_models: vec!["glm-5.2:cloud".into()],
    }
}

#[tokio::test]
#[ignore = "requires the user's installed Ollama service and an existing model"]
async fn discovers_and_streams_an_existing_model_without_pulling() {
    let provider = local_ollama();
    let models = discover_models(&provider, None).expect("Ollama discovery must succeed");
    let model = models
        .iter()
        .find(|model| model.as_str() == "glm-5.2:cloud")
        .or_else(|| models.first())
        .expect("the live smoke requires an already installed model")
        .clone();
    let gateway = GatewayHandle::start(GatewayConfig {
        provider,
        model_id: model.clone(),
        credential: None,
    })
    .expect("session gateway must start");
    let response = reqwest::Client::new()
        .post(format!("{}/v1/messages", gateway.origin()))
        .json(&serde_json::json!({
            "model": "must-be-overridden",
            "max_tokens": 16,
            "stream": true,
            "messages": [{"role":"user", "content":"Reply briefly with gateway smoke ok"}]
        }))
        .send()
        .await
        .expect("stream request must reach Ollama");
    assert!(
        response.status().is_success(),
        "status={}",
        response.status()
    );
    let bytes = response.bytes().await.expect("stream must remain readable");
    let text = String::from_utf8_lossy(&bytes);
    assert!(text.contains("event: message_start"), "response={text}");
    assert!(text.contains("event: message_stop"), "response={text}");
}
