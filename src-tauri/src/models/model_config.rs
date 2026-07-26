use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    pub id: String,
    pub name: String,
    pub model_type: String,
    pub provider: String,
    pub model_id: String,
    pub base_url: String,
    pub api_key: String,
    pub max_tokens: i32,
    pub streaming_enabled: bool,
    pub is_default: bool,
    /// 该模型声明支持的输入模态（"image" / "audio"）。空 = 仅文本。
    /// 玩家附件只发给声明了对应模态的模型，否则提交时明确报错（第 10 项）。
    #[serde(default)]
    pub input_modalities: Vec<String>,
}

impl ModelConfig {
    pub fn supports_modality(&self, modality: &str) -> bool {
        self.input_modalities
            .iter()
            .any(|value| value.trim().eq_ignore_ascii_case(modality))
    }
}

/// 第 10 项：玩家附件（图片/音频）只允许发给声明了对应输入模态的模型。
/// 不支持时给出中文错误，指明模型名、缺的模态与开启入口——不静默丢弃附件（红线 5）。
pub fn ensure_media_supported(
    model: &ModelConfig,
    media: &[crate::models::session::ContentPart],
) -> Result<(), String> {
    let mut unsupported: Vec<&str> = Vec::new();
    for part in media {
        let (modality, label) = match part.part_type.as_str() {
            "image_url" => ("image", "图片"),
            "input_audio" => ("audio", "语音"),
            _ => continue,
        };
        if !model.supports_modality(modality) && !unsupported.contains(&label) {
            unsupported.push(label);
        }
    }
    if unsupported.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "模型「{}」不支持{}输入：请在 设置 → 模型 中为它开启对应输入模态，或移除附件后重试。",
            model.name,
            unsupported.join("和")
        ))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfigCreateRequest {
    pub name: String,
    pub model_type: String,
    pub provider: String,
    pub model_id: String,
    pub base_url: String,
    pub api_key: String,
    pub max_tokens: i32,
    pub streaming_enabled: bool,
    pub is_default: bool,
    #[serde(default)]
    pub input_modalities: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ModelConfigUpdateRequest {
    pub name: Option<String>,
    pub model_type: Option<String>,
    pub provider: Option<String>,
    pub model_id: Option<String>,
    pub base_url: Option<String>,
    pub api_key: Option<String>,
    pub max_tokens: Option<i32>,
    pub streaming_enabled: Option<bool>,
    pub is_default: Option<bool>,
    pub input_modalities: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelTestResponse {
    pub ok: bool,
    pub detail: String,
    pub debug_lines: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageModelTestRequest {
    pub prompt: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageModelTestResponse {
    pub ok: bool,
    pub detail: String,
    pub debug_lines: Vec<String>,
    pub asset_path: Option<String>,
    pub image_url: Option<String>,
    pub seed: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelDiscoverRequest {
    pub provider: String,
    pub base_url: String,
    pub api_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelDiscoverResponse {
    pub ok: bool,
    pub detail: String,
    pub model_ids: Vec<String>,
    pub debug_lines: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingModelFileStatus {
    pub name: String,
    pub relative_path: String,
    pub exists: bool,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingModelStatus {
    pub model_id: String,
    pub display_name: String,
    pub installed: bool,
    pub detail: String,
    pub local_dir: String,
    pub total_size_bytes: u64,
    pub files: Vec<EmbeddingModelFileStatus>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::session::{ContentPart, ImageUrl};

    fn model(modalities: &[&str]) -> ModelConfig {
        ModelConfig {
            id: "m1".to_string(),
            name: "测试模型".to_string(),
            model_type: "text".to_string(),
            provider: "openai".to_string(),
            model_id: "gpt-test".to_string(),
            base_url: String::new(),
            api_key: String::new(),
            max_tokens: 1200,
            streaming_enabled: true,
            is_default: false,
            input_modalities: modalities.iter().map(|value| value.to_string()).collect(),
        }
    }

    fn image_part() -> ContentPart {
        ContentPart {
            part_type: "image_url".to_string(),
            text: None,
            image_url: Some(ImageUrl {
                url: "data:image/png;base64,QUJD".to_string(),
            }),
            input_audio: None,
        }
    }

    #[test]
    fn media_supported_only_when_declared() {
        let media = vec![image_part()];
        assert!(ensure_media_supported(&model(&["image"]), &media).is_ok());
        assert!(ensure_media_supported(&model(&[]), &media).is_err());
        let error = ensure_media_supported(&model(&["audio"]), &media).unwrap_err();
        assert!(error.contains("测试模型"), "错误应点名模型: {error}");
        assert!(error.contains("图片"), "错误应指出缺的模态: {error}");
        assert!(error.contains("设置"), "错误应指出开启入口: {error}");
    }
}
