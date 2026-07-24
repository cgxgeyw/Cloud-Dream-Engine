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

enum OpenAiImageSource {
    Base64(String),
    Url(String),
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
            _ => {
                self.generate_openai(base_url, api_key, model_id, request)
                    .await
            }
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

        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or("unknown")
            .to_string();
        let body = response
            .text()
            .await
            .map_err(|e| format!("Failed to read image API response: {}", e))?;
        let result: serde_json::Value = serde_json::from_str(&body).map_err(|_| {
            format!(
                "Automatic1111 API returned {} instead of JSON.",
                content_type
            )
        })?;

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

        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or("unknown")
            .to_string();
        let body = response
            .text()
            .await
            .map_err(|e| format!("Failed to read image API response: {}", e))?;
        let result: serde_json::Value = serde_json::from_str(&body).map_err(|_| {
            format!(
                "Image API returned {} instead of JSON. Check that the Base URL includes the API version, for example /v1.",
                content_type
            )
        })?;

        let (image_data, format) = match openai_image_source(&result)? {
            OpenAiImageSource::Base64(image_b64) => {
                use base64::Engine;
                let image_data = base64::engine::general_purpose::STANDARD
                    .decode(image_b64)
                    .map_err(|e| format!("Failed to decode image: {}", e))?;
                (image_data, "png".to_string())
            }
            OpenAiImageSource::Url(image_url) => self.download_generated_image(&image_url).await?,
        };

        Ok(ImageResponse {
            image_data,
            format,
            seed: None,
        })
    }

    async fn download_generated_image(&self, image_url: &str) -> Result<(Vec<u8>, String), String> {
        let response = self
            .http_client
            .get(image_url)
            .send()
            .await
            .map_err(|e| format!("Failed to download generated image: {}", e))?;
        if !response.status().is_success() {
            return Err(format!(
                "Generated image download returned HTTP {}",
                response.status()
            ));
        }
        let format = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .map(image_format_from_content_type)
            .unwrap_or("png")
            .to_string();
        let image_data = response
            .bytes()
            .await
            .map_err(|e| format!("Failed to read generated image: {}", e))?
            .to_vec();
        Ok((image_data, format))
    }
}

fn openai_image_source(result: &serde_json::Value) -> Result<OpenAiImageSource, String> {
    let image = result
        .get("data")
        .and_then(serde_json::Value::as_array)
        .and_then(|images| images.first())
        .ok_or_else(|| "No image data in response".to_string())?;
    if let Some(image_b64) = image.get("b64_json").and_then(serde_json::Value::as_str) {
        return Ok(OpenAiImageSource::Base64(image_b64.to_string()));
    }
    if let Some(image_url) = image.get("url").and_then(serde_json::Value::as_str) {
        return Ok(OpenAiImageSource::Url(image_url.to_string()));
    }
    Err("No image data in response; expected data[0].b64_json or data[0].url".to_string())
}

fn image_format_from_content_type(content_type: &str) -> &str {
    match content_type.split(';').next().unwrap_or("").trim() {
        "image/jpeg" => "jpg",
        "image/webp" => "webp",
        _ => "png",
    }
}

pub(crate) fn preferred_image_size(model_id: &str, width: i32, height: i32) -> (i32, i32) {
    if model_id.trim().to_ascii_lowercase().contains("-1k") {
        (1024, 1024)
    } else {
        (width, height)
    }
}

pub(crate) fn normalize_provider(provider: &str) -> String {
    match provider.trim().to_ascii_lowercase().as_str() {
        "openai-compatible" | "openai compatible" | "openai" | "gpt-image2" | "nanp banana2"
        | "google nano banana" => "openai".to_string(),
        "automatic1111" | "a1111" | "stable-diffusion" => "automatic1111".to_string(),
        // Custom image endpoints use the OpenAI-compatible Images API by default.
        _ => "openai".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::{normalize_provider, openai_image_source, preferred_image_size, OpenAiImageSource};
    use serde_json::json;

    #[test]
    fn normalizes_custom_image_provider_to_openai_compatible() {
        assert_eq!(normalize_provider("123"), "openai");
        assert_eq!(normalize_provider("my-image-proxy"), "openai");
    }

    #[test]
    fn preserves_automatic1111_as_a_special_protocol() {
        assert_eq!(normalize_provider("automatic1111"), "automatic1111");
    }

    #[test]
    fn accepts_openai_compatible_url_image_responses() {
        let result = json!({ "data": [{ "url": "https://example.test/image.png" }] });
        let source = openai_image_source(&result).expect("image source");
        assert!(
            matches!(source, OpenAiImageSource::Url(url) if url == "https://example.test/image.png")
        );
    }

    #[test]
    fn accepts_openai_compatible_base64_image_responses() {
        let result = json!({ "data": [{ "b64_json": "aGVsbG8=" }] });
        let source = openai_image_source(&result).expect("image source");
        assert!(matches!(source, OpenAiImageSource::Base64(value) if value == "aGVsbG8="));
    }

    #[test]
    fn uses_square_size_for_one_k_models() {
        assert_eq!(
            preferred_image_size("firefly-gpt-image-2-1k", 1536, 1024),
            (1024, 1024)
        );
        assert_eq!(
            preferred_image_size("gpt-image-2", 1536, 1024),
            (1536, 1024)
        );
    }
}
