use reqwest::Client;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageRequest {
    pub prompt: String,
    pub negative_prompt: Option<String>,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub steps: Option<i32>,
    pub cfg_scale: Option<f64>,
    pub seed: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageResponse {
    pub image_data: Vec<u8>,
    pub format: String,
    pub seed: Option<i32>,
}

pub struct ImageGenerator {
    http_client: Client,
}

impl ImageGenerator {
    pub fn new() -> Self {
        Self {
            http_client: Client::new(),
        }
    }

    pub async fn generate(
        &self,
        provider: &str,
        base_url: &str,
        api_key: &str,
        model_id: Option<&str>,
        request: &ImageRequest,
    ) -> Result<ImageResponse, String> {
        match normalize_provider(provider).as_str() {
            "automatic1111" | "stable-diffusion" => {
                self.generate_automatic1111(base_url, request).await
            }
            "openai" => {
                self.generate_openai(base_url, api_key, model_id, request)
                    .await
            }
            _ => Err(format!("Unsupported image provider: {}", provider)),
        }
    }

    async fn generate_automatic1111(
        &self,
        base_url: &str,
        request: &ImageRequest,
    ) -> Result<ImageResponse, String> {
        let url = format!("{}/sdapi/v1/txt2img", base_url.trim_end_matches('/'));

        let payload = serde_json::json!({
            "prompt": request.prompt,
            "negative_prompt": request.negative_prompt.as_deref().unwrap_or(""),
            "width": request.width.unwrap_or(512),
            "height": request.height.unwrap_or(512),
            "steps": request.steps.unwrap_or(20),
            "cfg_scale": request.cfg_scale.unwrap_or(7.0),
            "seed": request.seed.unwrap_or(-1),
        });

        let response = self
            .http_client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&payload)
            .send()
            .await
            .map_err(|e| format!("HTTP request failed: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(format!("API error {}: {}", status, body));
        }

        let result: serde_json::Value = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse response: {}", e))?;

        let images = result["images"]
            .as_array()
            .ok_or_else(|| "No images in response".to_string())?;

        let image_b64 = images
            .first()
            .and_then(|v| v.as_str())
            .ok_or_else(|| "Empty image data".to_string())?;

        use base64::Engine;
        let image_data = base64::engine::general_purpose::STANDARD
            .decode(image_b64)
            .map_err(|e| format!("Failed to decode image: {}", e))?;

        let seed = result["parameters"]["seed"].as_i64().map(|v| v as i32);

        Ok(ImageResponse {
            image_data,
            format: "png".to_string(),
            seed,
        })
    }

    async fn generate_openai(
        &self,
        base_url: &str,
        api_key: &str,
        model_id: Option<&str>,
        request: &ImageRequest,
    ) -> Result<ImageResponse, String> {
        let url = format!("{}/images/generations", base_url.trim_end_matches('/'));
        let model_name = model_id
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("gpt-image-1");

        let payload = serde_json::json!({
            "model": model_name,
            "prompt": request.prompt,
            "n": 1,
            "size": format!("{}x{}", request.width.unwrap_or(1024), request.height.unwrap_or(1024)),
            "response_format": "b64_json",
        });

        let response = self
            .http_client
            .post(&url)
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json")
            .json(&payload)
            .send()
            .await
            .map_err(|e| format!("HTTP request failed: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(format!("API error {}: {}", status, body));
        }

        let result: serde_json::Value = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse response: {}", e))?;

        let image_b64 = result["data"][0]["b64_json"]
            .as_str()
            .ok_or_else(|| "No image data in response".to_string())?;

        use base64::Engine;
        let image_data = base64::engine::general_purpose::STANDARD
            .decode(image_b64)
            .map_err(|e| format!("Failed to decode image: {}", e))?;

        Ok(ImageResponse {
            image_data,
            format: "png".to_string(),
            seed: None,
        })
    }
}

fn normalize_provider(provider: &str) -> String {
    match provider.trim().to_ascii_lowercase().as_str() {
        "openai-compatible" | "openai compatible" | "openai" | "gpt-image2" | "nanp banana2"
        | "google nano banana" => "openai".to_string(),
        "automatic1111" | "a1111" | "stable-diffusion" => "automatic1111".to_string(),
        other => other.to_string(),
    }
}
