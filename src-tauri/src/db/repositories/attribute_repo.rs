use crate::models::attribute::*;
use rusqlite::{params, Connection};
use std::collections::HashSet;

pub struct AttributeRepository<'a> {
    conn: &'a Connection,
}

impl<'a> AttributeRepository<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn list_schemas(&self, scope: Option<&str>) -> Result<Vec<AttributeSchema>, String> {
        let mut sql = "SELECT * FROM attribute_schemas".to_string();
        if scope.is_some() {
            sql.push_str(" WHERE scope = ?1");
        }
        sql.push_str(" ORDER BY scope, key");

        let mut stmt = self.conn.prepare(&sql).map_err(|e| e.to_string())?;

        let row_to_schema = |row: &rusqlite::Row| -> rusqlite::Result<AttributeSchema> {
            Ok(AttributeSchema {
                id: row.get(0)?,
                scope: row.get(1)?,
                key: row.get(2)?,
                label: row.get(3)?,
                value_type: row.get(4)?,
                description: row.get(5)?,
                default_value: serde_json::from_str(&row.get::<_, String>(6)?).unwrap_or_default(),
                enum_options: serde_json::from_str(&row.get::<_, String>(7)?).unwrap_or_default(),
                display_policy: serde_json::from_str(&row.get::<_, String>(8)?).unwrap_or_default(),
                access_policy: serde_json::from_str(&row.get::<_, String>(9)?).unwrap_or_default(),
                mutation_policy: serde_json::from_str(&row.get::<_, String>(10)?)
                    .unwrap_or_default(),
                influence_policy: serde_json::from_str(&row.get::<_, String>(11)?)
                    .unwrap_or_default(),
                projection_policy: serde_json::from_str(&row.get::<_, String>(12)?)
                    .unwrap_or_default(),
            })
        };

        let mut schemas = Vec::new();
        match scope {
            Some(s) => {
                let rows = stmt
                    .query_map(params![s], row_to_schema)
                    .map_err(|e| e.to_string())?;
                for row in rows {
                    schemas.push(row.map_err(|e| e.to_string())?);
                }
            }
            None => {
                let rows = stmt
                    .query_map([], row_to_schema)
                    .map_err(|e| e.to_string())?;
                for row in rows {
                    schemas.push(row.map_err(|e| e.to_string())?);
                }
            }
        };

        Ok(schemas)
    }

    pub fn create_schema(
        &self,
        req: &AttributeSchemaCreateRequest,
    ) -> Result<AttributeSchema, String> {
        let id = uuid::Uuid::new_v4().to_string();
        let scope = normalize_attribute_scope(req.scope.as_str())
            .ok_or_else(|| format!("Unsupported attribute scope: {}", req.scope.trim()))?;
        let key = req.key.trim().to_string();
        let label = req.label.trim().to_string();
        let value_type = normalize_attribute_value_type(req.value_type.as_str()).ok_or_else(|| {
            format!(
                "Unsupported attribute value_type: {}",
                req.value_type.trim()
            )
        })?;
        let description = req.description.trim().to_string();
        let enum_options = normalize_list(&req.enum_options);
        validate_attribute_value(&value_type, &req.default_value)?;
        self.conn.execute(
            "INSERT INTO attribute_schemas (id, scope, key, label, value_type, description, default_value_json, enum_options_json, display_policy_json, access_policy_json, mutation_policy_json, influence_policy_json, projection_policy_json) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                id,
                scope,
                key,
                label,
                value_type,
                description,
                serde_json::to_string(&req.default_value).unwrap_or_default(),
                serde_json::to_string(&enum_options).unwrap_or_default(),
                serde_json::to_string(&req.display_policy).unwrap_or_default(),
                serde_json::to_string(&req.access_policy).unwrap_or_default(),
                serde_json::to_string(&req.mutation_policy).unwrap_or_default(),
                serde_json::to_string(&req.influence_policy).unwrap_or_default(),
                serde_json::to_string(&req.projection_policy).unwrap_or_default(),
            ],
        )
        .map_err(|e| e.to_string())?;

        Ok(AttributeSchema {
            id,
            scope,
            key: req.key.clone(),
            label: req.label.clone(),
            value_type,
            description: req.description.clone(),
            default_value: req.default_value.clone(),
            enum_options: req.enum_options.clone(),
            display_policy: req.display_policy.clone(),
            access_policy: req.access_policy.clone(),
            mutation_policy: req.mutation_policy.clone(),
            influence_policy: req.influence_policy.clone(),
            projection_policy: req.projection_policy.clone(),
        })
    }

    pub fn update_schema(
        &self,
        id: &str,
        req: &AttributeSchemaCreateRequest,
    ) -> Result<AttributeSchema, String> {
        let scope = normalize_attribute_scope(req.scope.as_str())
            .ok_or_else(|| format!("Unsupported attribute scope: {}", req.scope.trim()))?;
        let key = req.key.trim().to_string();
        let label = req.label.trim().to_string();
        let value_type = normalize_attribute_value_type(req.value_type.as_str()).ok_or_else(|| {
            format!(
                "Unsupported attribute value_type: {}",
                req.value_type.trim()
            )
        })?;
        let description = req.description.trim().to_string();
        let enum_options = normalize_list(&req.enum_options);
        validate_attribute_value(&value_type, &req.default_value)?;
        self.conn.execute(
            "UPDATE attribute_schemas SET scope = ?1, key = ?2, label = ?3, value_type = ?4, description = ?5, default_value_json = ?6, enum_options_json = ?7, display_policy_json = ?8, access_policy_json = ?9, mutation_policy_json = ?10, influence_policy_json = ?11, projection_policy_json = ?12 WHERE id = ?13",
            params![
                scope,
                key,
                label,
                value_type,
                description,
                serde_json::to_string(&req.default_value).unwrap_or_default(),
                serde_json::to_string(&enum_options).unwrap_or_default(),
                serde_json::to_string(&req.display_policy).unwrap_or_default(),
                serde_json::to_string(&req.access_policy).unwrap_or_default(),
                serde_json::to_string(&req.mutation_policy).unwrap_or_default(),
                serde_json::to_string(&req.influence_policy).unwrap_or_default(),
                serde_json::to_string(&req.projection_policy).unwrap_or_default(),
                id,
            ],
        )
        .map_err(|e| e.to_string())?;

        let mut stmt = self
            .conn
            .prepare("SELECT * FROM attribute_schemas WHERE id = ?1")
            .map_err(|e| e.to_string())?;
        let mut rows = stmt
            .query_map(params![id], |row| {
                Ok(AttributeSchema {
                    id: row.get(0)?,
                    scope: row.get(1)?,
                    key: row.get(2)?,
                    label: row.get(3)?,
                    value_type: row.get(4)?,
                    description: row.get(5)?,
                    default_value: serde_json::from_str(&row.get::<_, String>(6)?)
                        .unwrap_or_default(),
                    enum_options: serde_json::from_str(&row.get::<_, String>(7)?)
                        .unwrap_or_default(),
                    display_policy: serde_json::from_str(&row.get::<_, String>(8)?)
                        .unwrap_or_default(),
                    access_policy: serde_json::from_str(&row.get::<_, String>(9)?)
                        .unwrap_or_default(),
                    mutation_policy: serde_json::from_str(&row.get::<_, String>(10)?)
                        .unwrap_or_default(),
                    influence_policy: serde_json::from_str(&row.get::<_, String>(11)?)
                        .unwrap_or_default(),
                    projection_policy: serde_json::from_str(&row.get::<_, String>(12)?)
                        .unwrap_or_default(),
                })
            })
            .map_err(|e| e.to_string())?;

        match rows.next() {
            Some(row) => Ok(row.map_err(|e| e.to_string())?),
            None => Err("Attribute schema not found".to_string()),
        }
    }

    pub fn delete_schema(&self, id: &str) -> Result<(), String> {
        // attribute_values rows referencing this schema are removed automatically
        // via the schema_id ON DELETE CASCADE foreign key.
        let affected = self
            .conn
            .execute("DELETE FROM attribute_schemas WHERE id = ?1", params![id])
            .map_err(|e| e.to_string())?;
        if affected == 0 {
            return Err("Attribute schema not found".to_string());
        }
        Ok(())
    }

    pub fn list_values(
        &self,
        owner_type: Option<&str>,
        owner_id: Option<&str>,
        schema_id: Option<&str>,
    ) -> Result<Vec<AttributeValue>, String> {
        let mut sql = "SELECT * FROM attribute_values WHERE 1=1".to_string();
        let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
        let mut param_idx = 1;

        if let Some(ot) = owner_type {
            sql.push_str(&format!(" AND owner_type = ?{}", param_idx));
            param_values.push(Box::new(ot.to_string()));
            param_idx += 1;
        }
        if let Some(oi) = owner_id {
            sql.push_str(&format!(" AND owner_id = ?{}", param_idx));
            param_values.push(Box::new(oi.to_string()));
            param_idx += 1;
        }
        if let Some(schema) = schema_id {
            sql.push_str(&format!(" AND schema_id = ?{}", param_idx));
            param_values.push(Box::new(schema.to_string()));
        }

        let mut stmt = self.conn.prepare(&sql).map_err(|e| e.to_string())?;
        let params_refs: Vec<&dyn rusqlite::types::ToSql> =
            param_values.iter().map(|p| p.as_ref()).collect();

        let rows = stmt
            .query_map(params_refs.as_slice(), |row| {
                Ok(AttributeValue {
                    id: row.get(0)?,
                    schema_id: row.get(1)?,
                    owner_type: row.get(2)?,
                    owner_id: row.get(3)?,
                    value: serde_json::from_str(&row.get::<_, String>(4)?).unwrap_or_default(),
                    source: row.get(5)?,
                })
            })
            .map_err(|e| e.to_string())?;

        let mut values = Vec::new();
        for row in rows {
            values.push(row.map_err(|e| e.to_string())?);
        }

        Ok(values)
    }

    pub fn upsert_value(
        &self,
        req: &AttributeValueUpsertRequest,
    ) -> Result<AttributeValue, String> {
        let mut stmt = self
            .conn
            .prepare("SELECT value_type FROM attribute_schemas WHERE id = ?1")
            .map_err(|e| e.to_string())?;
        let schema_value_type: String = stmt
            .query_row(params![req.schema_id], |row| row.get(0))
            .map_err(|e| e.to_string())?;
        validate_attribute_value(&schema_value_type, &req.value)?;
        let id = uuid::Uuid::new_v4().to_string();
        self.conn.execute(
            "INSERT OR REPLACE INTO attribute_values (id, schema_id, owner_type, owner_id, value_json, source) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                id,
                req.schema_id,
                req.owner_type,
                req.owner_id,
                serde_json::to_string(&req.value).unwrap_or_default(),
                req.source,
            ],
        )
        .map_err(|e| e.to_string())?;

        Ok(AttributeValue {
            id,
            schema_id: req.schema_id.clone(),
            owner_type: req.owner_type.clone(),
            owner_id: req.owner_id.clone(),
            value: req.value.clone(),
            source: req.source.clone(),
        })
    }
}

fn validate_attribute_value(
    value_type: &str,
    value: &serde_json::Value,
) -> Result<(), String> {
    let valid = match value_type {
        ATTRIBUTE_VALUE_TYPE_TEXT => value.is_null() || value.is_string(),
        ATTRIBUTE_VALUE_TYPE_NUMBER => value.is_null() || value.is_number(),
        ATTRIBUTE_VALUE_TYPE_BOOLEAN => value.is_null() || value.is_boolean(),
        ATTRIBUTE_VALUE_TYPE_LIST => value.is_null() || value.is_array(),
        ATTRIBUTE_VALUE_TYPE_JSON => value.is_null() || value.is_object() || value.is_array(),
        _ => false,
    };
    if valid {
        Ok(())
    } else {
        Err(format!("Value does not match attribute value_type: {value_type}"))
    }
}

fn normalize_list(values: &[String]) -> Vec<String> {
    let mut seen = HashSet::new();
    values
        .iter()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .filter_map(|value| {
            if seen.insert(value.to_string()) {
                Some(value.to_string())
            } else {
                None
            }
        })
        .collect()
}
