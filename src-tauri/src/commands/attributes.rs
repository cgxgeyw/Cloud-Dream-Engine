use crate::models::attribute::*;
use crate::state::AppState;
use tauri::State;

#[tauri::command]
pub async fn list_attribute_schemas(
    state: State<'_, AppState>,
    scope: Option<String>,
) -> Result<Vec<AttributeSchema>, String> {
    let db = state.db.lock().await;
    let repo = crate::db::repositories::attribute_repo::AttributeRepository::new(db.conn());
    repo.list_schemas(scope.as_deref())
}

#[tauri::command]
pub async fn create_attribute_schema(
    state: State<'_, AppState>,
    request: AttributeSchemaCreateRequest,
) -> Result<AttributeSchema, String> {
    let db = state.db.lock().await;
    let repo = crate::db::repositories::attribute_repo::AttributeRepository::new(db.conn());
    repo.create_schema(&request)
}

#[tauri::command]
pub async fn update_attribute_schema(
    state: State<'_, AppState>,
    id: String,
    request: AttributeSchemaCreateRequest,
) -> Result<AttributeSchema, String> {
    let db = state.db.lock().await;
    let repo = crate::db::repositories::attribute_repo::AttributeRepository::new(db.conn());
    repo.update_schema(&id, &request)
}

#[tauri::command]
pub async fn list_attribute_values(
    state: State<'_, AppState>,
    owner_type: Option<String>,
    owner_id: Option<String>,
    schema_id: Option<String>,
) -> Result<Vec<AttributeValue>, String> {
    let db = state.db.lock().await;
    let repo = crate::db::repositories::attribute_repo::AttributeRepository::new(db.conn());
    repo.list_values(
        owner_type.as_deref(),
        owner_id.as_deref(),
        schema_id.as_deref(),
    )
}

#[tauri::command]
pub async fn upsert_attribute_value(
    state: State<'_, AppState>,
    request: AttributeValueUpsertRequest,
) -> Result<AttributeValue, String> {
    let db = state.db.lock().await;
    let repo = crate::db::repositories::attribute_repo::AttributeRepository::new(db.conn());
    repo.upsert_value(&request)
}
