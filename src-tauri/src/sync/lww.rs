use serde::{Deserialize, Serialize};

/// PLAN §5.1 的同步列打包成一个值。每张可变更表都嵌这一个结构，
/// 序列用时 `#[serde(flatten)]` 摊平，前端看到的就是 `rev / updatedAt / ...` 平铺字段。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Stamp {
    pub id: String,
    pub rev: i64,
    pub created_at: i64,
    pub created_by: String,
    pub updated_at: i64,
    pub updated_by: String,
    /// 软删墓碑：同步靠它传播删除，v1 也用它做「删除可撤销」
    pub deleted_at: Option<i64>,
}

impl Stamp {
    pub fn new(id: String, now_ms: i64, device: &str) -> Self {
        Self {
            id,
            rev: 1,
            created_at: now_ms,
            created_by: device.to_owned(),
            updated_at: now_ms,
            updated_by: device.to_owned(),
            deleted_at: None,
        }
    }

    pub fn is_tombstone(&self) -> bool {
        self.deleted_at.is_some()
    }
}

/// 每次写入唯一该做的事：`rev + 1`、盖时间、记设备。
/// 所有 repo 的 update 路径都必须经过这里，否则「最后写入设备」这种字段会慢慢失真。
pub fn touch(stamp: &mut Stamp, now_ms: i64, device: &str) {
    stamp.rev += 1;
    stamp.updated_at = now_ms;
    stamp.updated_by = device.to_owned();
}

/// 行级 Last-Write-Wins：先比墙钟，平票比设备 id 字典序。
/// 平票真的会发生（同一毫秒两台设备各写一次），没有裁决位就会每次跑出不一样结果。
/// 不引 CRDT：两台设备、编辑面基本不相交，CRDT 的成本远大于收益（PLAN §5.1）。
///
/// v2 手机提交变更时用的就是这一句；这里只提供规则，不提供「应用变更」的框架 ——
/// 等真有对端要合并时再写，避免凭空造抽象。
pub fn is_newer(candidate: (i64, &str), current: (i64, &str)) -> bool {
    candidate.0 > current.0 || (candidate.0 == current.0 && candidate.1 > current.1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn touch_bumps_rev_and_rewrites_writer() {
        let mut s = Stamp::new("t1".into(), 100, "pc-a");
        assert_eq!(
            (s.rev, s.updated_at, s.updated_by.as_str()),
            (1, 100, "pc-a")
        );
        touch(&mut s, 200, "pc-b");
        assert_eq!(
            (s.rev, s.updated_at, s.updated_by.as_str()),
            (2, 200, "pc-b")
        );
        // created_* 一旦落下就不动，统计的「来源」维度靠它
        assert_eq!((s.created_at, s.created_by.as_str()), (100, "pc-a"));
    }

    #[test]
    fn tie_on_clock_is_broken_by_device_id() {
        assert!(is_newer((100, "b"), (100, "a")));
        assert!(!is_newer((100, "a"), (100, "b")));
        // 自反性：同一行自己不该赢自己
        assert!(!is_newer((100, "a"), (100, "a")));
    }

    #[test]
    fn older_candidate_never_wins() {
        assert!(!is_newer((400, "pc-b"), (500, "pc-a")));
        assert!(is_newer((501, "pc-a"), (500, "pc-a")));
        // 设备 id 大不代表能赢时间戳：clock 是先决条件
        assert!(!is_newer((100, "pc-z"), (900, "pc-a")));
    }
}
