// 拖拽排序用的分数索引（`todos.sort_key`）。
//
// 中点法插队：往两条之间放一项就取两者中点，不需要每次拖动重写整表。
// 浮点会被榨干（连续往同一缝隙里插几十次后中点等于端点），那时整区重排一次 ——
// 逻辑放在 Rust 而不是前端，是为了让 v2 手机提交的 sort_key 也过同一道校验。

/// 初始间距取 2 的幂：二进制下反复二分能多撑很多层才撞精度。
pub const GAP: f64 = 1024.0;

/// 取 a、b 之间的一个值；返回 None 表示缝隙已被浮点榨干，调用方该整区重排。
pub fn between(a: Option<f64>, b: Option<f64>) -> Option<f64> {
    match (a, b) {
        (None, None) => Some(GAP),
        (Some(x), None) => Some(x + GAP),
        (None, Some(y)) => Some(y - GAP),
        (Some(x), Some(y)) => {
            let mid = x + (y - x) / 2.0;
            if mid > x && mid < y {
                Some(mid)
            } else {
                None
            }
        }
    }
}

/// 整区重排后的新键：等距、保持当前顺序。
pub fn resequence(count: usize) -> Vec<f64> {
    (1..=count).map(|i| i as f64 * GAP).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inserts_at_the_edges_use_full_gap() {
        assert_eq!(between(None, None), Some(1024.0));
        assert_eq!(between(Some(1024.0), None), Some(2048.0));
        assert_eq!(between(None, Some(1024.0)), Some(0.0));
    }

    #[test]
    fn midpoint_keeps_order_and_reports_exhaustion() {
        let a = 1024.0f64;
        let mut b = 2048.0f64;
        // 一直往同一缝隙里插：最终必须诚实地说「插不下了」，而不是返回一个等于端点的键
        let mut steps = 0;
        while let Some(m) = between(Some(a), Some(b)) {
            assert!(m > a && m < b);
            b = m;
            steps += 1;
        }
        assert!(steps > 20, "f64 至少该撑住 20 多次同缝插队，实际 {steps}");
        assert!(steps < 80, "不该指望它无限撑，实际 {steps}");
    }

    #[test]
    fn resequence_is_monotonic_and_gap_spaced() {
        let keys = resequence(4);
        assert_eq!(keys, vec![1024.0, 2048.0, 3072.0, 4096.0]);
        assert!(resequence(0).is_empty());
    }
}
