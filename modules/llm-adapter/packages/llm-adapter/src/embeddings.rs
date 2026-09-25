//! OpenAI 兼容 `/v1/embeddings` 调用（记忆架构 A 的「真实 embedder」）。
//!
//! 自包含的 async 函数，供 core-runtime 语义召回的「真实语义」路径使用：
//! 在线时调本地（Ollama bge-m3 等）或远端嵌入模型，离线/未配置时调用方回退到
//! `runtime::hash_embed`（词袋级 lexical）。见 `CUR-MEM-ARCH-001` P1.5（同接口替换）。
//!
//! 约束：一个记忆索引固定一个 embedding 模型（维度一致、同语义空间）；换模型需重建索引。

use serde::{Deserialize, Serialize};

use crate::error::ApiError;

#[derive(Serialize)]
struct EmbeddingsRequest<'a> {
    model: &'a str,
    input: &'a [String],
}

#[derive(Deserialize)]
struct EmbeddingsResponse {
    data: Vec<EmbeddingData>,
}

#[derive(Deserialize)]
struct EmbeddingData {
    embedding: Vec<f32>,
}

/// 调用 `{base_url}/embeddings`，返回与 `texts` 等长的向量列表。
///
/// `base_url` 形如 `http://127.0.0.1:11434/v1`（Ollama / OpenAI 兼容端点）。
/// 空输入直接返回空；非 2xx 返回 `ApiError::Api`；网络/解析错误经 `?` 转 `ApiError`。
pub async fn embed_texts(
    base_url: &str,
    api_key: &str,
    model: &str,
    texts: &[String],
) -> Result<Vec<Vec<f32>>, ApiError> {
    if texts.is_empty() {
        return Ok(Vec::new());
    }
    let url = format!("{}/embeddings", base_url.trim_end_matches('/'));
    let response = reqwest::Client::new()
        .post(&url)
        .bearer_auth(api_key)
        .json(&EmbeddingsRequest {
            model,
            input: texts,
        })
        .send()
        .await?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(ApiError::Api {
            status,
            error_type: None,
            message: None,
            body,
            retryable: status.is_server_error(),
        });
    }
    let parsed: EmbeddingsResponse = response.json().await?;
    Ok(parsed.data.into_iter().map(|item| item.embedding).collect())
}

#[cfg(test)]
mod tests {
    use super::{EmbeddingsRequest, EmbeddingsResponse};

    #[test]
    fn request_serializes_with_model_and_input() {
        let body = serde_json::to_value(EmbeddingsRequest {
            model: "bge-m3",
            input: &["hi".to_string()],
        })
        .expect("serialize");
        assert_eq!(body["model"], "bge-m3");
        assert_eq!(body["input"][0], "hi");
    }

    #[test]
    fn response_parses_embedding_data() {
        let json = r#"{"data":[{"embedding":[0.1,0.2,0.3]}],"model":"bge-m3"}"#;
        let parsed: EmbeddingsResponse = serde_json::from_str(json).expect("parse");
        assert_eq!(parsed.data.len(), 1);
        assert_eq!(parsed.data[0].embedding, vec![0.1, 0.2, 0.3]);
    }
}
