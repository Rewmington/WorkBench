//! 药箱库的只读访问。
//!
//! 照抄它自己 `tools/export_backup.dart` 的做法：**复制 → 只读打开 → 用完删掉**。
//! 不直接开原文件，因为对方哪天换成 WAL 模式时，只读打开主文件而看不到 -wal
//! 会读到过期数据；连 sidecar 一起复制就没这个问题（今天它还是 journal_mode=delete，
//! 但依赖「对方不变」的假设迟早会咬人）。
//!
//! 绝不写它的库，绝不写它的 `medicine-sync/` 目录（PLAN §6.2）。

use std::path::Path;

use rusqlite::{Connection, OpenFlags};

use super::medicine_math::{Dose, Medicine};
use super::{sig, Snapshot, Status};
use crate::error::{Error, Result};
use crate::time::{days, Now};

/// 复制出来的库放在哪。
pub struct ReadGuard {
    dir: std::path::PathBuf,
}

impl Drop for ReadGuard {
    fn drop(&mut self) {
        // 临时副本里可能有健康数据，用完必须删干净；删不掉要留日志而不是静默
        if let Err(e) = std::fs::remove_dir_all(&self.dir) {
            log::warn!("临时药箱副本没删掉（{}）：{e}", self.dir.display());
        }
    }
}

/// 把源库（含可能存在的 -wal / -shm）复制到 `tmp_dir` 下的独立子目录。
fn copy_sidecars(src: &Path, tmp_dir: &Path) -> Result<std::path::PathBuf> {
    std::fs::create_dir_all(tmp_dir)?;
    for suffix in ["", "-wal", "-shm"] {
        let from = std::path::PathBuf::from(format!("{}{suffix}", src.display()));
        if !from.exists() {
            continue;
        }
        let to = tmp_dir.join(format!("{}{suffix}", src.file_name().unwrap_or_default().to_string_lossy()));
        std::fs::copy(from, &to)?;
    }
    Ok(tmp_dir.join(src.file_name().ok_or_else(|| Error::msg("药箱库路径没有文件名"))?))
}

/// 读 + 折算成一个快照。失败也要产出快照（`missing` / `error`），
/// 让命令层只负责落库与推送 —— 降级判断在这里可单测，不用起界面。
pub fn snapshot_for(
    source: &Path,
    tmp_root: &Path,
    now: &dyn Now,
    payload_of: impl FnOnce(&[Medicine], &[Dose], i64, i32) -> String,
) -> Snapshot {
    let ms = now.ms();
    let day = now.today();
    let day_start = days::day_start_ms(day);
    let now_minutes = ((ms - day_start) / 60_000) as i32;

    match read_today(source, tmp_root, ms, day_start) {
        Ok((meds, doses)) => {
            let payload = payload_of(&meds, &doses, day_start, now_minutes);
            Snapshot {
                source: "medicine".into(),
                kind: "today".into(),
                fetched_at: ms,
                sig: sig::content_sig(&payload),
                status: Status::Ok,
                detail: String::new(),
                payload_json: payload,
            }
        }
        // 读不到就不生成任何待办、不算任何数（PLAN §6.1），界面折叠成虚线条
        Err(e) => Snapshot {
            source: "medicine".into(),
            kind: "today".into(),
            fetched_at: ms,
            sig: String::new(),
            status: if source.exists() { Status::Error } else { Status::Missing },
            detail: e.to_string(),
            payload_json: crate::linking::empty_payload(),
        },
    }
}

/// 读今天的用药数据。返回 `(药品, 今日打卡)`；调用方只拿聚合结果，原始行不外传。
pub fn read_today(src: &Path, tmp_root: &Path, now_ms: i64, day_start_ms: i64) -> Result<(Vec<Medicine>, Vec<Dose>)> {
    if !src.exists() {
        return Err(Error::msg("没找到药箱的库，可能已卸载或换过位置"));
    }

    let dir = tmp_root.join(format!("med-{}", now_ms.max(0)));
    let _guard = ReadGuard { dir: dir.clone() };
    let db_path = copy_sidecars(src, &dir)?;

    let conn = Connection::open_with_flags(
        &db_path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_PRIVATE_CACHE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|e| Error::msg(format!("只读打开药箱副本失败：{e}")))?;
    conn.busy_timeout(std::time::Duration::from_millis(2000))?;

    let mut meds = Vec::new();
    {
        let mut stmt = conn.prepare(
            "SELECT id, name, boxes, per_box, remaining, doses_per_day, amount_per_dose,
                    unit, low_days, times, created_at
             FROM medicines WHERE deleted = 0",
        )?;
        let rows = stmt.query_map([], Medicine::from_row)?;
        for row in rows {
            meds.push(row?);
        }
    }

    // 只取今天的打卡：把 320 行历史读进内存没有任何用处，且这是健康数据
    let mut doses = Vec::new();
    {
        let mut stmt = conn.prepare(
            "SELECT medicine_id, taken_at, amount FROM dose_logs
             WHERE deleted = 0 AND taken_at >= ?1 AND taken_at < ?2",
        )?;
        let rows = stmt.query_map(rusqlite::params![day_start_ms, day_start_ms + 86_400_000], |r| {
            Ok(Dose {
                medicine_id: r.get(0)?,
                taken_at: r.get(1)?,
                amount: r.get(2)?,
            })
        })?;
        for row in rows {
            doses.push(row?);
        }
    }

    drop(conn);
    Ok((meds, doses))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 自己造一个药箱形状的库来测读取路径 —— 不去碰用户真机上的那个文件。
    fn fixture(with_wal: bool) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("medicine_tracker.db");
        {
            let conn = Connection::open(&path).unwrap();
            if with_wal {
                conn.pragma_update(None, "journal_mode", "WAL").unwrap();
            }
            conn.execute_batch(
                "CREATE TABLE medicines (id TEXT PRIMARY KEY, name TEXT, boxes REAL, per_box REAL,
                    remaining REAL, doses_per_day REAL, amount_per_dose REAL, unit TEXT,
                    low_days INT, note TEXT, created_at INT, updated_at INT, deleted INT,
                    dirty INT, times TEXT);
                 CREATE TABLE dose_logs (id TEXT PRIMARY KEY, medicine_id TEXT, taken_at INT,
                    amount REAL, note TEXT, updated_at INT, deleted INT, dirty INT);",
            )
            .unwrap();
            conn.execute(
                "INSERT INTO medicines (id,name,boxes,per_box,remaining,doses_per_day,amount_per_dose,
                    unit,low_days,note,created_at,updated_at,deleted,dirty,times)
                 VALUES ('m1','二甲双胍',2,30,60,2,1,'片',7,'',0,1,0,0,'07:30,21:00'),
                        ('m2','已删的',1,10,1,1,1,'片',7,'',0,1,1,0,'')",
                [],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO dose_logs (id,medicine_id,taken_at,amount,note,updated_at,deleted,dirty)
                 VALUES ('d1','m1',?1,1,'',1,0,0), ('d2','m1',?2,0,'',1,0,0), ('d3','m1',?3,1,'',1,1,0)",
                rusqlite::params![1_000, 2_000, 3_000],
            )
            .unwrap();
            if with_wal {
                conn.pragma_update(None, "journal_mode", "DELETE").unwrap();
            }
        }
        dir
    }

    #[test]
    fn reads_only_live_rows_and_only_todays_logs() {
        let src_root = fixture(false);
        let tmp_root = tempfile::tempdir().unwrap();
        let db = src_root.path().join("medicine_tracker.db");

        let day_start = 1_000;
        let (meds, doses) = read_today(&db, tmp_root.path(), 5_000, day_start).unwrap();

        assert_eq!(meds.len(), 1, "deleted=1 的那支不该出现");
        assert_eq!(meds[0].name, "二甲双胍");
        assert_eq!(meds[0].schedule_minutes(), vec![450, 1260]);
        assert_eq!(meds[0].days_left(), Some(30.0));

        assert_eq!(doses.len(), 2, "窗口外的与 deleted=1 的都不该进来");
        assert_eq!(doses.iter().filter(|d| d.is_skipped()).count(), 1);
    }

    #[test]
    fn temp_copy_is_cleaned_up_after_reading() {
        let src_root = fixture(false);
        let tmp_root = tempfile::tempdir().unwrap();
        let db = src_root.path().join("medicine_tracker.db");

        read_today(&db, tmp_root.path(), 9_000, 1_000).unwrap();
        let leftovers: Vec<_> = std::fs::read_dir(tmp_root.path()).unwrap().collect();
        assert!(leftovers.is_empty(), "副本没删干净：{leftovers:?}");
    }

    #[test]
    fn a_missing_source_says_so_instead_of_returning_empty() {
        let tmp_root = tempfile::tempdir().unwrap();
        let err = read_today(
            Path::new(r"C:\no-such-app\medicine_tracker.db"),
            tmp_root.path(),
            1,
            0,
        )
        .unwrap_err();
        assert!(err.to_string().contains("没找到"), "{err}");
    }

    /// 降级判断在命令层之外，所以这里能不起界面就测掉（M3 验收的三种破坏场景之二、之三）
    #[test]
    fn snapshot_for_maps_the_three_failure_shapes() {
        use crate::linking::{Status};
        use crate::time::FixedNow;

        let tmp_root = tempfile::tempdir().unwrap();
        let payload_of = |_: &[Medicine], _: &[Dose], _: i64, _: i32| "{\"items\":[],\"count\":0}".to_owned();

        // 药箱被卸载：文件不在
        let gone = Path::new(r"C:\Users\hp\AppData\Roaming\dev.mine\nope\medicine_tracker.db");
        let snap = snapshot_for(gone, tmp_root.path(), &FixedNow(1_700_000_000_000), payload_of);
        assert_eq!(snap.status, Status::Missing);
        assert!(snap.detail.contains("没找到"), "{}", snap.detail);
        assert_eq!(snap.sig, "", "缺源的快照不该带内容指纹");

        // 文件在但不是 SQLite 库（比如被换成一个文本文件）
        let dir = tempfile::tempdir().unwrap();
        let bogus = dir.path().join("medicine_tracker.db");
        std::fs::write(&bogus, b"not a database at all, just bytes").unwrap();
        let snap = snapshot_for(&bogus, tmp_root.path(), &FixedNow(1_700_000_000_000), payload_of);
        assert_eq!(snap.status, Status::Error);
        assert!(!snap.detail.is_empty(), "要说清楚为什么读不了");
    }
}
