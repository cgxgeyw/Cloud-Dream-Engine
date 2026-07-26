use crate::models::generation_params::*;
use serde::{Deserialize, Serialize};

/// 一个被丢弃的参数及原因。要求「被过滤并可见，而不是静默发送」，
/// 所以过滤结果必须能回传给调用方、进调试视图。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DroppedParam {
    pub name: String,
    pub reason: String,
}

/// 过滤结果：可发送的参数 + 被丢弃的清单 + 夹紧等提示。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FilteredParams {
    pub effective: GenerationParams,
    pub dropped: Vec<DroppedParam>,
    pub notes: Vec<String>,
}

/// 给回合 trace 用的采样参数描述：过滤前的解析结果 + 实际发出的 + 被丢弃的及原因。
/// 「不支持的参数被过滤并可见，而不是静默发送」这条验收标准就靠这里落在调试视图上。
pub fn describe_params_for_trace(
    provider: &str,
    requested: &GenerationParams,
    extra: serde_json::Value,
) -> serde_json::Value {
    let filtered = filter_for_provider(provider, requested);
    merge_extra(
        serde_json::json!({
            "provider": super::normalize_provider(provider),
            // 三级覆盖解析后的期望值（含本层没被 provider 接受的项）。
            "requested": requested,
            // 真正进入请求体的参数。
            "effective": filtered.effective,
            // 被摘掉的参数及原因，空数组表示全部下发。
            "dropped": filtered.dropped,
            // 夹紧等调整说明。
            "notes": filtered.notes,
        }),
        extra,
    )
}

/// 预览场景（还不知道会用哪个模型/provider）下的描述：只报解析结果，不谎报过滤结论。
pub fn describe_params_without_provider(
    requested: &GenerationParams,
    extra: serde_json::Value,
) -> serde_json::Value {
    let (sanitized, notes) = requested.sanitized();
    merge_extra(
        serde_json::json!({
            "requested": sanitized,
            "notes": notes,
            "provider_filter": "未定（预览阶段还没选定模型，过滤结论见实际回合的调试视图）",
        }),
        extra,
    )
}

fn merge_extra(mut value: serde_json::Value, extra: serde_json::Value) -> serde_json::Value {
    if let (Some(target), Some(source)) = (value.as_object_mut(), extra.as_object()) {
        for (key, item) in source {
            target.insert(key.clone(), item.clone());
        }
    }
    value
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all_params() -> GenerationParams {
        GenerationParams {
            temperature: Some(0.9),
            top_p: Some(0.8),
            top_k: Some(40),
            max_tokens: Some(800),
            stop: Some(vec!["###".to_string()]),
            presence_penalty: Some(0.3),
            frequency_penalty: Some(0.4),
            seed: Some(7),
        }
    }

    #[test]
    fn openai_compatible_endpoints_drop_top_k_with_a_reason() {
        let filtered = filter_for_provider("openai", &all_params());

        assert!(filtered.effective.top_k.is_none());
        assert_eq!(filtered.effective.seed, Some(7));
        assert_eq!(filtered.effective.presence_penalty, Some(0.3));
        let dropped = filtered
            .dropped
            .iter()
            .map(|item| item.name.as_str())
            .collect::<Vec<_>>();
        assert_eq!(dropped, vec![PARAM_TOP_K]);
        assert!(
            filtered.dropped[0].reason.contains("top_k"),
            "丢弃原因要写清是哪个参数，供调试视图展示"
        );
    }

    #[test]
    fn anthropic_drops_penalties_and_seed_but_keeps_top_k() {
        let filtered = filter_for_provider("claude", &all_params());

        assert_eq!(filtered.effective.top_k, Some(40));
        assert_eq!(filtered.effective.stop, Some(vec!["###".to_string()]));
        assert!(filtered.effective.presence_penalty.is_none());
        assert!(filtered.effective.frequency_penalty.is_none());
        assert!(filtered.effective.seed.is_none());
        let mut dropped = filtered
            .dropped
            .iter()
            .map(|item| item.name.clone())
            .collect::<Vec<_>>();
        dropped.sort();
        assert_eq!(
            dropped,
            vec![PARAM_FREQUENCY_PENALTY, PARAM_PRESENCE_PENALTY, PARAM_SEED]
        );
    }

    #[test]
    fn params_that_were_never_configured_are_not_reported_as_dropped() {
        // 「没配」和「配了但不支持」必须区分开，否则调试视图会满屏噪音。
        let filtered = filter_for_provider(
            "anthropic",
            &GenerationParams {
                temperature: Some(0.6),
                ..Default::default()
            },
        );

        assert_eq!(filtered.effective.temperature, Some(0.6));
        assert!(filtered.dropped.is_empty());
    }

    #[test]
    fn trace_description_reports_requested_effective_and_dropped_together() {
        let described = describe_params_for_trace(
            "anthropic",
            &all_params(),
            serde_json::json!({ "json_mode": true }),
        );

        assert_eq!(described["provider"], "anthropic");
        assert_eq!(described["json_mode"], true);
        assert_eq!(described["requested"]["seed"], 7, "期望值仍可见");
        assert!(
            described["effective"].get("seed").is_none(),
            "实际发出的请求里不带 seed"
        );
        assert_eq!(described["dropped"].as_array().map(Vec::len), Some(3));
    }

    #[test]
    fn preview_description_does_not_claim_a_filter_verdict() {
        let described = describe_params_without_provider(
            &GenerationParams {
                temperature: Some(3.0),
                ..Default::default()
            },
            serde_json::json!({}),
        );

        assert_eq!(described["requested"]["temperature"], 2.0, "预览也要夹紧");
        assert!(described.get("dropped").is_none());
        assert!(described["provider_filter"].is_string());
    }
}

/// 某 provider 支持的参数集合。这里按「我们实际发出去的请求体字段」列，
/// 不按各家文档的全集——发不出去的参数支持了也没用。
fn supported_params(provider: &str) -> &'static [&'static str] {
    match provider {
        // OpenAI 兼容端点（含 ollama / lmstudio 的兼容层）。
        // 兼容层不支持 top_k：那是 ollama 原生 /api/generate 的参数，
        // 走 /v1/chat/completions 时会被忽略。
        "openai" | "ollama" | "lmstudio" => &[
            PARAM_TEMPERATURE,
            PARAM_TOP_P,
            PARAM_MAX_TOKENS,
            PARAM_STOP,
            PARAM_PRESENCE_PENALTY,
            PARAM_FREQUENCY_PENALTY,
            PARAM_SEED,
        ],
        // Anthropic 有 top_k，但没有 presence/frequency penalty 和 seed。
        "anthropic" => &[
            PARAM_TEMPERATURE,
            PARAM_TOP_P,
            PARAM_TOP_K,
            PARAM_MAX_TOKENS,
            PARAM_STOP,
        ],
        _ => &[PARAM_TEMPERATURE, PARAM_MAX_TOKENS],
    }
}

fn unsupported_reason(provider: &str, param: &str) -> String {
    match (provider, param) {
        (_, PARAM_TOP_K) if provider != "anthropic" => {
            format!("{provider} 的 OpenAI 兼容端点不支持 {param}")
        }
        ("anthropic", PARAM_PRESENCE_PENALTY | PARAM_FREQUENCY_PENALTY) => {
            format!("anthropic 不支持 {param}（无对应惩罚参数）")
        }
        ("anthropic", PARAM_SEED) => "anthropic 不支持 seed（无法固定随机种子）".to_string(),
        _ => format!("{provider} 不支持 {param}"),
    }
}

/// 按 provider 过滤生成参数：不支持的参数被摘掉并记录原因。
pub fn filter_for_provider(provider: &str, params: &GenerationParams) -> FilteredParams {
    let normalized = super::normalize_provider(provider);
    let supported = supported_params(normalized.as_str());
    let (sanitized, notes) = params.sanitized();
    let mut effective = GenerationParams::default();
    let mut dropped = Vec::new();

    let mut keep = |name: &str, present: bool| -> bool {
        if !present {
            return false;
        }
        if supported.contains(&name) {
            true
        } else {
            dropped.push(DroppedParam {
                name: name.to_string(),
                reason: unsupported_reason(normalized.as_str(), name),
            });
            false
        }
    };

    if keep(PARAM_TEMPERATURE, sanitized.temperature.is_some()) {
        effective.temperature = sanitized.temperature;
    }
    if keep(PARAM_TOP_P, sanitized.top_p.is_some()) {
        effective.top_p = sanitized.top_p;
    }
    if keep(PARAM_TOP_K, sanitized.top_k.is_some()) {
        effective.top_k = sanitized.top_k;
    }
    if keep(PARAM_MAX_TOKENS, sanitized.max_tokens.is_some()) {
        effective.max_tokens = sanitized.max_tokens;
    }
    if keep(PARAM_STOP, sanitized.stop.is_some()) {
        effective.stop = sanitized.stop.clone();
    }
    if keep(PARAM_PRESENCE_PENALTY, sanitized.presence_penalty.is_some()) {
        effective.presence_penalty = sanitized.presence_penalty;
    }
    if keep(
        PARAM_FREQUENCY_PENALTY,
        sanitized.frequency_penalty.is_some(),
    ) {
        effective.frequency_penalty = sanitized.frequency_penalty;
    }
    if keep(PARAM_SEED, sanitized.seed.is_some()) {
        effective.seed = sanitized.seed;
    }

    FilteredParams {
        effective,
        dropped,
        notes,
    }
}
