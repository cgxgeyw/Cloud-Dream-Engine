use rusqlite::params;
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::state::AppState;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeBase { pub id: String, pub world_id: Option<String>, pub name: String, pub description: String, pub enabled: bool }
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeDocument { pub id: String, pub knowledge_base_id: String, pub title: String, pub content: String, pub source_filename: String, pub updated_at: String }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundledKnowledgeBase { pub name: String, #[serde(default)] pub description: String, #[serde(default)] pub documents: Vec<BundledDocument> }
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundledDocument { pub title: String, pub content: String, #[serde(default)] pub source_filename: String }

pub fn tool_capability(world: &crate::models::world::WorldDefinition) -> Option<serde_json::Value> {
    (world.director_config["knowledge_access"] == true).then(|| serde_json::json!({
        "tool_name": "search_knowledge", "description": "Search this world's authorized knowledge documents. Results include source titles and text excerpts.",
        "arguments_schema": {"type":"object", "required":["query"], "properties":{"query":{"type":"string"}}}
    }))
}

pub fn search(conn: &rusqlite::Connection, world_id: &str, arguments: &serde_json::Value) -> Result<serde_json::Value, String> {
    let query = arguments["query"].as_str().filter(|s| !s.trim().is_empty() && s.len() <= 2048).ok_or("查询不能为空且最多 2048 字节")?;
    let terms = crate::services::game_engine::memory::build_memory_search_terms(query);
    let mut stmt = conn.prepare("SELECT d.id, d.title, d.content, b.name FROM knowledge_documents d JOIN knowledge_bases b ON b.id = d.knowledge_base_id WHERE b.enabled = 1 AND (b.world_id = ?1 OR (b.world_id IS NULL AND EXISTS (SELECT 1 FROM world_knowledge_bindings x WHERE x.world_id = ?1 AND x.knowledge_base_id = b.id))) ORDER BY d.id").map_err(|e| e.to_string())?;
    let rows = stmt.query_map(params![world_id], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, String>(3)?))).map_err(|e| e.to_string())?;
    let mut hits = Vec::new();
    for row in rows { let (id, title, content, base) = row.map_err(|e| e.to_string())?;
        for (index, chunk) in content.chars().collect::<Vec<_>>().chunks(1400).enumerate() {
            let excerpt: String = chunk.iter().collect();
            let text = crate::services::game_engine::memory::normalize_memory_text(&format!("{title} {excerpt}"));
            let score = terms.iter().filter(|t| text.contains(t.as_str())).count();
            if score > 0 { hits.push((score, serde_json::json!({"document_id": id, "title": title, "knowledge_base": base, "chunk": index, "content": excerpt, "matched_terms": terms.iter().filter(|t| text.contains(t.as_str())).collect::<Vec<_>>() }))); hits.sort_by(|a, b| b.0.cmp(&a.0)); hits.truncate(6); }
        }
    }
    hits.sort_by(|a, b| b.0.cmp(&a.0)); hits.truncate(6);
    Ok(serde_json::json!({"query":query,"results":hits.into_iter().map(|(_, v)| v).collect::<Vec<_>>()}))
}

fn enable_world_knowledge(conn: &rusqlite::Connection, world_id: &str) -> Result<(), String> {
    conn.execute("UPDATE worlds SET director_config_json = json_set(director_config_json, '$.knowledge_access', json('true')) WHERE id = ?1", params![world_id]).map_err(|e| e.to_string()).map(|_| ())
}

#[tauri::command]
pub async fn get_knowledge_bindings(state: State<'_, AppState>, knowledge_base_id: String) -> Result<Vec<String>, String> {
    let db = state.db.lock().await;
    let mut stmt = db.conn().prepare("SELECT world_id FROM world_knowledge_bindings WHERE knowledge_base_id = ?1").map_err(|e| e.to_string())?;
    let rows = stmt.query_map(params![knowledge_base_id], |r| r.get(0)).map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn set_knowledge_binding(state: State<'_, AppState>, knowledge_base_id: String, world_id: String, enabled: bool) -> Result<(), String> {
    let db = state.db.lock().await;
    let owner: Option<String> = db.conn().query_row("SELECT world_id FROM knowledge_bases WHERE id = ?1", params![knowledge_base_id], |r| r.get(0)).map_err(|e| e.to_string())?;
    if owner.is_some() { return Err("世界专属知识库不能关联到其他世界".into()); }
    let sql = if enabled { "INSERT OR IGNORE INTO world_knowledge_bindings (knowledge_base_id, world_id) VALUES (?1, ?2)" } else { "DELETE FROM world_knowledge_bindings WHERE knowledge_base_id = ?1 AND world_id = ?2" };
    db.conn().execute(sql, params![knowledge_base_id, world_id]).map_err(|e| e.to_string())?;
    if enabled { enable_world_knowledge(db.conn(), &world_id)?; }
    Ok(())
}

pub fn export_bundled(conn: &rusqlite::Connection, world_id: &str) -> Result<Vec<BundledKnowledgeBase>, String> {
    let mut stmt = conn.prepare("SELECT id, name, description FROM knowledge_bases WHERE world_id = ?1 ORDER BY id").map_err(|e| e.to_string())?;
    let rows = stmt.query_map(params![world_id], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?))).map_err(|e| e.to_string())?;
    let mut result = Vec::new();
    for row in rows { let (id, name, description) = row.map_err(|e| e.to_string())?;
        let mut docs = conn.prepare("SELECT title, content, source_filename FROM knowledge_documents WHERE knowledge_base_id = ?1 ORDER BY id").map_err(|e| e.to_string())?;
        let documents = docs.query_map(params![id], |r| Ok(BundledDocument { title: r.get(0)?, content: r.get(1)?, source_filename: r.get(2)? })).map_err(|e| e.to_string())?.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())?;
        result.push(BundledKnowledgeBase { name, description, documents });
    } Ok(result)
}

pub fn import_bundled(conn: &rusqlite::Connection, world_id: &str, bases: &[BundledKnowledgeBase]) -> Result<(), String> {
    // World-package upgrades replace only that world's bundled resources.
    conn.execute("DELETE FROM knowledge_bases WHERE world_id = ?1", params![world_id]).map_err(|e| e.to_string())?;
    for base in bases {
        if base.name.trim().is_empty() { return Err("知识库名称不能为空".into()); }
        let id = uuid::Uuid::new_v4().to_string();
        conn.execute("INSERT INTO knowledge_bases (id, world_id, name, description) VALUES (?1, ?2, ?3, ?4)", params![id, world_id, base.name, base.description]).map_err(|e| e.to_string())?;
        for doc in &base.documents {
            if doc.content.len() > 20 * 1024 * 1024 { return Err("知识库正文最大 20MB".into()); }
            conn.execute("INSERT INTO knowledge_documents (id, knowledge_base_id, title, content, source_filename, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)", params![uuid::Uuid::new_v4().to_string(), id, doc.title, doc.content, doc.source_filename, chrono::Utc::now().to_rfc3339()]).map_err(|e| e.to_string())?;
        }
    } Ok(())
}

fn row_base(row: &rusqlite::Row<'_>) -> rusqlite::Result<KnowledgeBase> {
    Ok(KnowledgeBase { id: row.get(0)?, world_id: row.get(1)?, name: row.get(2)?, description: row.get(3)?, enabled: row.get::<_, i64>(4)? != 0 })
}

#[tauri::command]
pub async fn list_knowledge_bases(state: State<'_, AppState>, world_id: Option<String>) -> Result<Vec<KnowledgeBase>, String> {
    let db = state.db.lock().await;
    let mut stmt = db.conn().prepare("SELECT id, world_id, name, description, enabled FROM knowledge_bases WHERE (?1 IS NULL OR world_id = ?1 OR world_id IS NULL) ORDER BY world_id IS NOT NULL, name").map_err(|e| e.to_string())?;
    let rows = stmt.query_map(params![world_id], row_base).map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn create_knowledge_base(state: State<'_, AppState>, name: String, description: String, world_id: Option<String>) -> Result<KnowledgeBase, String> {
    let id = uuid::Uuid::new_v4().to_string();
    if name.trim().is_empty() { return Err("知识库名称不能为空".into()); }
    let db = state.db.lock().await;
    db.conn().execute("INSERT INTO knowledge_bases (id, world_id, name, description) VALUES (?1, ?2, ?3, ?4)", params![id, world_id, name.trim(), description]).map_err(|e| e.to_string())?;
    if let Some(owner) = &world_id { enable_world_knowledge(db.conn(), owner)?; }
    db.conn().query_row("SELECT id, world_id, name, description, enabled FROM knowledge_bases WHERE id = ?1", params![id], row_base).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn delete_knowledge_base(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let db = state.db.lock().await;
    db.conn().execute("DELETE FROM knowledge_bases WHERE id = ?1", params![id]).map_err(|e| e.to_string()).map(|_| ())
}

#[tauri::command]
pub async fn list_knowledge_documents(state: State<'_, AppState>, knowledge_base_id: String) -> Result<Vec<KnowledgeDocument>, String> {
    let db = state.db.lock().await;
    let mut stmt = db.conn().prepare("SELECT id, knowledge_base_id, title, content, source_filename, updated_at FROM knowledge_documents WHERE knowledge_base_id = ?1 ORDER BY title").map_err(|e| e.to_string())?;
    let rows = stmt.query_map(params![knowledge_base_id], |row| Ok(KnowledgeDocument { id: row.get(0)?, knowledge_base_id: row.get(1)?, title: row.get(2)?, content: row.get(3)?, source_filename: row.get(4)?, updated_at: row.get(5)? })).map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
}

fn extract_text(filename: &str, data: &[u8]) -> Result<String, String> {
    let ext = filename.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        "txt" | "md" | "markdown" | "csv" | "json" | "jsonl" | "yaml" | "yml" | "xml" => String::from_utf8(data.to_vec()).map_err(|_| "文件不是有效的 UTF-8 文本".to_string()),
        "docx" => {
            let cursor = std::io::Cursor::new(data);
            let mut archive = zip::ZipArchive::new(cursor).map_err(|e| format!("无法读取 DOCX：{e}"))?;
            let file = archive.by_name("word/document.xml").map_err(|_| "DOCX 中没有正文".to_string())?;
            let mut xml = String::new(); use std::io::Read;
            file.take(20 * 1024 * 1024 + 1).read_to_string(&mut xml).map_err(|e| e.to_string())?;
            if xml.len() > 20 * 1024 * 1024 { return Err("DOCX 解压正文过大".into()); }
            let mut reader = quick_xml::Reader::from_str(&xml);
            let mut text = String::new(); let mut in_text = false;
            loop { match reader.read_event().map_err(|e| e.to_string())? {
                quick_xml::events::Event::Start(e) if e.local_name().as_ref() == b"t" => in_text = true,
                quick_xml::events::Event::Text(e) if in_text => text.push_str(&e.unescape().map_err(|e| e.to_string())?),
                quick_xml::events::Event::End(e) => { if e.local_name().as_ref() == b"t" { in_text = false; } if e.local_name().as_ref() == b"p" { text.push('\n'); } },
                quick_xml::events::Event::Eof => break,
                _ => ()
            } }
            Ok(text)
        }
        "pdf" => pdf_extract::extract_text_from_mem(data).map_err(|e| format!("PDF 文本提取失败：{e}")),
        _ => Err("支持 TXT、Markdown、CSV、JSON、YAML、XML、DOCX 和 PDF 文件".to_string()),
    }
}

#[tauri::command]
pub async fn update_knowledge_document(state: State<'_, AppState>, id: String, title: String, content: String) -> Result<(), String> {
    if title.trim().is_empty() || content.trim().is_empty() || content.len() > 20 * 1024 * 1024 { return Err("标题和正文不能为空，正文最大 20MB".into()); }
    let db = state.db.lock().await;
    db.conn().execute("UPDATE knowledge_documents SET title = ?2, content = ?3, updated_at = ?4 WHERE id = ?1", params![id, title, content, chrono::Utc::now().to_rfc3339()]).map_err(|e| e.to_string()).map(|_| ())
}

#[tauri::command]
pub async fn add_knowledge_document(state: State<'_, AppState>, knowledge_base_id: String, filename: String, data: Vec<u8>) -> Result<KnowledgeDocument, String> {
    if data.len() > 20 * 1024 * 1024 { return Err("知识库文件最大 20MB".to_string()); }
    let extract_filename = filename.clone();
    let content = tauri::async_runtime::spawn_blocking(move || extract_text(&extract_filename, &data)).await.map_err(|e| e.to_string())??;
    if content.len() > 20 * 1024 * 1024 { return Err("提取正文最大 20MB".into()); }
    if content.trim().is_empty() { return Err("文件没有可用文字".to_string()); }
    let id = uuid::Uuid::new_v4().to_string(); let now = chrono::Utc::now().to_rfc3339();
    let title = filename.rsplit(['/', '\\']).next().unwrap_or(&filename).to_string();
    let db = state.db.lock().await;
    db.conn().execute("INSERT INTO knowledge_documents (id, knowledge_base_id, title, content, source_filename, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)", params![id, knowledge_base_id, title, content, filename, now]).map_err(|e| e.to_string())?;
    Ok(KnowledgeDocument { id, knowledge_base_id, title, content, source_filename: filename, updated_at: now })
}

#[tauri::command]
pub async fn delete_knowledge_document(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let db = state.db.lock().await; db.conn().execute("DELETE FROM knowledge_documents WHERE id = ?1", params![id]).map_err(|e| e.to_string()).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn database() -> rusqlite::Connection {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON; CREATE TABLE worlds(id TEXT PRIMARY KEY, director_config_json TEXT DEFAULT '{}'); CREATE TABLE knowledge_bases(id TEXT PRIMARY KEY, world_id TEXT REFERENCES worlds(id) ON DELETE CASCADE, name TEXT, description TEXT, enabled INTEGER DEFAULT 1); CREATE TABLE knowledge_documents(id TEXT PRIMARY KEY, knowledge_base_id TEXT REFERENCES knowledge_bases(id) ON DELETE CASCADE, title TEXT, content TEXT, source_filename TEXT, updated_at TEXT); CREATE TABLE world_knowledge_bindings(world_id TEXT REFERENCES worlds(id) ON DELETE CASCADE, knowledge_base_id TEXT REFERENCES knowledge_bases(id) ON DELETE CASCADE, PRIMARY KEY(world_id,knowledge_base_id)); INSERT INTO worlds(id) VALUES('one'),('two');").unwrap();
        conn
    }
    #[test]
    fn bundled_roundtrip_upgrade_and_cascade() {
        let conn = database();
        let payload = vec![BundledKnowledgeBase { name: "规则".into(), description: "规则说明".into(), documents: vec![BundledDocument { title: "毒酒".into(), content: "毒酒扣血".into(), source_filename: "rules.md".into() }] }];
        import_bundled(&conn, "one", &payload).unwrap();
        assert_eq!(export_bundled(&conn, "one").unwrap()[0].documents[0].content, "毒酒扣血");
        import_bundled(&conn, "one", &payload).unwrap();
        assert_eq!(export_bundled(&conn, "one").unwrap().len(), 1);
        assert!(search(&conn, "two", &serde_json::json!({"query":"毒酒"})).unwrap()["results"].as_array().unwrap().is_empty());
        assert_eq!(search(&conn, "one", &serde_json::json!({"query":"毒酒"})).unwrap()["results"].as_array().unwrap().len(), 1);
        conn.execute("DELETE FROM worlds WHERE id='one'", []).unwrap();
        assert_eq!(conn.query_row("SELECT count(*) FROM knowledge_documents", [], |r| r.get::<_, i64>(0)).unwrap(), 0);
    }
    #[test]
    fn public_access_requires_binding() {
        let conn = database();
        conn.execute_batch("INSERT INTO knowledge_bases VALUES('public',NULL,'公共','',1); INSERT INTO knowledge_documents VALUES('doc','public','毒酒','毒酒扣血','rules.md','now');").unwrap();
        let query = serde_json::json!({"query":"毒酒"});
        assert!(search(&conn, "one", &query).unwrap()["results"].as_array().unwrap().is_empty());
        conn.execute_batch("INSERT INTO world_knowledge_bindings VALUES('one','public');").unwrap();
        assert_eq!(search(&conn, "one", &query).unwrap()["results"].as_array().unwrap().len(), 1);
        assert!(search(&conn, "two", &query).unwrap()["results"].as_array().unwrap().is_empty());
    }
    #[test]
    fn docx_extracts_only_text_and_unescapes_xml() {
        use std::io::Write;
        let mut archive = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        archive.start_file("word/document.xml", zip::write::SimpleFileOptions::default()).unwrap();
        archive.write_all("<w:document xmlns:w='test'><w:p><w:r><w:rPr/><w:t>规则 &amp; 毒酒</w:t></w:r></w:p></w:document>".as_bytes()).unwrap();
        let bytes = archive.finish().unwrap().into_inner();
        assert_eq!(extract_text("test.docx", &bytes).unwrap(), "规则 & 毒酒\n");
        assert!(extract_text("bad.txt", &[0xff]).is_err());
        assert!(extract_text("bad.exe", b"text").is_err());
    }
}
