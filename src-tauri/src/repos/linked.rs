use rusqlite::{params, Connection, OptionalExtension};

use crate::error::Result;
use crate::linking::{Snapshot, Status};

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotView {
    pub source: String,
    pub kind: String,
    pub fetched_at: i64,
    pub sig: String,
    pub status: String,
    pub detail: String,
    pub payload_json: String,
}

fn to_view(row: Snapshot) -> SnapshotView {
    SnapshotView {
        source: row.source,
        kind: row.kind,
        fetched_at: row.fetched_at,
        sig: row.sig,
        status: row.status.as_str().to_owned(),
        detail: row.detail,
        payload_json: row.payload_json,
    }
}

fn from_row(r: &rusqlite::Row) -> rusqlite::Result<Snapshot> {
    Ok(Snapshot {
        source: r.get("source")?,
        kind: r.get("kind")?,
        fetched_at: r.get("fetched_at")?,
        sig: r.get("sig")?,
        status: Status::parse(&r.get::<_, String>("status")?),
        detail: r.get("detail")?,
        payload_json: r.get("payload_json")?,
    })
}

const COLS: &str = "source, kind, fetched_at, sig, status, detail, payload_json";

/// 写入缓存。签名没变就只刷新 `fetched_at`（让「最后成功于 X 分钟前」是真的），
/// 返回是否真的换了内容 —— 前端拿这个决定要不要重绘。
pub fn upsert(conn: &Connection, snap: &Snapshot) -> Result<bool> {
    let previous: Option<String> = conn
        .query_row(
            "SELECT sig FROM linked_snapshots WHERE source = ?1 AND kind = ?2",
            params![snap.source, snap.kind],
            |r| r.get(0),
        )
        .optional()?;

    conn.execute(
        "INSERT INTO linked_snapshots (source, kind, fetched_at, sig, status, detail, payload_json)
         VALUES (?1,?2,?3,?4,?5,?6,?7)
         ON CONFLICT(source, kind) DO UPDATE SET
           fetched_at = excluded.fetched_at,
           sig = excluded.sig,
           status = excluded.status,
           detail = excluded.detail,
           payload_json = excluded.payload_json",
        params![
            snap.source,
            snap.kind,
            snap.fetched_at,
            snap.sig,
            snap.status.as_str(),
            snap.detail,
            snap.payload_json
        ],
    )?;

    Ok(previous.as_deref() != Some(snap.sig.as_str()))
}

pub fn get(conn: &Connection, source: &str, kind: &str) -> Result<Option<Snapshot>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLS} FROM linked_snapshots WHERE source = ?1 AND kind = ?2"
    ))?;
    let found = stmt
        .query_map(params![source, kind], from_row)?
        .next()
        .and_then(|r| r.ok());
    Ok(found)
}

pub fn get_view(conn: &Connection, source: &str, kind: &str) -> Result<Option<SnapshotView>> {
    Ok(get(conn, source, kind)?.map(to_view))
}

pub fn list_for(conn: &Connection, source: &str) -> Result<Vec<SnapshotView>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLS} FROM linked_snapshots WHERE source = ?1 ORDER BY kind"
    ))?;
    let rows = stmt.query_map(params![source], from_row)?;
    let mut out = Vec::new();
    for row in rows {
        out.push(to_view(row?));
    }
    Ok(out)
}

/// 只更新状态与说明（线上比对用），不动 payload。
pub fn set_status(conn: &Connection, source: &str, kind: &str, status: Status, detail: &str, fetched_at: i64) -> Result<()> {
    conn.execute(
        "UPDATE linked_snapshots SET status = ?3, detail = ?4, fetched_at = ?5 WHERE source = ?1 AND kind = ?2",
        params![source, kind, status.as_str(), detail, fetched_at],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::linking::empty_payload;

    fn snap(sig: &str, payload: &str) -> Snapshot {
        Snapshot {
            source: "blog".into(),
            kind: "projects".into(),
            fetched_at: 100,
            sig: sig.into(),
            status: Status::Ok,
            detail: String::new(),
            payload_json: payload.into(),
        }
    }

    fn conn() -> Connection {
        let mut c = crate::db::open_in_memory().unwrap();
        crate::db::migrations::migrate(&mut c).unwrap();
        c
    }

    #[test]
    fn upsert_reports_only_real_changes() {
        let c = conn();
        let body = empty_payload();
        assert!(upsert(&c, &snap("aaa", &body)).unwrap(), "第一次算变化");
        assert!(!upsert(&c, &snap("aaa", &body)).unwrap(), "同签名不该报变化");
        assert!(upsert(&c, &snap("bbb", &body)).unwrap());

        let listed = list_for(&c, "blog").unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].sig, "bbb");
    }

    #[test]
    fn status_can_move_to_stale_without_losing_the_payload() {
        let c = conn();
        upsert(&c, &snap("aaa", r#"{"count":1}"#)).unwrap();
        set_status(&c, "blog", "projects", Status::Stale, "线上 2 条 / 本地缓存 1 条", 200).unwrap();

        let row = get(&c, "blog", "projects").unwrap().unwrap();
        assert_eq!(row.status, Status::Stale);
        assert_eq!(row.payload_json, r#"{"count":1}"#, "比对结果不该覆盖缓存");
        assert_eq!(row.fetched_at, 200);
    }
}
