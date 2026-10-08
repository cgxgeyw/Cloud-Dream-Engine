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
    /// 关闭该模型的 JSON 结构化输出（不发 `response_format=json_object`）。
    ///
    /// 部分 OpenAI 兼容模型（实测阶跃星辰 step-5-preview）在 json_object 模式下
    /// 会把字段名写坏，输出 `{"  \t":"正文…","narration":""}` 这类非法 JSON，
    /// 导致回合解析失败、界面显示「（本回合没有可显示的台词）」。
    /// 关掉后模型改走普通文本输出，靠解析器从 ```json 围栏里取结构，反而稳定。
    #[serde(default)]
    pub json_mode_disabled: bool,
}

impl ModelConfig {
    pub fn supports_modality(&self, modality: &str) -> bool {
        self.input_modalities
            .iter()
            .any(|value| value.trim().eq_ignore_ascii_case(modality))
    }
}

/// IPC 回传给前端时的 API Key 掩码前缀。与真实 key 区分，update 时识别为「保留旧值」。
pub const API_KEY_MASK_PREFIX: &str = "••••••••";

/// 列表/读取模型时脱敏：避免完整密钥经 IPC 回到前端。
pub fn mask_api_key(key: &str) -> String {
    let trimmed = key.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    if trimmed.len() <= 4 {
        API_KEY_MASK_PREFIX.to_string()
    } else {
        format!("{API_KEY_MASK_PREFIX}{}", &trimmed[trimmed.len() - 4..])
    }
}

/// 前端回传的值是否为脱敏掩码（用户未修改密钥字段）。
pub fn is_masked_api_key(value: &str) -> bool {
    let trimmed = value.trim();
    trimmed == API_KEY_MASK_PREFIX || trimmed.starts_with(API_KEY_MASK_PREFIX)
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

/// 单张图片附件的体积上限（解码后的二进制字节）。
/// 前端压缩后上限 6MB，这里放宽到 8MB 作为兜底：老版本客户端、
/// 未来的其他入口（插件/MCP）都可能绕过前端校验。
pub const MAX_IMAGE_PART_BYTES: usize = 8 * 1024 * 1024;
/// 单条语音附件的体积上限（解码后的二进制字节）。
pub const MAX_AUDIO_PART_BYTES: usize = 16 * 1024 * 1024;

fn strip_data_url_prefix(value: &str) -> &str {
    match value.split_once(',') {
        Some((prefix, rest)) if prefix.starts_with("data:") => rest,
        _ => value,
    }
}

/// 按 base64 长度反推解码后的字节数（含 padding 的粗略估算，够用于上限判断）。
fn estimated_decoded_bytes(payload: &str) -> usize {
    let trimmed = payload.trim_end_matches('=');
    trimmed.len() * 3 / 4
}

/// 用魔数判断这段解码数据是不是真实图片。
/// 只要前 12 字节，避免为了校验而解码整个（可能上百 MB 的）附件。
fn looks_like_image(bytes: &[u8]) -> bool {
    let starts_with = |magic: &[u8]| bytes.len() >= magic.len() && &bytes[..magic.len()] == magic;
    starts_with(b"\x89PNG\r\n\x1a\n")
        || starts_with(&[0xFF, 0xD8, 0xFF])
        || starts_with(b"GIF87a")
        || starts_with(b"GIF89a")
        || starts_with(b"BM")
        || starts_with(b"RIFF") && bytes.len() >= 12 && &bytes[8..12] == b"WEBP"
}

/// 附件内容校验：体积上限 + 真实类型。
///
/// 动机：文件选择器的 MIME 不可信，曾有一个 94MB 的 APK 被标成 `image/png`
/// 送进请求，132MB 的请求体让端点迟迟不返回、会话永远停在「回复中」，
/// 同时把 turn_journal 撑到上百 MB。因此这里在「发 HTTP / 写回合数据」之前
/// 就把不合法附件挡掉，而不是等到超时。
pub fn ensure_media_payload_valid(
    media: &[crate::models::session::ContentPart],
) -> Result<(), String> {
    for part in media {
        match part.part_type.as_str() {
            "image_url" => {
                let Some(image) = part.image_url.as_ref() else {
                    continue;
                };
                let payload = strip_data_url_prefix(image.url.trim());
                let size = estimated_decoded_bytes(payload);
                if size > MAX_IMAGE_PART_BYTES {
                    return Err(format!(
                        "图片附件过大（约 {}MB，上限 {}MB）：请换一张更小的图片后重试。",
                        size / 1024 / 1024,
                        MAX_IMAGE_PART_BYTES / 1024 / 1024
                    ));
                }
                // 只解前 16 个 base64 字符（12 字节）看魔数即可。
                let head_len = (payload.len().min(16) / 4) * 4;
                let head = &payload[..head_len];
                let decoded = base64::Engine::decode(
                    &base64::engine::general_purpose::STANDARD,
                    head,
                )
                .unwrap_or_default();
                if !looks_like_image(&decoded) {
                    return Err(
                        "图片附件不是有效的图片文件（可能误选了压缩包或其他格式），请重新选择后重试。"
                            .to_string(),
                    );
                }
            }
            "input_audio" => {
                let Some(audio) = part.input_audio.as_ref() else {
                    continue;
                };
                let payload = strip_data_url_prefix(audio.data.trim());
                let size = estimated_decoded_bytes(payload);
                if size > MAX_AUDIO_PART_BYTES {
                    return Err(format!(
                        "语音附件过大（约 {}MB，上限 {}MB）：请录制更短的语音后重试。",
                        size / 1024 / 1024,
                        MAX_AUDIO_PART_BYTES / 1024 / 1024
                    ));
                }
            }
            _ => {}
        }
    }
    Ok(())
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
    #[serde(default)]
    pub json_mode_disabled: bool,
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
    pub json_mode_disabled: Option<bool>,
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
            json_mode_disabled: false,
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

    #[test]
    fn api_key_is_masked_for_ipc() {
        assert_eq!(mask_api_key(""), "");
        assert_eq!(mask_api_key("abcd"), API_KEY_MASK_PREFIX);
        let masked = mask_api_key("sk-secret-key-9876");
        assert!(masked.ends_with("9876"));
        assert!(!masked.contains("secret"));
        assert!(is_masked_api_key(&masked));
        assert!(!is_masked_api_key("sk-new-key"));
        assert!(!is_masked_api_key(""));
    }
}
