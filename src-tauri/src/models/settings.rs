use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub text_model_provider: String,
    pub default_text_model: String,
    pub image_model_provider: String,
    pub default_image_workflow: String,
    pub embedding_enabled: bool,
    pub default_embedding_model: String,
    pub home_background_strategy: String,
    pub export_directory: String,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            text_model_provider: "openai".to_string(),
            default_text_model: "gpt-4".to_string(),
            image_model_provider: "automatic1111".to_string(),
            default_image_workflow: "txt2img".to_string(),
            embedding_enabled: true,
            default_embedding_model: "BAAI/bge-small-zh-v1.5".to_string(),
            home_background_strategy: String::new(),
            export_directory: String::new(),
        }
    }
}
