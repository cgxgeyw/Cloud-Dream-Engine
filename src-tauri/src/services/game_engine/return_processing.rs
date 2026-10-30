use serde_json::Value;

/// Apply the world's configured return-processing rules to one model return.
///
/// `scope` is the execution path (`director` or `character`). Rules marked
/// `both` apply to either path; rules for the other path are ignored.
pub(crate) fn apply_return_processing(
    director_config: &Value,
    raw_text: &str,
    scope: &str,
) -> Value {
    let mut text = raw_text.to_string();
    let mut applied_rules = Vec::new();
    let mut rules = director_config
        .get("return_processing_rules")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|item| {
            let object = item.as_object()?;
            let enabled = object
                .get("enabled")
                .and_then(Value::as_bool)
                .unwrap_or(true);
            let rule_scope = object
                .get("scope")
                .and_then(Value::as_str)
                .unwrap_or("both")
                .trim();
            let pattern = object
                .get("pattern")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let replacement = object
                .get("replacement")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let order = object.get("order").and_then(Value::as_i64).unwrap_or(0);
            let name = object
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("unnamed_rule")
                .to_string();
            if !enabled
                || !(rule_scope == "both" || rule_scope == scope)
                || pattern.trim().is_empty()
            {
                return None;
            }
            Some((order, name, pattern, replacement))
        })
        .collect::<Vec<_>>();
    rules.sort_by_key(|(order, _, _, _)| *order);

    for (_, name, pattern, replacement) in rules {
        match regex::Regex::new(&pattern) {
            Ok(re) => {
                let count = re.find_iter(&text).count();
                if count > 0 {
                    text = re.replace_all(&text, replacement.as_str()).to_string();
                    applied_rules.push(serde_json::json!({
                        "name": name,
                        "pattern": pattern,
                        "replacement": replacement,
                        "count": count,
                    }));
                }
            }
            Err(err) => {
                applied_rules.push(serde_json::json!({
                    "name": name,
                    "pattern": pattern,
                    "error": err.to_string(),
                    "count": 0,
                }));
            }
        }
    }

    serde_json::json!({
        "before": raw_text,
        "after": text,
        "applied_rules": applied_rules,
    })
}

#[cfg(test)]
mod tests {
    use super::apply_return_processing;

    #[test]
    fn applies_only_matching_scope_and_both_rules() {
        let config = serde_json::json!({
            "return_processing_rules": [
                { "name": "director", "scope": "director", "pattern": "A", "replacement": "D" },
                { "name": "character", "scope": "character", "pattern": "A", "replacement": "C" },
                { "name": "both", "scope": "both", "pattern": "B", "replacement": "X" }
            ]
        });
        let result = apply_return_processing(&config, "AB", "character");
        assert_eq!(result["after"], "CX");
        assert_eq!(result["applied_rules"].as_array().unwrap().len(), 2);
    }
}
