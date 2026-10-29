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
use super::{sig, Snapshot, Status, Today};
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

/// 云端那边的结果。**作为数据注入**，这样本模块不需要知道 `medicine_cloud` 的存在 ——
/// 读文件的和读网络的互相 import 过一次，降级路径就再也说不清是谁的责任了。
#[derive(Debug, Clone)]
pub enum CloudRead {
    /// 没配第二条应用密码 = 不启用云端，这不是失败，detail 里也不该出现云端字样
    Skipped,
    /// 配了但这次没读上来（断网 / 目录不存在 / 凭据被拒）
    Failed(String),
    /// 读到了：数据 + 哪台设备 + 它发布的时刻
    Fresh(Today, String, i64),
}

/// 读 + 折算成一个快照。失败也要产出快照（`missing` / `error`），
/// 让命令层只负责落库与推送 —— 降级判断在这里可单测，不用起界面。
///
/// 本机文件与云端快照**谁的行更新用谁**，`detail` 里必须写清用的是哪一份：
/// 云端那份可能是几小时前发布的，不写就是拿旧数据冒充实时（PLAN §2 原则 5）。
pub fn refresh(
    source: &Path,
    cloud: CloudRead,
    tmp_root: &Path,
    now: &dyn Now,
    payload_of: impl FnOnce(&[Medicine], &[Dose], i64, i32) -> String,
) -> Snapshot {
    let ms = now.ms();
    let day = now.today();
    let day_start = days::day_start_ms(day);
    let now_minutes = ((ms - day_start) / 60_000) as i32;

    let local = read_today(source, tmp_root, ms, day_start);
    let cloud_today = match &cloud {
        CloudRead::Fresh(t, device, published) => {
            Some((t.clone(), device.clone(), *published))
        }
        _ => None,
    };

    // 三条分支就是这件事的全部复杂度：本机赢、云端赢、两边都不行。
    let chosen: Option<(Today, String)> = match (&local, cloud_today) {
        (Ok(l), Some((c, device, published))) => {
            if super::cloud_is_fresher(c.fresh_ms, l.fresh_ms) {
                Some((c, format!("云端 · {device} 发布于 {}", ago(published, ms))))
            } else {
                Some((l.clone(), local_detail(&device, published, ms)))
            }
        }
        (Ok(l), None) => Some((
            l.clone(),
            match &cloud {
                // 配了云端却读不上，而 detail 里什么都不写 = 和「压根没配」长得一样，
                // 用户会以为云端在正常工作。这一句是唯一的报错出口。
                CloudRead::Failed(ce) => format!("本机 · 云端这次没读上：{ce}"),
                _ => String::new(),
            },
        )),
        // 本机读不到而云端读到了：这是云端存在的主要意义之一（药箱被卸载 / 库被移走）
        (Err(_), Some((c, device, published))) => {
            Some((c, format!("仅云端 · {device} 发布于 {}", ago(published, ms))))
        }
        (Err(e), None) => {
            let reason = match &cloud {
                CloudRead::Failed(ce) => format!("{e}；云端也没读上：{ce}"),
                _ => e.to_string(),
            };
            return Snapshot {
                source: "medicine".into(),
                kind: "today".into(),
                fetched_at: ms,
                sig: String::new(),
                status: if source.exists() { Status::Error } else { Status::Missing },
                detail: reason,
                payload_json: crate::linking::empty_payload(),
            };
        }
    };

    match chosen {
        Some((today, detail)) => {
            let payload = payload_of(&today.meds, &today.doses, day_start, now_minutes);
            Snapshot {
                source: "medicine".into(),
                kind: "today".into(),
                fetched_at: ms,
                sig: sig::content_sig(&payload),
                status: Status::Ok,
                detail,
                payload_json: payload,
            }
        }
        // 走到这里只可能是「本机失败 + 云端也失败」，上面已经 return 了
        None => Snapshot {
            source: "medicine".into(),
            kind: "today".into(),
            fetched_at: ms,
            sig: String::new(),
            status: Status::Error,
            detail: "本机与云端都没有数据".to_owned(),
            payload_json: crate::linking::empty_payload(),
        },
    }
}

/// 用本机那份时，如果云端其实也读上来了，要说一句"云端那份更旧"——
/// 否则用户会以为配了云端就一直在用云端。
fn local_detail(device: &str, published: i64, ms: i64) -> String {
    format!("本机 · 云端那份是 {device} {}前的", ago(published, ms))
}

/// 相对时刻的中文说法。云端那份的新鲜度必须让人一眼看出隔了多久。
fn ago(then: i64, now_ms: i64) -> String {
    let d = now_ms - then;
    if d < 90_000 {
        "刚刚".to_owned()
    } else if d < 3_600_000 {
        format!("{} 分钟前", d / 60_000)
    } else if d < 86_400_000 {
        format!("{} 小时前", d / 3_600_000)
    } else {
        format!("{} 天前", d / 86_400_000)
    }
}

/// 只有本机一条路时的入口（也是全部旧测试走的路径）。
pub fn snapshot_for(
    source: &Path,
    tmp_root: &Path,
    now: &dyn Now,
    payload_of: impl FnOnce(&[Medicine], &[Dose], i64, i32) -> String,
) -> Snapshot {
    refresh(source, CloudRead::Skipped, tmp_root, now, payload_of)
}

/// 读今天的用药数据。调用方只拿聚合结果与一个新鲜度，原始行不外传。
pub fn read_today(
    src: &Path,
    tmp_root: &Path,
    now_ms: i64,
    day_start_ms: i64,
) -> Result<Today> {
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

    // 新鲜度**含墓碑**：刚删掉一条药也是"这份数据比对方新"的证据。
    // 只看活动行的话，删除会看起来像"什么都没变"，于是本机永远追不上云端。
    let fresh_ms = conn
        .query_row(
            "SELECT max(u) FROM (
               SELECT max(updated_at) AS u FROM medicines
               UNION ALL SELECT max(updated_at) FROM dose_logs
             )",
            [],
            |r| r.get::<_, Option<i64>>(0),
        )
        .unwrap_or(None)
        .unwrap_or(0);

    drop(conn);
    Ok(Today {
        meds,
        doses,
        fresh_ms,
    })
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
        let today = read_today(&db, tmp_root.path(), 5_000, day_start).unwrap();
        let (meds, doses) = (&today.meds, &today.doses);

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

    // ---- 本机 / 云端取更新者 ----

    use crate::linking::Today;
    use crate::time::FixedNow;

    /// fixture 里所有行的 `updated_at` 都是 1，所以本机新鲜度 = 1，
    /// 造一个"比它新"或"不比它新"的云端数据只需要一个数。
    fn cloud_today(fresh: i64) -> Today {
        Today {
            meds: vec![Medicine {
                id: "m1".into(),
                name: "云端那份".into(),
                per_box: 30.0,
                remaining: 3.0,
                doses_per_day: 2.0,
                amount_per_dose: 1.0,
                unit: "片".into(),
                low_days: 7,
                times: String::new(),
                created_at: 0,
            }],
            doses: Vec::new(),
            fresh_ms: fresh,
        }
    }

    fn payload_of(meds: &[Medicine], _: &[Dose], _: i64, _: i32) -> String {
        meds.iter()
            .map(|m| m.name.clone())
            .collect::<Vec<_>>()
            .join(",")
    }

    fn refresh_with(cloud: CloudRead, source: &Path) -> crate::linking::Snapshot {
        let tmp = tempfile::tempdir().unwrap();
        refresh(
            source,
            cloud,
            tmp.path(),
            // 2023-11-15 前后，纯粹为了和 published_at 拉开一个可断言的差
            &FixedNow(1_700_000_000_000),
            payload_of,
        )
    }

    #[test]
    fn cloud_wins_only_when_its_rows_are_newer() {
        let src = fixture(false);
        let db = src.path().join("medicine_tracker.db");

        let snap = refresh_with(CloudRead::Fresh(cloud_today(99), "phone".into(), 1), &db);
        assert_eq!(snap.status, Status::Ok);
        assert_eq!(snap.payload_json, "云端那份", "云端更新时该用云端的数据");
        assert!(snap.detail.contains("云端"), "{}", snap.detail);
        assert!(snap.detail.contains("phone"), "要说清是哪台设备发布的");

        // 相等 → 本机；更旧 → 本机
        for fresh in [1, 0] {
            let snap = refresh_with(CloudRead::Fresh(cloud_today(fresh), "phone".into(), 1), &db);
            assert!(snap.payload_json.contains("二甲双胍"), "fresh={fresh} 时本机该赢");
            assert!(snap.detail.starts_with("本机"), "{}", snap.detail);
        }
    }

    #[test]
    fn using_the_local_copy_still_says_the_cloud_was_read() {
        let src = fixture(false);
        let db = src.path().join("medicine_tracker.db");
        let snap = refresh_with(CloudRead::Fresh(cloud_today(0), "phone".into(), 1), &db);
        // 配了云端不等于在用云端 —— 不写这一句，用户会一直以为看到的是手机的数据
        assert!(snap.detail.contains("云端那份是 phone"), "{}", snap.detail);
    }

    #[test]
    fn a_missing_local_app_falls_back_to_the_cloud() {
        let gone = Path::new(r"C:\Users\hp\AppData\Roaming\dev.mine\nope\medicine_tracker.db");
        let snap = refresh_with(
            CloudRead::Fresh(cloud_today(99), "phone".into(), 1_700_000_000_000 - 90_000),
            gone,
        );
        assert_eq!(snap.status, Status::Ok, "药箱被卸载时云端那份仍然可用");
        assert!(snap.detail.starts_with("仅云端"), "{}", snap.detail);
        assert!(snap.detail.contains("分钟前"), "发布时刻要翻成相对说法：{}", snap.detail);
    }

    #[test]
    fn both_unavailable_keeps_both_reasons() {
        let gone = Path::new(r"C:\Users\hp\AppData\Roaming\dev.mine\nope\medicine_tracker.db");
        let snap = refresh_with(CloudRead::Failed("坚果云返回了 404".into()), gone);
        assert_eq!(snap.status, Status::Missing);
        assert!(snap.detail.contains("没找到"), "{}", snap.detail);
        assert!(snap.detail.contains("404"), "两条路各自为什么不行都要说：{}", snap.detail);
    }

    #[test]
    fn no_second_credential_means_the_cloud_is_never_mentioned() {
        let src = fixture(false);
        let db = src.path().join("medicine_tracker.db");
        let snap = refresh_with(CloudRead::Skipped, &db);
        assert_eq!(snap.status, Status::Ok);
        assert!(snap.detail.is_empty(), "没启用云端就别提它：{}", snap.detail);
    }

    #[test]
    fn a_failed_cloud_is_not_silent_when_local_works() {
        let src = fixture(false);
        let db = src.path().join("medicine_tracker.db");
        let snap = refresh_with(CloudRead::Failed("坚果云返回了 401".into()), &db);
        assert_eq!(snap.status, Status::Ok, "本机那份照常可用");
        assert!(snap.payload_json.contains("二甲双胍"), "{}", snap.payload_json);
        // 这一句是「配了云端但读不上」唯一的可见出口，别让它和「没配」撞成一回事
        assert!(snap.detail.contains("云端这次没读上"), "{}", snap.detail);
        assert!(snap.detail.contains("401"), "{}", snap.detail);
    }
}
