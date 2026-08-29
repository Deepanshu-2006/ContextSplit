use crate::db::models::ContextSignal;
use chrono::Utc;
use rusqlite::{params, Connection, Result};
use uuid::Uuid;

pub fn create_context_signal(
    conn: &Connection,
    workspace_id: Option<&str>,
    item_id: Option<&str>,
    signal_type: &str,
    session_id: Option<&str>,
    payload: &str,
) -> Result<ContextSignal> {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();

    conn.execute(
        "INSERT INTO context_signals (id, workspace_id, item_id, signal_type, session_id, payload, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![id, workspace_id, item_id, signal_type, session_id, payload, now, now],
    )?;

    Ok(ContextSignal {
        id,
        workspace_id: workspace_id.map(|s| s.to_string()),
        item_id: item_id.map(|s| s.to_string()),
        signal_type: signal_type.to_string(),
        session_id: session_id.map(|s| s.to_string()),
        payload: payload.to_string(),
        created_at: now.clone(),
        updated_at: now,
    })
}

pub fn get_context_signal(conn: &Connection, id: &str) -> Result<ContextSignal> {
    let mut stmt = conn.prepare("SELECT id, workspace_id, item_id, signal_type, session_id, payload, created_at, updated_at FROM context_signals WHERE id = ?1")?;
    let signal = stmt.query_row(params![id], |row| {
        Ok(ContextSignal {
            id: row.get(0)?,
            workspace_id: row.get(1)?,
            item_id: row.get(2)?,
            signal_type: row.get(3)?,
            session_id: row.get(4)?,
            payload: row.get(5)?,
            created_at: row.get(6)?,
            updated_at: row.get(7)?,
        })
    })?;
    
    Ok(signal)
}

pub fn list_context_signals(conn: &Connection, workspace_id: Option<&str>, item_id: Option<&str>) -> Result<Vec<ContextSignal>> {
    let mut signals = Vec::new();

    if let Some(w_id) = workspace_id {
        let mut stmt = conn.prepare("SELECT id, workspace_id, item_id, signal_type, session_id, payload, created_at, updated_at FROM context_signals WHERE workspace_id = ?1 ORDER BY created_at DESC")?;
        let iter = stmt.query_map(params![w_id], |row| {
            Ok(ContextSignal {
                id: row.get(0)?,
                workspace_id: row.get(1)?,
                item_id: row.get(2)?,
                signal_type: row.get(3)?,
                session_id: row.get(4)?,
                payload: row.get(5)?,
                created_at: row.get(6)?,
                updated_at: row.get(7)?,
            })
        })?;
        for signal in iter {
            signals.push(signal?);
        }
    } else if let Some(i_id) = item_id {
        let mut stmt = conn.prepare("SELECT id, workspace_id, item_id, signal_type, session_id, payload, created_at, updated_at FROM context_signals WHERE item_id = ?1 ORDER BY created_at DESC")?;
        let iter = stmt.query_map(params![i_id], |row| {
            Ok(ContextSignal {
                id: row.get(0)?,
                workspace_id: row.get(1)?,
                item_id: row.get(2)?,
                signal_type: row.get(3)?,
                session_id: row.get(4)?,
                payload: row.get(5)?,
                created_at: row.get(6)?,
                updated_at: row.get(7)?,
            })
        })?;
        for signal in iter {
            signals.push(signal?);
        }
    } else {
        let mut stmt = conn.prepare("SELECT id, workspace_id, item_id, signal_type, session_id, payload, created_at, updated_at FROM context_signals ORDER BY created_at DESC")?;
        let iter = stmt.query_map([], |row| {
            Ok(ContextSignal {
                id: row.get(0)?,
                workspace_id: row.get(1)?,
                item_id: row.get(2)?,
                signal_type: row.get(3)?,
                session_id: row.get(4)?,
                payload: row.get(5)?,
                created_at: row.get(6)?,
                updated_at: row.get(7)?,
            })
        })?;
        for signal in iter {
            signals.push(signal?);
        }
    }

    Ok(signals)
}
