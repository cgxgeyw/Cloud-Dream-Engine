use crate::models::scheduled_notification::{
    ScheduledNotification, ScheduledNotificationCreate,
};
use chrono::Utc;
use rusqlite::{params, Connection};

pub struct ScheduledNotificationRepository<'a> {
    conn: &'a Connection,
}

impl<'a> ScheduledNotificationRepository<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn create(
        &self,
        request: &ScheduledNotificationCreate,
    ) -> Result<ScheduledNotification, String> {
        let source = request.source.trim();
        let notification = ScheduledNotification {
            id: uuid::Uuid::new_v4().to_string(),
            session_id: request.session_id.trim().to_string(),
            world_name: request.world_name.trim().to_string(),
            source: if source.is_empty() {
                uuid::Uuid::new_v4().to_string()
            } else {
                source.to_string()
            },
            title: request.title.trim().to_string(),
            body: request.body.trim().to_string(),
            scheduled_at: request.scheduled_at.trim().to_string(),
            created_at: Utc::now().to_rfc3339(),
            fired_at: None,
            status: "scheduled".to_string(),
            metadata: request.metadata.clone(),
        };
        self.conn
            .execute(
                "INSERT INTO scheduled_notifications (id, session_id, world_name, source, title, body, scheduled_at, created_at, fired_at, status, metadata_json)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
                 ON CONFLICT(session_id, source) DO UPDATE SET
                    world_name = excluded.world_name,
                    title = excluded.title,
                    body = excluded.body,
                    scheduled_at = excluded.scheduled_at,
                    status = 'scheduled',
                    fired_at = NULL,
                    metadata_json = excluded.metadata_json",
                params![
                    notification.id,
                    notification.session_id,
                    notification.world_name,
                    notification.source,
                    notification.title,
                    notification.body,
                    notification.scheduled_at,
                    notification.created_at,
                    notification.fired_at,
                    notification.status,
                    serde_json::to_string(&notification.metadata).unwrap_or_else(|_| "{}".to_string()),
                ],
            )
            .map_err(|error| error.to_string())?;
        self.get_by_session_source(&notification.session_id, &notification.source)?
            .ok_or_else(|| "Scheduled notification was not persisted".to_string())
    }

    pub fn list_pending(&self) -> Result<Vec<ScheduledNotification>, String> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, session_id, world_name, source, title, body, scheduled_at, created_at, fired_at, status, metadata_json
                 FROM scheduled_notifications
                 WHERE status = 'scheduled'
                 ORDER BY scheduled_at ASC, created_at ASC",
            )
            .map_err(|error| error.to_string())?;
        let items = stmt
            .query_map([], row_to_notification)
            .map_err(|error| error.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?;
        Ok(items)
    }

    pub fn list_for_session(
        &self,
        session_id: &str,
        status: Option<&str>,
        limit: usize,
    ) -> Result<Vec<ScheduledNotification>, String> {
        let status = status
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("scheduled");
        let limit = limit.clamp(1, 100) as i64;
        if status.eq_ignore_ascii_case("all") {
            let mut stmt = self
                .conn
                .prepare(
                    "SELECT id, session_id, world_name, source, title, body, scheduled_at, created_at, fired_at, status, metadata_json
                     FROM scheduled_notifications
                     WHERE session_id = ?1
                     ORDER BY scheduled_at ASC, created_at ASC
                     LIMIT ?2",
                )
                .map_err(|error| error.to_string())?;
            let items = stmt
                .query_map(params![session_id, limit], row_to_notification)
                .map_err(|error| error.to_string())?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|error| error.to_string())?;
            return Ok(items);
        }

        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, session_id, world_name, source, title, body, scheduled_at, created_at, fired_at, status, metadata_json
                 FROM scheduled_notifications
                 WHERE session_id = ?1 AND status = ?2
                 ORDER BY scheduled_at ASC, created_at ASC
                 LIMIT ?3",
            )
            .map_err(|error| error.to_string())?;
        let items = stmt
            .query_map(params![session_id, status, limit], row_to_notification)
            .map_err(|error| error.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?;
        Ok(items)
    }

    pub fn get(&self, id: &str) -> Result<Option<ScheduledNotification>, String> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, session_id, world_name, source, title, body, scheduled_at, created_at, fired_at, status, metadata_json
                 FROM scheduled_notifications
                 WHERE id = ?1
                 LIMIT 1",
            )
            .map_err(|error| error.to_string())?;
        let mut rows = stmt
            .query_map(params![id], row_to_notification)
            .map_err(|error| error.to_string())?;
        rows.next().transpose().map_err(|error| error.to_string())
    }

    pub fn get_by_session_source(
        &self,
        session_id: &str,
        source: &str,
    ) -> Result<Option<ScheduledNotification>, String> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, session_id, world_name, source, title, body, scheduled_at, created_at, fired_at, status, metadata_json
                 FROM scheduled_notifications
                 WHERE session_id = ?1 AND source = ?2
                 LIMIT 1",
            )
            .map_err(|error| error.to_string())?;
        let mut rows = stmt
            .query_map(params![session_id, source], row_to_notification)
            .map_err(|error| error.to_string())?;
        rows.next().transpose().map_err(|error| error.to_string())
    }

    pub fn replace_scheduled(
        &self,
        id: &str,
        title: &str,
        body: &str,
        scheduled_at: &str,
        metadata: &serde_json::Value,
    ) -> Result<Option<ScheduledNotification>, String> {
        self.conn
            .execute(
                "UPDATE scheduled_notifications
                 SET title = ?2,
                     body = ?3,
                     scheduled_at = ?4,
                     status = 'scheduled',
                     fired_at = NULL,
                     metadata_json = ?5
                 WHERE id = ?1 AND status = 'scheduled'",
                params![
                    id,
                    title.trim(),
                    body.trim(),
                    scheduled_at.trim(),
                    serde_json::to_string(metadata).unwrap_or_else(|_| "{}".to_string()),
                ],
            )
            .map_err(|error| error.to_string())?;
        self.get(id)
    }

    pub fn cancel(&self, id: &str, reason: &str) -> Result<Option<ScheduledNotification>, String> {
        self.conn
            .execute(
                "UPDATE scheduled_notifications
                 SET status = 'canceled',
                     fired_at = ?2,
                     metadata_json = json_set(
                         COALESCE(NULLIF(metadata_json, ''), '{}'),
                         '$.canceled_reason',
                         ?3
                     )
                 WHERE id = ?1 AND status = 'scheduled'",
                params![id, Utc::now().to_rfc3339(), reason.trim()],
            )
            .map_err(|error| error.to_string())?;
        self.get(id)
    }

    pub fn mark_fired(&self, id: &str) -> Result<(), String> {
        self.conn
            .execute(
                "UPDATE scheduled_notifications SET status = 'fired', fired_at = ?2 WHERE id = ?1 AND status = 'scheduled'",
                params![id, Utc::now().to_rfc3339()],
            )
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    #[cfg(mobile)]
    pub fn mark_native_scheduled(&self, id: &str, native_id: i32) -> Result<(), String> {
        self.conn
            .execute(
                "UPDATE scheduled_notifications
                 SET metadata_json = json_set(
                     COALESCE(NULLIF(metadata_json, ''), '{}'),
                     '$.delivery',
                     'native',
                     '$.native_notification_id',
                     ?2
                 )
                 WHERE id = ?1 AND status = 'scheduled'",
                params![id, native_id],
            )
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    pub fn mark_failed(&self, id: &str, error: &str) -> Result<(), String> {
        self.conn
            .execute(
                "UPDATE scheduled_notifications
                 SET status = 'failed',
                     fired_at = ?2,
                     metadata_json = json_set(COALESCE(NULLIF(metadata_json, ''), '{}'), '$.error', ?3)
                 WHERE id = ?1 AND status = 'scheduled'",
                params![id, Utc::now().to_rfc3339(), error],
            )
            .map_err(|err| err.to_string())?;
        Ok(())
    }
}

fn row_to_notification(row: &rusqlite::Row<'_>) -> rusqlite::Result<ScheduledNotification> {
    let metadata_json: String = row.get(10)?;
    Ok(ScheduledNotification {
        id: row.get(0)?,
        session_id: row.get(1)?,
        world_name: row.get(2)?,
        source: row.get(3)?,
        title: row.get(4)?,
        body: row.get(5)?,
        scheduled_at: row.get(6)?,
        created_at: row.get(7)?,
        fired_at: row.get(8)?,
        status: row.get(9)?,
        metadata: serde_json::from_str(&metadata_json).unwrap_or_else(|_| serde_json::json!({})),
    })
}
