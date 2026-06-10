use std::path::{Path, PathBuf};
use std::time::Duration as StdDuration;

use chrono::{DateTime, Duration, Local, NaiveDateTime, TimeZone, Utc};
use rusqlite::params;
use tauri::AppHandle;
use tauri_plugin_notification::{NotificationExt, PermissionState};
#[cfg(mobile)]
use tauri_plugin_notification::Schedule;

#[cfg(mobile)]
const MIN_NATIVE_SCHEDULE_DELAY: StdDuration = StdDuration::from_secs(5);

use crate::db::repositories::scheduled_notification_repo::ScheduledNotificationRepository;
use crate::db::Database;
use crate::models::scheduled_notification::{
    PendingScheduledNotification, ScheduledNotification, ScheduledNotificationCreate,
};

const SCHEDULE_STATUS_SCHEMA_ID: &str = "attr-schedule-assistant-notifications";
const SCHEDULE_STATUS_SCHEMA_KEY: &str = "scheduled_notifications";
const SCHEDULE_TODO_SCHEMA_ID: &str = "attr-schedule-assistant-todo-items";
const SCHEDULE_TODO_SCHEMA_KEY: &str = "todo_items";

pub struct NotificationScheduler;

pub struct NotificationToolInput<'a> {
    pub session_id: &'a str,
    pub world_name: &'a str,
    pub source: &'a str,
    pub title: Option<&'a str>,
    pub content: &'a str,
    pub requested_time: &'a str,
    pub metadata: serde_json::Value,
}

pub struct NotificationToolContext<'a> {
    pub session_id: &'a str,
    pub world_id: &'a str,
    pub world_name: &'a str,
    pub turn_index: i32,
}

#[derive(Clone, Copy)]
pub struct NotificationToolRuntime<'a> {
    pub app: &'a AppHandle,
    pub data_dir: &'a Path,
}

impl NotificationScheduler {
    pub fn schedule_tool_notification(
        conn: &rusqlite::Connection,
        app: &AppHandle,
        data_dir: &Path,
        input: NotificationToolInput<'_>,
    ) -> Result<ScheduledNotification, String> {
        let body = input.content.trim();
        if body.is_empty() {
            return Err("Notification content is required".to_string());
        }
        ensure_notification_permission(app)?;

        let scheduled_at = parse_notification_time(input.requested_time)?;
        let title = input
            .title
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("Dream Engine");
        let repo = ScheduledNotificationRepository::new(conn);
        let source = input.source.trim();
        if !source.is_empty() {
            if let Some(existing) = repo.get_by_session_source(input.session_id, source)? {
                Self::cancel_delivery(app, &existing)?;
            }
        }
        let metadata = metadata_with_revision(input.metadata);
        let notification = repo.create(
            &ScheduledNotificationCreate {
                session_id: input.session_id.to_string(),
                world_name: input.world_name.to_string(),
                source: input.source.to_string(),
                title: title.to_string(),
                body: body.to_string(),
                scheduled_at: scheduled_at.to_rfc3339(),
                metadata,
            },
        )?;
        Self::schedule_delivery(app.clone(), data_dir.to_path_buf(), notification.clone())?;
        Ok(notification)
    }

    pub fn execute_tool_call(
        conn: &rusqlite::Connection,
        app: &AppHandle,
        data_dir: &Path,
        context: NotificationToolContext<'_>,
        tool_call_id: &str,
        arguments: &serde_json::Map<String, serde_json::Value>,
    ) -> serde_json::Value {
        let action = normalize_notification_action(arguments);
        let result = match action.as_str() {
            "create" => Self::execute_create(conn, app, data_dir, &context, tool_call_id, arguments),
            "update" => Self::execute_update(conn, app, data_dir, &context, tool_call_id, arguments),
            "delete" => Self::execute_delete(conn, app, &context, arguments),
            "list" => Self::execute_list(conn, &context, arguments),
            "get" => Self::execute_get(conn, &context, arguments),
            _ => Err(format!("Unsupported notification action: {action}")),
        };
        match result {
            Ok(value) => serde_json::json!({
                "id": tool_call_id,
                "tool_name": "schedule_notification",
                "tool_call_id": tool_call_id,
                "action": action,
                "ok": true,
                "result": value,
            }),
            Err(error) => serde_json::json!({
                "id": tool_call_id,
                "tool_name": "schedule_notification",
                "tool_call_id": tool_call_id,
                "action": action,
                "ok": false,
                "error": error,
            }),
        }
    }

    fn execute_create(
        conn: &rusqlite::Connection,
        app: &AppHandle,
        data_dir: &Path,
        context: &NotificationToolContext<'_>,
        tool_call_id: &str,
        arguments: &serde_json::Map<String, serde_json::Value>,
    ) -> Result<serde_json::Value, String> {
        let requested_time = arg_text(arguments, "time")
            .or_else(|| arg_text(arguments, "scheduled_at"))
            .ok_or_else(|| "Notification time is required".to_string())?;
        let body = arg_text(arguments, "content")
            .or_else(|| arg_text(arguments, "body"))
            .or_else(|| arg_text(arguments, "message"))
            .ok_or_else(|| "Notification content is required".to_string())?;
        let title = arg_text(arguments, "title").unwrap_or_else(|| "Dream Engine".to_string());
        let source = arg_text(arguments, "source")
            .or_else(|| arg_text(arguments, "key"))
            .or_else(|| arg_text(arguments, "notification_key"))
            .unwrap_or_else(|| {
                format!(
                    "tool:schedule_notification:{}:{}",
                    context.session_id, tool_call_id
                )
            });
        let notification = Self::schedule_tool_notification(
            conn,
            app,
            data_dir,
            NotificationToolInput {
                session_id: context.session_id,
                world_name: context.world_name,
                source: &source,
                title: Some(&title),
                content: &body,
                requested_time: &requested_time,
                metadata: tool_metadata(context, tool_call_id, "create", arguments),
            },
        )?;
        sync_session_schedule_attribute(conn, context.session_id)?;
        Ok(notification_result(&notification))
    }

    fn execute_update(
        conn: &rusqlite::Connection,
        app: &AppHandle,
        data_dir: &Path,
        context: &NotificationToolContext<'_>,
        tool_call_id: &str,
        arguments: &serde_json::Map<String, serde_json::Value>,
    ) -> Result<serde_json::Value, String> {
        let repo = ScheduledNotificationRepository::new(conn);
        let existing = resolve_notification(&repo, context.session_id, arguments)?;
        if existing.status != "scheduled" {
            return Err(format!(
                "Only scheduled notifications can be updated; current status is {}",
                existing.status
            ));
        }
        let scheduled_at = if let Some(requested_time) =
            arg_text(arguments, "time").or_else(|| arg_text(arguments, "scheduled_at"))
        {
            parse_notification_time(&requested_time)?.to_rfc3339()
        } else {
            existing.scheduled_at.clone()
        };
        let body = arg_text(arguments, "content")
            .or_else(|| arg_text(arguments, "body"))
            .or_else(|| arg_text(arguments, "message"))
            .unwrap_or_else(|| existing.body.clone());
        let title = arg_text(arguments, "title").unwrap_or_else(|| existing.title.clone());
        let metadata = merge_tool_metadata(
            existing.metadata.clone(),
            context,
            tool_call_id,
            "update",
            arguments,
        );
        Self::cancel_delivery(app, &existing)?;
        let updated = repo
            .replace_scheduled(&existing.id, &title, &body, &scheduled_at, &metadata)?
            .ok_or_else(|| "Notification was not updated".to_string())?;
        Self::schedule_delivery(app.clone(), data_dir.to_path_buf(), updated.clone())?;
        sync_session_schedule_attribute(conn, context.session_id)?;
        Ok(notification_result(&updated))
    }

    fn execute_delete(
        conn: &rusqlite::Connection,
        app: &AppHandle,
        context: &NotificationToolContext<'_>,
        arguments: &serde_json::Map<String, serde_json::Value>,
    ) -> Result<serde_json::Value, String> {
        let repo = ScheduledNotificationRepository::new(conn);
        let existing = resolve_notification(&repo, context.session_id, arguments)?;
        if existing.status != "scheduled" {
            return Err(format!(
                "Only scheduled notifications can be deleted; current status is {}",
                existing.status
            ));
        }
        Self::cancel_delivery(app, &existing)?;
        let reason = arg_text(arguments, "reason").unwrap_or_else(|| "tool_delete".to_string());
        let canceled = repo
            .cancel(&existing.id, &reason)?
            .ok_or_else(|| "Notification was not canceled".to_string())?;
        sync_session_schedule_attribute(conn, context.session_id)?;
        Ok(notification_result(&canceled))
    }

    fn execute_list(
        conn: &rusqlite::Connection,
        context: &NotificationToolContext<'_>,
        arguments: &serde_json::Map<String, serde_json::Value>,
    ) -> Result<serde_json::Value, String> {
        let repo = ScheduledNotificationRepository::new(conn);
        let status = arg_text(arguments, "status");
        let limit = arg_usize(arguments, "limit").unwrap_or(20);
        let notifications = repo.list_for_session(context.session_id, status.as_deref(), limit)?;
        Ok(serde_json::json!({
            "notifications": notifications,
            "count": notifications.len(),
        }))
    }

    fn execute_get(
        conn: &rusqlite::Connection,
        context: &NotificationToolContext<'_>,
        arguments: &serde_json::Map<String, serde_json::Value>,
    ) -> Result<serde_json::Value, String> {
        let repo = ScheduledNotificationRepository::new(conn);
        let notification = resolve_notification(&repo, context.session_id, arguments)?;
        Ok(notification_result(&notification))
    }

    pub fn restore_pending(app: &AppHandle, data_dir: &Path) -> Result<usize, String> {
        let db = Database::new(&data_dir.to_path_buf())?;
        let repo = ScheduledNotificationRepository::new(db.conn());
        let pending = repo.list_pending()?;
        let count = pending.len();
        for notification in pending {
            if is_native_delivery(&notification) && is_due(&notification.scheduled_at) {
                let _ = repo.mark_fired(&notification.id);
                continue;
            }
            if let Err(error) =
                Self::schedule_delivery(app.clone(), data_dir.to_path_buf(), notification.clone())
            {
                let _ = repo.mark_failed(&notification.id, &error);
            }
        }
        let _ = sync_all_session_schedule_attributes(db.conn());
        Ok(count)
    }

    fn schedule_delivery(
        app: AppHandle,
        data_dir: PathBuf,
        notification: ScheduledNotification,
    ) -> Result<(), String> {
        #[cfg(mobile)]
        {
            if should_use_native_schedule(&notification.scheduled_at) {
                return Self::schedule_native_notification(&app, &data_dir, &notification);
            }
        }

        Self::spawn_notification_task(app, data_dir, notification);
        Ok(())
    }

    #[cfg(mobile)]
    fn schedule_native_notification(
        app: &AppHandle,
        data_dir: &Path,
        notification: &ScheduledNotification,
    ) -> Result<(), String> {
        ensure_notification_permission(app)?;
        let scheduled_at = parse_stored_notification_time(&notification.scheduled_at)?;
        let date = chrono_to_offset_datetime(scheduled_at)?;
        let native_id = native_notification_id(&notification.id);
        let result = app
            .notification()
            .builder()
            .id(native_id)
            .title(notification.title.clone())
            .body(notification.body.clone())
            .schedule(Schedule::At {
                date,
                repeating: false,
                allow_while_idle: true,
            })
            .show();
        match result {
            Ok(_) => {
                let db = Database::new(&data_dir.to_path_buf())?;
                ScheduledNotificationRepository::new(db.conn())
                    .mark_native_scheduled(&notification.id, native_id)?;
                Ok(())
            }
            Err(error) => Err(error.to_string()),
        }
    }

    fn spawn_notification_task(
        app: AppHandle,
        data_dir: PathBuf,
        notification: ScheduledNotification,
    ) {
        tauri::async_runtime::spawn(async move {
            let expected_revision = notification
                .metadata
                .get("revision")
                .and_then(|value| value.as_str())
                .map(str::to_string);
            if let Some(delay) = delay_until(&notification.scheduled_at) {
                tokio::time::sleep(delay).await;
            }

            let db = match Database::new(&data_dir) {
                Ok(db) => db,
                Err(_) => return,
            };
            let repo = ScheduledNotificationRepository::new(db.conn());
            let current = match repo.get(&notification.id) {
                Ok(Some(value))
                    if value.status == "scheduled"
                        && value.scheduled_at == notification.scheduled_at =>
                {
                    value
                }
                _ => return,
            };
            if let Some(expected_revision) = expected_revision.as_deref() {
                let current_revision = current
                    .metadata
                    .get("revision")
                    .and_then(|value| value.as_str());
                if current_revision != Some(expected_revision) {
                    return;
                }
            }
            if let Err(error) = ensure_notification_permission(&app) {
                let _ = repo.mark_failed(&current.id, &error);
                return;
            }
            let result = app
                .notification()
                .builder()
                .title(current.title.clone())
                .body(current.body.clone())
                .show();
            match result {
                Ok(_) => {
                    let _ = repo.mark_fired(&current.id);
                    let _ = sync_session_schedule_attribute(db.conn(), &current.session_id);
                }
                Err(error) => {
                    let _ = repo.mark_failed(&current.id, &error.to_string());
                    let _ = sync_session_schedule_attribute(db.conn(), &current.session_id);
                }
            }
        });
    }

    fn cancel_delivery(
        app: &AppHandle,
        notification: &ScheduledNotification,
    ) -> Result<(), String> {
        #[cfg(mobile)]
        {
            if let Some(native_id) = notification
                .metadata
                .get("native_notification_id")
                .and_then(|value| value.as_i64())
                .and_then(|value| i32::try_from(value).ok())
            {
                app.notification()
                    .cancel(vec![native_id])
                    .map_err(|error| error.to_string())?;
                let _ = app.notification().remove_active(vec![native_id]);
            }
        }
        #[cfg(not(mobile))]
        {
            let _ = app;
            let _ = notification;
        }
        Ok(())
    }
}

pub(crate) fn sync_session_schedule_attribute(
    conn: &rusqlite::Connection,
    session_id: &str,
) -> Result<(), String> {
    ensure_schedule_status_attribute_schema(conn)?;
    ensure_schedule_todo_attribute_schema(conn)?;
    let notifications =
        ScheduledNotificationRepository::new(conn).list_for_session(session_id, Some("scheduled"), 50)?;
    let value = format_schedule_status_items(&notifications);

    write_session_list_attribute(conn, session_id, SCHEDULE_STATUS_SCHEMA_ID, &value)?;
    write_session_list_attribute(conn, session_id, SCHEDULE_TODO_SCHEMA_ID, &value)?;

    Ok(())
}

pub(crate) fn sync_all_session_schedule_attributes(
    conn: &rusqlite::Connection,
) -> Result<(), String> {
    let mut stmt = conn
        .prepare("SELECT DISTINCT session_id FROM scheduled_notifications")
        .map_err(|error| error.to_string())?;
    let session_ids = stmt
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    for session_id in session_ids {
        sync_session_schedule_attribute(conn, &session_id)?;
    }
    Ok(())
}

fn ensure_schedule_status_attribute_schema(conn: &rusqlite::Connection) -> Result<(), String> {
    conn.execute(
        "INSERT INTO attribute_schemas (
            id, scope, key, label, value_type, description, default_value_json, enum_options_json,
            display_policy_json, access_policy_json, mutation_policy_json, influence_policy_json,
            projection_policy_json
         )
         VALUES (?1, 'session', ?2, ?3, 'list', ?4, '[]', '[]', ?5, '{}', '{}', ?6, '{}')
         ON CONFLICT(id) DO UPDATE SET
            scope = 'session',
            key = excluded.key,
            label = excluded.label,
            value_type = 'list',
            description = excluded.description,
            default_value_json = excluded.default_value_json,
            display_policy_json = excluded.display_policy_json,
            influence_policy_json = excluded.influence_policy_json",
        params![
            SCHEDULE_STATUS_SCHEMA_ID,
            SCHEDULE_STATUS_SCHEMA_KEY,
            "\u{5f85}\u{529e}\u{4e8b}\u{9879}",
            "Scheduled reminders for the current session.",
            serde_json::json!({ "game_visible": true }).to_string(),
            serde_json::json!({ "ui.status_panel": { "enabled": true } }).to_string(),
        ],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

fn ensure_schedule_todo_attribute_schema(conn: &rusqlite::Connection) -> Result<(), String> {
    conn.execute(
        "INSERT INTO attribute_schemas (
            id, scope, key, label, value_type, description, default_value_json, enum_options_json,
            display_policy_json, access_policy_json, mutation_policy_json, influence_policy_json,
            projection_policy_json
         )
         VALUES (?1, 'session', ?2, ?3, 'list', ?4, '[]', '[]', ?5, ?6, ?7, ?8, ?9)
         ON CONFLICT(id) DO UPDATE SET
            scope = 'session',
            key = excluded.key,
            label = excluded.label,
            value_type = 'list',
            description = excluded.description,
            default_value_json = excluded.default_value_json,
            display_policy_json = excluded.display_policy_json,
            access_policy_json = excluded.access_policy_json,
            mutation_policy_json = excluded.mutation_policy_json,
            influence_policy_json = excluded.influence_policy_json,
            projection_policy_json = excluded.projection_policy_json",
        params![
            SCHEDULE_TODO_SCHEMA_ID,
            SCHEDULE_TODO_SCHEMA_KEY,
            "\u{5f85}\u{529e}\u{4e8b}\u{9879}",
            "Pending schedule assistant todo items for the current session.",
            serde_json::json!({ "editor_visible": true, "game_visible": true, "debug_visible": true }).to_string(),
            serde_json::json!({
                "creator_read": true,
                "player_read": true,
                "agent_self_read": true,
                "director_read": true,
                "plugin_read": true
            })
            .to_string(),
            serde_json::json!({
                "creator_write": true,
                "rule_write": true,
                "trigger_write": true,
                "player_action_write": true,
                "allowed_ops": ["set"]
            })
            .to_string(),
            serde_json::json!({
                "prompt.director": { "enabled": true, "mode": "raw" },
                "ui.status_panel": { "enabled": true }
            })
            .to_string(),
            serde_json::json!({
                "inherit_to_session": true,
                "session_owner_type": "session",
                "mutable_in_session": true
            })
            .to_string(),
        ],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

fn write_session_list_attribute(
    conn: &rusqlite::Connection,
    session_id: &str,
    schema_id: &str,
    value: &[String],
) -> Result<(), String> {
    conn.execute(
        "DELETE FROM attribute_values WHERE schema_id = ?1 AND owner_type = 'session' AND owner_id = ?2",
        params![schema_id, session_id],
    )
    .map_err(|error| error.to_string())?;
    conn.execute(
        "INSERT INTO attribute_values (id, schema_id, owner_type, owner_id, value_json, source)
         VALUES (?1, ?2, 'session', ?3, ?4, 'schedule_notification')",
        params![
            uuid::Uuid::new_v4().to_string(),
            schema_id,
            session_id,
            serde_json::to_string(&serde_json::Value::Array(
                value.iter().cloned().map(serde_json::Value::String).collect(),
            ))
            .unwrap_or_default(),
        ],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

fn format_schedule_status_items(notifications: &[ScheduledNotification]) -> Vec<String> {
    notifications
        .iter()
        .map(|notification| {
            let title = notification.title.trim();
            let body = notification.body.trim();
            let time_label = format_schedule_time(&notification.scheduled_at);
            if title.is_empty() || title == body {
                format!("[{}] {}", time_label, body)
            } else if body.is_empty() {
                format!("[{}] {}", time_label, title)
            } else {
                format!("[{}] {} - {}", time_label, title, body)
            }
        })
        .collect::<Vec<_>>()
}

fn format_schedule_time(value: &str) -> String {
    DateTime::parse_from_rfc3339(value)
        .map(|time| {
            time.with_timezone(&Local)
                .format("%Y-%m-%d %H:%M")
                .to_string()
        })
        .unwrap_or_else(|_| value.trim().to_string())
}

#[cfg(mobile)]
fn ensure_notification_permission(app: &AppHandle) -> Result<(), String> {
    let notification = app.notification();
    match notification
        .permission_state()
        .map_err(|error| error.to_string())?
    {
        PermissionState::Granted => Ok(()),
        PermissionState::Denied => Err("Notification permission denied".to_string()),
        PermissionState::Prompt | PermissionState::PromptWithRationale => Err(
            "Notification permission is not granted yet. Please allow notifications and try again."
                .to_string(),
        ),
    }
}

#[cfg(not(mobile))]
fn ensure_notification_permission(app: &AppHandle) -> Result<(), String> {
    let notification = app.notification();
    let state = notification
        .permission_state()
        .map_err(|error| error.to_string())?;
    let state = match state {
        PermissionState::Granted => return Ok(()),
        PermissionState::Denied => {
            return Err("Notification permission denied".to_string());
        }
        PermissionState::Prompt | PermissionState::PromptWithRationale => notification
            .request_permission()
            .map_err(|error| error.to_string())?,
    };
    if state == PermissionState::Granted {
        Ok(())
    } else {
        Err(format!("Notification permission not granted: {state}"))
    }
}

pub fn notification_tool_definition() -> serde_json::Value {
    serde_json::json!({
        "tool_name": "schedule_notification",
        "description": "Manage local OS-level notifications for the player. Always call this tool through native tool_calls. Use action=create/update/delete/list/get when the player asks to add, change, cancel, inspect, or review reminders.",
        "arguments_schema": {
            "type": "object",
            "required": ["action"],
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["create", "update", "delete", "list", "get"],
                    "description": "create adds a reminder; update changes an existing scheduled reminder; delete cancels one; list returns reminders for this session; get returns one reminder."
                },
                "notification_id": {
                    "type": "string",
                    "description": "Notification id returned by create/list/get. Required for update/delete/get unless source or key is provided."
                },
                "source": {
                    "type": "string",
                    "description": "Optional stable caller-defined key for a reminder. Can be used instead of notification_id for update/delete/get."
                },
                "key": {
                    "type": "string",
                    "description": "Alias of source for a stable reminder key."
                },
                "time": {
                    "type": "string",
                    "description": "When to notify for create/update. Prefer RFC3339 with timezone, for example 2026-06-07T21:30:00+08:00. Also accepts local YYYY-MM-DD HH:MM, YYYY-MM-DD HH:MM:SS, YYYY-MM-DDTHH:MM, YYYY-MM-DDTHH:MM:SS, 10m, 2h, or 1d."
                },
                "content": {
                    "type": "string",
                    "description": "Notification body for create/update."
                },
                "title": {
                    "type": "string",
                    "description": "Optional notification title."
                },
                "status": {
                    "type": "string",
                    "enum": ["scheduled", "fired", "failed", "canceled", "all"],
                    "description": "Filter for list. Defaults to scheduled."
                },
                "limit": {
                    "type": "integer",
                    "description": "Maximum rows for list. Defaults to 20, maximum 100."
                },
                "reason": {
                    "type": "string",
                    "description": "Optional reason for delete."
                }
            }
        }
    })
}

pub fn pending_notification_from_tool_call(
    session_id: &str,
    tool_call_id: &str,
    arguments: &serde_json::Map<String, serde_json::Value>,
) -> Result<PendingScheduledNotification, String> {
    let action = normalize_notification_action(arguments);
    if action != "create" {
        return Err("Deferred notification calls only support action=create".to_string());
    }
    let requested_time = arg_text(arguments, "time")
        .or_else(|| arg_text(arguments, "scheduled_at"))
        .ok_or_else(|| "Notification time is required".to_string())?;
    let body = arg_text(arguments, "content")
        .or_else(|| arg_text(arguments, "body"))
        .or_else(|| arg_text(arguments, "message"))
        .ok_or_else(|| "Notification content is required".to_string())?;
    let scheduled_at = parse_notification_time(&requested_time)?.to_rfc3339();
    let title = arg_text(arguments, "title").unwrap_or_else(|| "Dream Engine".to_string());
    Ok(PendingScheduledNotification {
        tool_call_id: tool_call_id.to_string(),
        source: format!("tool:schedule_notification:{session_id}:{tool_call_id}"),
        title,
        body,
        requested_time,
        scheduled_at,
        arguments: serde_json::Value::Object(arguments.clone()),
    })
}

fn normalize_notification_action(arguments: &serde_json::Map<String, serde_json::Value>) -> String {
    let action = arg_text(arguments, "action").unwrap_or_else(|| "create".to_string());
    match action.trim().to_ascii_lowercase().as_str() {
        "create" | "add" | "set" | "schedule" => "create".to_string(),
        "update" | "edit" | "reschedule" | "change" => "update".to_string(),
        "delete" | "remove" | "cancel" => "delete".to_string(),
        "list" | "query" | "search" => "list".to_string(),
        "get" | "read" | "show" => "get".to_string(),
        other => other.to_string(),
    }
}

fn resolve_notification(
    repo: &ScheduledNotificationRepository<'_>,
    session_id: &str,
    arguments: &serde_json::Map<String, serde_json::Value>,
) -> Result<ScheduledNotification, String> {
    if let Some(id) = arg_text(arguments, "notification_id").or_else(|| arg_text(arguments, "id")) {
        let notification = repo
            .get(&id)?
            .ok_or_else(|| format!("Notification not found: {id}"))?;
        if notification.session_id != session_id {
            return Err("Notification belongs to another session".to_string());
        }
        return Ok(notification);
    }

    if let Some(source) = arg_text(arguments, "source")
        .or_else(|| arg_text(arguments, "key"))
        .or_else(|| arg_text(arguments, "notification_key"))
    {
        return repo
            .get_by_session_source(session_id, &source)?
            .ok_or_else(|| format!("Notification not found for source: {source}"));
    }

    Err("notification_id or source is required".to_string())
}

fn tool_metadata(
    context: &NotificationToolContext<'_>,
    tool_call_id: &str,
    action: &str,
    arguments: &serde_json::Map<String, serde_json::Value>,
) -> serde_json::Value {
    merge_tool_metadata(
        serde_json::json!({}),
        context,
        tool_call_id,
        action,
        arguments,
    )
}

fn merge_tool_metadata(
    base: serde_json::Value,
    context: &NotificationToolContext<'_>,
    tool_call_id: &str,
    action: &str,
    arguments: &serde_json::Map<String, serde_json::Value>,
) -> serde_json::Value {
    let mut object = base.as_object().cloned().unwrap_or_default();
    object.insert("revision".to_string(), serde_json::Value::String(uuid::Uuid::new_v4().to_string()));
    object.insert("tool_call_id".to_string(), serde_json::Value::String(tool_call_id.to_string()));
    object.insert("tool_name".to_string(), serde_json::Value::String("schedule_notification".to_string()));
    object.insert("action".to_string(), serde_json::Value::String(action.to_string()));
    object.insert("arguments".to_string(), serde_json::Value::Object(arguments.clone()));
    object.insert("turn_index".to_string(), serde_json::json!(context.turn_index));
    object.insert("world_id".to_string(), serde_json::Value::String(context.world_id.to_string()));
    serde_json::Value::Object(object)
}

fn metadata_with_revision(metadata: serde_json::Value) -> serde_json::Value {
    let mut object = metadata.as_object().cloned().unwrap_or_default();
    object.insert(
        "revision".to_string(),
        serde_json::Value::String(uuid::Uuid::new_v4().to_string()),
    );
    serde_json::Value::Object(object)
}

fn notification_result(notification: &ScheduledNotification) -> serde_json::Value {
    serde_json::json!({
        "id": notification.id,
        "session_id": notification.session_id,
        "world_name": notification.world_name,
        "source": notification.source,
        "title": notification.title,
        "content": notification.body,
        "scheduled_at": notification.scheduled_at,
        "created_at": notification.created_at,
        "fired_at": notification.fired_at,
        "status": notification.status,
        "metadata": notification.metadata,
    })
}

pub fn parse_notification_time(input: &str) -> Result<DateTime<Utc>, String> {
    let raw = input.trim();
    if raw.is_empty() {
        return Err("Notification time is required".to_string());
    }
    if let Ok(value) = DateTime::parse_from_rfc3339(raw) {
        return Ok(value.with_timezone(&Utc));
    }

    for format in [
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%d %H:%M",
        "%Y-%m-%dT%H:%M:%S",
        "%Y-%m-%dT%H:%M",
    ] {
        if let Ok(value) = NaiveDateTime::parse_from_str(raw, format) {
            return local_naive_to_utc(value);
        }
    }

    if let Some(value) = parse_relative_time(raw) {
        return Ok(value);
    }
    Err(format!(
        "Unsupported notification time: {raw}. Use RFC3339, YYYY-MM-DD HH:MM, YYYY-MM-DDTHH:MM:SS, 10m, 2h, or 1d."
    ))
}

fn parse_relative_time(input: &str) -> Option<DateTime<Utc>> {
    let lower = input.trim().to_ascii_lowercase();
    let number = lower
        .chars()
        .take_while(|ch| ch.is_ascii_digit())
        .collect::<String>();
    if number.is_empty() {
        return None;
    }
    let amount = number.parse::<i64>().ok()?;
    if amount <= 0 {
        return None;
    }
    let unit = lower[number.len()..].trim();
    let duration = match unit {
        "s" | "sec" | "second" | "seconds" => Duration::seconds(amount),
        "m" | "min" | "minute" | "minutes" => Duration::minutes(amount),
        "h" | "hr" | "hour" | "hours" => Duration::hours(amount),
        "d" | "day" | "days" => Duration::days(amount),
        _ => return None,
    };
    Some(Utc::now() + duration)
}

fn local_naive_to_utc(value: NaiveDateTime) -> Result<DateTime<Utc>, String> {
    Local
        .from_local_datetime(&value)
        .single()
        .or_else(|| Local.from_local_datetime(&value).earliest())
        .map(|value| value.with_timezone(&Utc))
        .ok_or_else(|| "Invalid local notification time".to_string())
}

fn parse_stored_notification_time(scheduled_at: &str) -> Result<DateTime<Utc>, String> {
    DateTime::parse_from_rfc3339(scheduled_at)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|error| format!("Invalid stored notification time: {error}"))
}

fn is_due(scheduled_at: &str) -> bool {
    parse_stored_notification_time(scheduled_at)
        .map(|scheduled| scheduled <= Utc::now())
        .unwrap_or(false)
}

fn is_native_delivery(notification: &ScheduledNotification) -> bool {
    notification
        .metadata
        .get("delivery")
        .and_then(|value| value.as_str())
        == Some("native")
}

#[cfg(mobile)]
fn should_use_native_schedule(scheduled_at: &str) -> bool {
    delay_until(scheduled_at)
        .map(|delay| delay >= MIN_NATIVE_SCHEDULE_DELAY)
        .unwrap_or(false)
}

#[cfg(mobile)]
fn chrono_to_offset_datetime(value: DateTime<Utc>) -> Result<time::OffsetDateTime, String> {
    time::OffsetDateTime::from_unix_timestamp(value.timestamp())
        .map_err(|error| format!("Invalid native notification time: {error}"))
}

#[cfg(mobile)]
fn native_notification_id(id: &str) -> i32 {
    let mut hash: u32 = 0x811c9dc5;
    for byte in id.as_bytes() {
        hash ^= u32::from(*byte);
        hash = hash.wrapping_mul(0x01000193);
    }
    (hash & 0x7fff_ffff) as i32
}

fn delay_until(scheduled_at: &str) -> Option<StdDuration> {
    let scheduled = DateTime::parse_from_rfc3339(scheduled_at)
        .ok()?
        .with_timezone(&Utc);
    let now = Utc::now();
    if scheduled <= now {
        return None;
    }
    (scheduled - now).to_std().ok()
}

fn arg_text(arguments: &serde_json::Map<String, serde_json::Value>, key: &str) -> Option<String> {
    arguments
        .get(key)
        .and_then(|value| match value {
            serde_json::Value::String(text) => Some(text.trim().to_string()),
            serde_json::Value::Number(number) => Some(number.to_string()),
            _ => None,
        })
        .filter(|value| !value.is_empty())
}

fn arg_usize(arguments: &serde_json::Map<String, serde_json::Value>, key: &str) -> Option<usize> {
    arguments
        .get(key)
        .and_then(|value| {
            value
                .as_u64()
                .or_else(|| value.as_str()?.trim().parse::<u64>().ok())
        })
        .and_then(|value| usize::try_from(value).ok())
}

#[cfg(test)]
mod tests {
    use super::parse_notification_time;
    use chrono::{Duration, Utc};

    #[test]
    fn parses_relative_notification_time() {
        let parsed = parse_notification_time("10m").expect("relative time");
        assert!(parsed > Utc::now() + Duration::minutes(9));
        assert!(parsed < Utc::now() + Duration::minutes(11));
    }

    #[test]
    fn parses_rfc3339_notification_time() {
        let parsed = parse_notification_time("2026-06-07T21:30:00+08:00").expect("rfc3339");
        assert_eq!(parsed.to_rfc3339(), "2026-06-07T13:30:00+00:00");
    }

    #[test]
    fn parses_local_iso_notification_time_without_timezone() {
        parse_notification_time("2026-06-10T16:55:00").expect("local iso seconds");
        parse_notification_time("2026-06-10T16:55").expect("local iso minutes");
    }
}
