//! 从坚果云读药箱的设备快照。
//!
//! 药箱每台设备 PUT 的是**合并后的全量** —— 它自己源码里那句注释说得最清楚：
//! 「合并完之后整表发布自己这一份：第三台设备只读任意一份就能拿到全量」。
//! 所以这里**不重实现它的逐行 LWW 合并**，只挑 `published_at` 最大的那一份。
//! 少写一段合并逻辑，就少一处能把健康数据算错的地方。
//!
//! 只读由类型保证（见 [`super::dav_readonly`]）：这个模块压根拿不到发写请求的方法。

use serde_json::{Map, Value};

use super::dav_readonly::ReadRemote;
use super::medicine_math::{Dose, Medicine};
use super::Today;
use crate::error::{Error, Result};

/// 云端的一份可用快照 = 一份今天的药箱数据 + 它是谁、什么时候发布的。
/// 界面必须把后两样说出来：云端那份可能是几小时前发布的，
/// 不说来源就等于把旧数据当实时的用（PLAN §2 原则 5）。
#[derive(Debug, Clone)]
pub struct Fetched {
    pub today: Today,
    /// 这份快照来自哪台设备
    pub device: String,
    /// 该设备发布它的时刻（客户端时钟）
    pub published_at: i64,
    /// 目录里一共读到几份，用来解释"取的是哪一份"
    pub docs_read: usize,
}

pub fn fetch(remote: &ReadRemote) -> Result<Fetched> {
    let list = remote.list_devices()?;
    if list.is_empty() {
        return Err(Error::msg(
            "药箱的同步目录里还没有任何 device-*.json —— 它得先在药箱里点一次同步",
        ));
    }

    let mut got: Vec<Fetched> = Vec::new();
    let mut errs: Vec<String> = Vec::new();
    for entry in &list {
        match remote.get(&entry.name).and_then(|d| parse(&d.bytes)) {
            Ok(one) => got.push(one),
            Err(e) => errs.push(format!("{}：{e}", entry.name)),
        }
    }

    let mut chosen = choose(got).ok_or_else(|| {
        Error::msg(format!(
            "药箱快照一份都读不出来（{}）",
            if errs.is_empty() { "目录为空".to_owned() } else { errs.join("；") }
        ))
    })?;
    chosen.docs_read = list.len();
    Ok(chosen)
}

/// 多份设备快照里取发布时刻最新的那一份。
///
/// 单独成函数是因为"哪份算新"是这件事里唯一有判断成分的一步，值得被直接测到。
fn choose(all: Vec<Fetched>) -> Option<Fetched> {
    all.into_iter()
        .reduce(|acc, cur| if acc.published_at >= cur.published_at { acc } else { cur })
}

fn parse(bytes: &[u8]) -> Result<Fetched> {
    let doc: Value = serde_json::from_slice(bytes)
        .map_err(|e| Error::msg(format!("设备快照不是合法 JSON：{e}")))?;
    let obj = doc.as_object().ok_or_else(|| Error::msg("设备快照的顶层不是对象"))?;

    let mut fresh = 0i64;
    let mut meds = Vec::new();
    for row in arr(obj, "medicines") {
        let Some(row) = row.as_object() else { continue };
        // 先取 updated_at 再判 deleted：墓碑也是新鲜度
        fresh = fresh.max(as_i64(row.get("updated_at")));
        if as_i64(row.get("deleted")) != 0 {
            continue;
        }
        if let Some(m) = medicine_from_json(row) {
            meds.push(m);
        }
    }

    let mut doses = Vec::new();
    for row in arr(obj, "dose_logs") {
        let Some(row) = row.as_object() else { continue };
        fresh = fresh.max(as_i64(row.get("updated_at")));
        if as_i64(row.get("deleted")) != 0 {
            continue;
        }
        if let Some(d) = dose_from_json(row) {
            doses.push(d);
        }
    }

    Ok(Fetched {
        today: Today {
            meds,
            doses,
            fresh_ms: fresh,
        },
        device: obj
            .get("device")
            .and_then(Value::as_str)
            .unwrap_or("未知设备")
            .to_owned(),
        published_at: as_i64(obj.get("published_at")),
        docs_read: 0,
    })
}

fn arr<'a>(obj: &'a Map<String, Value>, key: &str) -> &'a [Value] {
    obj.get(key)
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
}

fn as_i64(v: Option<&Value>) -> i64 {
    v.and_then(Value::as_f64).map(|f| f as i64).unwrap_or(0)
}

fn as_i64_or(v: Option<&Value>, default: i64) -> i64 {
    v.and_then(Value::as_f64).map(|f| f as i64).unwrap_or(default)
}

fn as_f64(v: Option<&Value>) -> Option<f64> {
    v.and_then(Value::as_f64)
}

/// 与 `Medicine::from_row` 同一条兜底规则：`remaining` 是库存的唯一真相，
/// 老数据没写它时才回退成 `boxes × per_box`。两处口径不一致会出现
/// "本机看是一个数、云端看是另一个数"。
fn medicine_from_json(row: &Map<String, Value>) -> Option<Medicine> {
    let id = row.get("id").and_then(Value::as_str)?;
    let per_box = as_f64(row.get("per_box")).unwrap_or(0.0);
    let boxes = as_f64(row.get("boxes")).unwrap_or(0.0);
    Some(Medicine {
        id: id.to_owned(),
        name: row
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        per_box,
        remaining: as_f64(row.get("remaining")).unwrap_or(boxes * per_box),
        doses_per_day: as_f64(row.get("doses_per_day")).unwrap_or(0.0),
        amount_per_dose: as_f64(row.get("amount_per_dose")).unwrap_or(0.0),
        unit: row
            .get("unit")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .unwrap_or("片")
            .to_owned(),
        low_days: as_i64_or(row.get("low_days"), 7) as i32,
        times: row
            .get("times")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        created_at: as_i64(row.get("created_at")),
    })
}

fn dose_from_json(row: &Map<String, Value>) -> Option<Dose> {
    Some(Dose {
        medicine_id: row.get("medicine_id").and_then(Value::as_str)?.to_owned(),
        taken_at: as_i64(row.get("taken_at")),
        amount: as_f64(row.get("amount")).unwrap_or(0.0),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 合成数据 —— 真实用药记录不进测试，也不该进这个仓库。
    fn doc(device: &str, published_at: i64, meds: &str, doses: &str) -> Vec<u8> {
        format!(
            r#"{{"device":"{device}","published_at":{published_at},
                 "medicines":{meds},"dose_logs":{doses}}}"#
        )
        .into_bytes()
    }

    const AMOX_ROW: &str = r#"{"id":"m1","name":"阿莫西林","boxes":1,"per_box":20,
        "remaining":6,"doses_per_day":2,"amount_per_dose":1,"unit":"粒","low_days":3,
        "times":"08:00,20:00","created_at":1,"updated_at":500,"deleted":0}"#;

    #[test]
    fn reads_a_full_snapshot_and_keeps_the_newest_publish() {
        let amox = format!("[{AMOX_ROW}]");
        let old = parse(&doc("pc", 1_000, &amox, "[]")).unwrap();
        let fresh = parse(&doc("phone", 2_000, &amox, "[]")).unwrap();
        assert_eq!(old.published_at, 1_000);

        let chosen = choose(vec![old.clone(), fresh.clone()]).unwrap();
        assert_eq!(chosen.device, "phone", "取发布最新的那一份");
        // 顺序反过来也要选到同一份，reduce 不该受入参顺序影响
        assert_eq!(choose(vec![fresh, old]).unwrap().device, "phone");
    }

    #[test]
    fn tombstones_are_excluded_but_still_count_as_freshness() {
        let meds = format!(
            "[{AMOX_ROW},{{\"id\":\"m2\",\"name\":\"已删的药\",\"remaining\":9,\"updated_at\":900,\"deleted\":1}}]"
        );
        let doses = r#"[{"id":"d1","medicine_id":"m1","taken_at":5,"amount":1,"updated_at":700,"deleted":0},
                {"id":"d2","medicine_id":"m1","taken_at":6,"amount":1,"updated_at":999,"deleted":1}]"#;
        let got = parse(&doc("pc", 10, &meds, doses)).unwrap();
        assert_eq!(got.today.meds.len(), 1, "墓碑不该出现在库存里：{:?}", got.today.meds);
        assert_eq!(got.today.doses.len(), 1);
        // 999 来自一条被删掉的打卡：删除也是"这份数据比本机新"的证据
        assert_eq!(got.today.fresh_ms, 999);
    }

    #[test]
    fn old_rows_without_remaining_fall_back_to_boxes_times_per_box() {
        let bytes = doc(
            "pc",
            1,
            r#"[{"id":"m1","name":"老数据","boxes":2,"per_box":10,"updated_at":1,"deleted":0}]"#,
            "[]",
        );
        let got = parse(&bytes).unwrap();
        assert_eq!(got.today.meds[0].remaining, 20.0);
        assert_eq!(got.today.meds[0].unit, "片", "缺 unit 时与 from_row 同一兜底");
        assert_eq!(got.today.meds[0].low_days, 7, "缺 low_days 时与 from_row 同一兜底");
    }

    #[test]
    fn a_row_without_an_id_is_dropped_rather_than_panicking() {
        let bytes = doc(
            "pc",
            1,
            r#"[{"name":"没有 id 的行","updated_at":1,"deleted":0}]"#,
            "[]",
        );
        let got = parse(&bytes).unwrap();
        assert!(got.today.meds.is_empty());
    }

    #[test]
    fn garbage_is_reported_not_swallowed() {
        let err = parse(b"not json").unwrap_err().to_string();
        assert!(err.contains("JSON"), "{err}");
        let err = parse(b"[1,2]").unwrap_err().to_string();
        assert!(err.contains("顶层"), "{err}");
        // 一份都读不出来时，fetch 的报错要带上原因（这里直接验 choose）
        assert!(choose(Vec::new()).is_none());
    }
}
