use serde::{Deserialize, Serialize};

/// 生成参数（采样参数）。与「模型连接配置」（provider/地址/密钥/模型 id）分开：
/// 连接配置回答「连哪」，本对象回答「怎么生成」。
///
/// 每个字段都是 Option，`None` 表示「本层不覆盖」，交给下一层或应用默认值决定。
/// 三级覆盖顺序：应用默认 → 世界 → 会话（`resolve` 逐层 merge）。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct GenerationParams {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top_k: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<i32>,
    /// 停止词。空数组与 None 等价（都表示不设停止词）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub presence_penalty: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frequency_penalty: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seed: Option<i64>,
}

/// 生成参数的调用角色。宿主对两类调用有不同的内置默认值（历史行为：导演 0.7、角色 0.8），
/// 三级覆盖都在这个默认值之上叠加。
pub const GENERATION_ROLE_DIRECTOR: &str = "director";
pub const GENERATION_ROLE_CHARACTER: &str = "character";
/// 世界生成器等宿主辅助调用，与游戏内回合无关，只吃应用默认。
pub const GENERATION_ROLE_UTILITY: &str = "utility";

impl GenerationParams {
    /// 各角色的内置默认值。改造前这些值硬编码在 director.rs / turn_context.rs，
    /// 现在收敛到一处，作为三级覆盖的最底层。
    pub fn builtin_default_for_role(role: &str) -> GenerationParams {
        let temperature = match role {
            GENERATION_ROLE_CHARACTER => 0.8,
            _ => 0.7,
        };
        GenerationParams {
            temperature: Some(temperature),
            ..Default::default()
        }
    }

    /// 从 JSON（世界 director_config、DB 列）宽松解析：结构不对就当「本层没配」，
    /// 不让一处手写坏了的配置阻断整局游戏。
    pub fn from_json(value: Option<&serde_json::Value>) -> Option<GenerationParams> {
        let value = value?;
        if value.is_null() {
            return None;
        }
        let parsed = serde_json::from_value::<GenerationParams>(value.clone()).ok()?;
        (!parsed.is_empty()).then_some(parsed)
    }

    /// 从 DB 里存的 JSON 文本解析，空串/坏数据都视作「本层没配」。
    pub fn from_json_text(raw: &str) -> Option<GenerationParams> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return None;
        }
        Self::from_json(serde_json::from_str::<serde_json::Value>(trimmed).ok().as_ref())
    }

    /// 所有字段都没配（该层不产生任何覆盖）。
    pub fn is_empty(&self) -> bool {
        *self == GenerationParams::default()
    }

    /// 用 `over` 覆盖 self：`over` 里有值的字段生效，`None` 的字段保留 self 的值。
    pub fn merge(&self, over: &GenerationParams) -> GenerationParams {
        GenerationParams {
            temperature: over.temperature.or(self.temperature),
            top_p: over.top_p.or(self.top_p),
            top_k: over.top_k.or(self.top_k),
            max_tokens: over.max_tokens.or(self.max_tokens),
            stop: over.stop.clone().or_else(|| self.stop.clone()),
            presence_penalty: over.presence_penalty.or(self.presence_penalty),
            frequency_penalty: over.frequency_penalty.or(self.frequency_penalty),
            seed: over.seed.or(self.seed),
        }
    }

    /// 完整解析链：内置角色默认 → 应用 → 世界 → 会话。任一层缺省即跳过。
    pub fn resolve_for_role(
        role: &str,
        app: &GenerationParams,
        world: Option<&GenerationParams>,
        session: Option<&GenerationParams>,
    ) -> GenerationParams {
        let mut result = Self::builtin_default_for_role(role).merge(app);
        if let Some(world) = world {
            result = result.merge(world);
        }
        if let Some(session) = session {
            result = result.merge(session);
        }
        result
    }

    /// 落进合法区间。超界的值夹紧而不是丢弃——用户填了 5.0 说明想要更随机，
    /// 静默丢掉不如夹到上限；同时把夹紧动作记进 notes 供 UI 展示。
    pub fn sanitized(&self) -> (GenerationParams, Vec<String>) {
        let mut notes = Vec::new();
        let mut out = self.clone();
        let mut clamp_f64 = |value: &mut Option<f64>, lo: f64, hi: f64, name: &str| {
            if let Some(current) = *value {
                let fixed = current.clamp(lo, hi);
                if (fixed - current).abs() > f64::EPSILON {
                    notes.push(format!("{name} {current} 超出 [{lo}, {hi}]，已夹紧为 {fixed}"));
                    *value = Some(fixed);
                }
            }
        };
        clamp_f64(&mut out.temperature, 0.0, 2.0, PARAM_TEMPERATURE);
        clamp_f64(&mut out.top_p, 0.0, 1.0, PARAM_TOP_P);
        clamp_f64(&mut out.presence_penalty, -2.0, 2.0, PARAM_PRESENCE_PENALTY);
        clamp_f64(&mut out.frequency_penalty, -2.0, 2.0, PARAM_FREQUENCY_PENALTY);
        if let Some(top_k) = out.top_k {
            if top_k < 1 {
                notes.push(format!("{PARAM_TOP_K} {top_k} 无效，已忽略"));
                out.top_k = None;
            }
        }
        if let Some(max_tokens) = out.max_tokens {
            if max_tokens < 1 {
                notes.push(format!("{PARAM_MAX_TOKENS} {max_tokens} 无效，已忽略"));
                out.max_tokens = None;
            }
        }
        // 空停止词数组对各家 API 都无意义，且 Anthropic 会因空数组报错。
        if out.stop.as_ref().is_some_and(|items| items.is_empty()) {
            out.stop = None;
        }
        (out, notes)
    }
}

/// 参数名常量，用于过滤报告与前端展示，避免各处手写字符串。
pub const PARAM_TEMPERATURE: &str = "temperature";
pub const PARAM_TOP_P: &str = "top_p";
pub const PARAM_TOP_K: &str = "top_k";
pub const PARAM_MAX_TOKENS: &str = "max_tokens";
pub const PARAM_STOP: &str = "stop";
pub const PARAM_PRESENCE_PENALTY: &str = "presence_penalty";
pub const PARAM_FREQUENCY_PENALTY: &str = "frequency_penalty";
pub const PARAM_SEED: &str = "seed";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn three_level_override_takes_the_innermost_value_per_field() {
        let app = GenerationParams {
            temperature: Some(0.5),
            top_p: Some(0.9),
            max_tokens: Some(1000),
            ..Default::default()
        };
        let world = GenerationParams {
            temperature: Some(1.1),
            seed: Some(42),
            ..Default::default()
        };
        let session = GenerationParams {
            temperature: Some(0.2),
            ..Default::default()
        };

        let resolved = GenerationParams::resolve_for_role(
            GENERATION_ROLE_CHARACTER,
            &app,
            Some(&world),
            Some(&session),
        );

        assert_eq!(resolved.temperature, Some(0.2), "会话层优先级最高");
        assert_eq!(resolved.seed, Some(42), "会话没配 seed 时沿用世界层");
        assert_eq!(resolved.top_p, Some(0.9), "世界与会话都没配时沿用应用层");
        assert_eq!(resolved.max_tokens, Some(1000));
    }

    #[test]
    fn builtin_role_defaults_survive_when_no_layer_configures_anything() {
        // 改造前硬编码的 0.7 / 0.8 现在是内置默认：三层全空时行为必须与改造前一致。
        let director = GenerationParams::resolve_for_role(
            GENERATION_ROLE_DIRECTOR,
            &GenerationParams::default(),
            None,
            None,
        );
        let character = GenerationParams::resolve_for_role(
            GENERATION_ROLE_CHARACTER,
            &GenerationParams::default(),
            None,
            None,
        );

        assert_eq!(director.temperature, Some(0.7));
        assert_eq!(character.temperature, Some(0.8));
        assert!(director.top_p.is_none(), "没配的参数不该凭空出现");
    }

    #[test]
    fn out_of_range_values_are_clamped_with_a_visible_note() {
        let (sanitized, notes) = GenerationParams {
            temperature: Some(5.0),
            top_p: Some(-1.0),
            max_tokens: Some(0),
            stop: Some(vec![]),
            ..Default::default()
        }
        .sanitized();

        assert_eq!(sanitized.temperature, Some(2.0));
        assert_eq!(sanitized.top_p, Some(0.0));
        assert_eq!(sanitized.max_tokens, None, "非法 max_tokens 被忽略而非发送");
        assert_eq!(sanitized.stop, None, "空停止词数组等价于没配");
        assert_eq!(notes.len(), 3, "夹紧/忽略都要留下可展示的说明");
        assert!(notes.iter().any(|note| note.contains("temperature")));
    }

    #[test]
    fn empty_and_broken_json_layers_are_treated_as_no_override() {
        assert!(GenerationParams::from_json_text("").is_none());
        assert!(GenerationParams::from_json_text("{}").is_none(), "空对象即不覆盖");
        assert!(GenerationParams::from_json_text("not json").is_none());
        assert!(
            GenerationParams::from_json(Some(&serde_json::json!({ "temperature": "热一点" })))
                .is_none(),
            "类型不对的世界包配置不该让整局游戏失败"
        );
        assert_eq!(
            GenerationParams::from_json(Some(&serde_json::json!({ "temperature": 1.3 })))
                .and_then(|params| params.temperature),
            Some(1.3)
        );
    }
}
