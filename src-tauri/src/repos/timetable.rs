//! 课表：教务导出的那张 CSV 网格 → `classes` 表。
//!
//! 形状是**矩阵不是清单**（列 = 星期，行 = 节次，格子里塞多行字段），
//! 所以这里读的是"格子"而不是"行"。真实样例来自 2026-10-30 的教务导出：
//! 一个格子可能装**两段**（同一门课的不同周次），字段顺序固定为
//! 课程名 / 教师 / 周次 / 教室 / 节次。
//!
//! 编码只接 UTF-8：中文 Windows 上 Excel 的「CSV(逗号分隔)」写的是 GBK，
//! 而 GBK 解码要引一个依赖，不值得 —— 判出来不是 UTF-8 就明说该怎么另存。

use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;

use crate::error::{Error, Result};
use crate::time::Now;

/// 一行 = 一个「星期 × 节次 × 周次集合」。同一门课常有多行。
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClassRow {
    pub name: String,
    pub teacher: String,
    pub room: String,
    /// 1 = 周一 … 7 = 周日
    pub day: i32,
    pub start_min: i32,
    pub end_min: i32,
    pub period: String,
    /// 规范化后的周次集合，如 `1-10,12-16`
    pub weeks: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub term: String,
    pub rows: usize,
    pub courses: usize,
    /// 第 1 周周日的日期还没设过 —— 界面要拿这个去提示"先定开学日"，
    /// 否则"今天第几周"根本无从算起，硬猜会猜错一整个学期。
    pub needs_week1: bool,
}

/// settings 里的那两条：当前学期 + 第 1 周的周一（ISO 日期）
pub const KEY_TERM: &str = "timetable.term";
pub const KEY_WEEK1: &str = "timetable.week1Monday";

const DAY_CHARS: [char; 7] = ['一', '二', '三', '四', '五', '六', '日'];

/// 逗号分隔（RFC4180 那一套：引号内允许逗号与换行，`""` 是引号本身）。
/// 自己写而不引 csv crate：状态机 30 行，而依赖面是"能读中文 Excel 的导出"这种坑货。
fn split_csv(text: &str) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut cell = String::new();
    let mut quoted = false;
    let mut it = text.chars().peekable();
    while let Some(c) = it.next() {
        if quoted {
            if c == '"' {
                if it.peek() == Some(&'"') {
                    cell.push('"');
                    it.next();
                } else {
                    quoted = false;
                }
            } else {
                cell.push(c);
            }
            continue;
        }
        match c {
            '"' => quoted = true,
            ',' => {
                row.push(std::mem::take(&mut cell));
            }
            '\n' | '\r' => {
                if c == '\r' && it.peek() == Some(&'\n') {
                    it.next();
                }
                row.push(std::mem::take(&mut cell));
                rows.push(std::mem::take(&mut row));
            }
            _ => cell.push(c),
        }
    }
    if !cell.is_empty() || !row.is_empty() {
        row.push(cell);
        rows.push(row);
    }
    rows
}

/// 一行里出现了几个不同的「星期X」：够 5 个才当表头行。
/// 数的是**不同的星期字**，所以同一行里重复出现「星期一」也不会被误判成表头。
fn day_count(row: &[String]) -> usize {
    DAY_CHARS
        .iter()
        .filter(|d| {
            row.iter()
                .any(|c| c.trim().starts_with("星期") && c.contains(**d))
        })
        .count()
}

/// 认 `08:00-09:40` 这种时间范围。找不到就不是节次行。
fn time_range(text: &str) -> Option<(i32, i32)> {
    let re = |s: &str| -> Option<i32> {
        let (h, m) = s.split_once(':')?;
        Some(h.parse::<i32>().ok()? * 60 + m.parse::<i32>().ok()?)
    };
    for line in text.lines() {
        let t = line.trim();
        if let Some((a, b)) = t.split_once('-') {
            if let (Some(s), Some(e)) = (re(a), re(b)) {
                return Some((s, e));
            }
        }
    }
    None
}

/// 周次那行长这样：`1-10,12-16[周]` 或 `1,3,5,7,9[周]`。返回去掉 `[周]` 的规范串。
fn weeks_of(line: &str) -> Option<String> {
    let t = line.trim().trim_end_matches("[周]").trim();
    if t.is_empty() {
        return None;
    }
    t.split(',').all(|part| {
        part.split('-').all(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
    })
    .then(|| t.to_owned())
}

/// 一个格子 → 若干节课。字段顺序固定五段，段数不对就明说，不猜。
fn parse_cell(cell: &str, day: i32, start: i32, end: i32, period: &str) -> Result<Vec<ClassRow>> {
    let lines: Vec<&str> = cell.lines().map(str::trim).filter(|s| !s.is_empty()).collect();
    if lines.is_empty() {
        return Ok(Vec::new());
    }
    if !lines.len().is_multiple_of(5) {
        return Err(Error::msg(format!(
            "星期{} 的格子里有 {} 行，不是 5 的倍数（课程名/教师/周次/教室/节次 一组）：{}",
            day,
            lines.len(),
            lines.first().copied().unwrap_or("")
        )));
    }
    let mut out = Vec::new();
    for g in lines.chunks(5) {
        // 第 3 段必须是周次，否则整组错位 —— 错位着导入会把教师当教室存进去
        let Some(weeks) = weeks_of(g[2]) else {
            return Err(Error::msg(format!(
                "星期{} 的格子第 3 行不是周次（拿到「{}」），这份导出与预期形状不同",
                day, g[2]
            )));
        };
        out.push(ClassRow {
            name: g[0].to_owned(),
            teacher: g[1].to_owned(),
            room: g[3].to_owned(),
            day,
            start_min: start,
            end_min: end,
            period: if g[4].is_empty() { period.to_owned() } else { format!("{period} {}", g[4]) },
            weeks,
        });
    }
    Ok(out)
}

/// 整张表 → 学期 + 行。错误信息都指向具体格子，让人能自己回去看那份文件。
pub fn parse(text: &str) -> Result<(String, Vec<ClassRow>)> {
    let rows = split_csv(text);
    let term = rows
        .iter()
        .flatten()
        .find_map(|c| {
            let t = c.trim();
            let after = t.split("学年学期").nth(1)?;
            let rest = after.trim_start_matches(['：', ':', ' ']);
            rest.split([' ', '\t']).next().filter(|s| !s.is_empty()).map(str::to_owned)
        })
        .ok_or_else(|| Error::msg("这份文件里没有「学年学期」那一行，不像教务导出的课表"))?;

    let header_at = rows
        .iter()
        .position(|r| day_count(r) >= 5)
        .ok_or_else(|| Error::msg("没找到 星期一…星期日 的表头行"))?;
    let header = &rows[header_at];
    // 编号取自**认出来的那个星期字**，不是「第几个看起来像星期的列」。
    // 按顺序数会出事：多出一列「周末安排」就把编号推到 8，撞上 `CHECK (day BETWEEN 1 AND 7)`，
    // 而那已经是 DELETE 之后了。认不出星期字的列一律不当星期用。
    let mut taken: std::collections::HashSet<i32> = std::collections::HashSet::new();
    let days: Vec<(usize, i32)> = header
        .iter()
        .enumerate()
        .filter_map(|(col, c)| {
            let t = c.trim();
            let rest = t.strip_prefix("星期").or_else(|| t.strip_prefix('周'))?;
            let name = rest.chars().next()?;
            DAY_CHARS
                .iter()
                .position(|d| *d == name)
                .map(|i| (col, i as i32 + 1))
        })
        // 同一个星期出现两次（有的导出会重一列）：只认第一个，别让一门课被写两遍
        .filter(|(_, day)| taken.insert(*day))
        .collect();

    let mut out = Vec::new();
    for r in rows.iter().skip(header_at + 1) {
        if r.is_empty() {
            continue;
        }
        let Some((start, end)) = time_range(&r[0]) else { continue };
        let period = r[0].lines().next().unwrap_or("").trim().to_owned();
        for (col, day) in &days {
            let Some(cell) = r.get(*col) else { continue };
            out.extend(parse_cell(cell, *day, start, end, &period)?);
        }
    }
    if out.is_empty() {
        return Err(Error::msg("认出了表头，但格子里一节课都没有"));
    }
    Ok((term, out))
}

/// 周次集合是否包含第 n 周。`1-10,12-16` 里 11 不算。
pub fn weeks_include(spec: &str, week: i32) -> bool {
    spec.split(',').any(|part| {
        let part = part.trim();
        match part.split_once('-') {
            Some((a, b)) => a.parse::<i32>().unwrap_or(i32::MAX) <= week
                && week <= b.parse::<i32>().unwrap_or(i32::MIN),
            None => part.parse::<i32>().map(|n| n == week).unwrap_or(false),
        }
    })
}

/// 导入即整学期替换：课表是"外部真相的一份副本"，留着旧的只会让两周的课叠在一起。
pub fn import(conn: &Connection, now: &dyn Now, device: &str, text: &str) -> Result<Report> {
    let (term, rows) = parse(text)?;
    let ms = now.ms();

    // 「先删后插」中间任何一步失败都会把这一学期留在空表上，所以这一整段是原子的。
    // repo 层收 `&Connection`、事务通常由命令层持有（见 tags.rs 的说明）—— 这条例外是因为
    // 只有这里知道删与插是同一件事，命令层拿着裸 conn 无从判断。
    let tx = conn.unchecked_transaction()?;

    tx.execute("DELETE FROM classes WHERE term = ?1", params![term])?;
    let mut ids = std::collections::HashSet::new();
    for r in &rows {
        let id = uuid::Uuid::now_v7().simple().to_string();
        ids.insert(r.name.clone());
        tx.execute(
            "INSERT INTO classes (id, rev, created_at, created_by, updated_at, updated_by,
                                  deleted_at, term, name, teacher, room, day,
                                  start_min, end_min, period, weeks)
             VALUES (?1,1,?2,?3,?2,?3,NULL,?4,?5,?6,?7,?8,?9,?10,?11,?12)",
            params![id, ms, device, term, r.name, r.teacher, r.room, r.day, r.start_min, r.end_min, r.period, r.weeks],
        )?;
    }

    let had_week1 = tx
        .query_row(
            "SELECT value_json FROM settings WHERE key = ?1",
            params![KEY_WEEK1],
            |row| row.get::<_, String>(0),
        )
        .optional()?
        .is_some_and(|v| !v.contains("null"));
    if !had_week1 {
        tx.execute(
            "INSERT INTO settings (key, value_json, updated_at, updated_by)
             VALUES (?1, 'null', ?2, ?3)
             ON CONFLICT(key) DO UPDATE SET value_json = excluded.value_json, updated_at = excluded.updated_at",
            params![KEY_WEEK1, ms, device],
        )?;
    }
    tx.execute(
        "INSERT INTO settings (key, value_json, updated_at, updated_by)
         VALUES (?1, ?4, ?2, ?3)
         ON CONFLICT(key) DO UPDATE SET value_json = excluded.value_json, updated_at = excluded.updated_at",
        params![KEY_TERM, ms, device, format!("\"{term}\"")],
    )?;

    let report = Report {
        courses: ids.len(),
        rows: rows.len(),
        term,
        needs_week1: !had_week1,
    };
    tx.commit()?;
    Ok(report)
}

/// 今天在第几周。开学日没设 / 还没开学 / 学期已结束 → `None`，
/// 而不是回一个 0 或者硬猜 —— 猜错会把一整个学期的课表显示成另一周的样子。
pub fn week_of(week1_monday_day: i32, today_day: i32, total_weeks: i32) -> Option<i32> {
    let elapsed = crate::time::days::days_between(week1_monday_day, today_day);
    if elapsed < 0 {
        return None;
    }
    let week = (elapsed / 7 + 1) as i32;
    (week <= total_weeks).then_some(week)
}

/// 一段周次集合的最高周，如 `1-10,12-16` → 16。
fn weeks_high(spec: &str) -> Option<i32> {
    spec.split(',')
        .filter_map(|part| part.trim().split('-').next_back())
        .filter_map(|n| n.parse::<i32>().ok())
        .max()
}

/// 学期总周数：CSV 里没有这一项，从各课周次集合的最大端推一个默认值（界面允许改）。
fn max_week(specs: impl Iterator<Item = String>) -> Option<i32> {
    specs.filter_map(|s| weeks_high(&s)).max()
}

/// 网格的一行（节次）。整学期的骨架，不随"这一周有没有课"变化。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PeriodLabel {
    pub start_min: i32,
    pub label: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StateView {
    pub term: Option<String>,
    /// ISO 日期 `yyyy-mm-dd`；没设过时为 None —— 界面据此要求填开学日
    pub week1_monday: Option<String>,
    pub current_week: Option<i32>,
    pub total_weeks: Option<i32>,
    pub rows: usize,
    pub courses: usize,
    /// 为什么要把节次放进 state：某一节课可能整周都不上（第 11 周那种空档周），
    /// 只按"本周的课"画行会让网格塌成一片空白，用户看到的是"课表丢了"。
    pub periods: Vec<PeriodLabel>,
}

fn setting_str(conn: &Connection, key: &str) -> Option<String> {
    conn.query_row("SELECT value_json FROM settings WHERE key = ?1", params![key], |r| {
        r.get::<_, String>(0)
    })
    .optional()
    .ok()
    .flatten()
    .and_then(|v| {
        let t = v.trim().trim_matches('"').to_owned();
        (!t.is_empty() && t != "null").then_some(t)
    })
}

fn put_setting(conn: &Connection, key: &str, json: &str, ms: i64, device: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO settings (key, value_json, updated_at, updated_by)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(key) DO UPDATE SET value_json = excluded.value_json, updated_at = excluded.updated_at",
        params![key, json, ms, device],
    )?;
    Ok(())
}

/// `2026-09-07` → `20260907`（本地日整数）。校验到能拒绝乱填的程度即可。
pub fn iso_to_day(iso: &str) -> Result<i32> {
    let parts: Vec<&str> = iso.trim().split('-').collect();
    if parts.len() != 3 || parts[0].len() != 4 || parts[1].len() != 2 || parts[2].len() != 2 {
        return Err(Error::msg("开学日要用 yyyy-mm-dd 这种写法"));
    }
    let (y, mo, day) = (
        parts[0].parse::<i32>(),
        parts[1].parse::<u32>(),
        parts[2].parse::<u32>(),
    );
    let (Ok(y), Ok(mo), Ok(day)) = (y, mo, day) else {
        return Err(Error::msg("开学日不是合法日期"));
    };
    // 让日历而不是 `1..=31` 来判断：「格式对但那一天不存在」(2026-02-31、2026-04-31)
    // 以前会被收下来，之后 `days_between` 认不出它、返回间隔 0 天，
    // 于是周次恒等于第 1 周 —— 一整个学期都显示成开学第一周。
    if chrono::NaiveDate::from_ymd_opt(y, mo, day).is_none() {
        return Err(Error::msg("日历里没有这一天"));
    }
    Ok(y * 10000 + mo as i32 * 100 + day as i32)
}

pub fn state(conn: &Connection, now: &dyn Now) -> Result<StateView> {
    let term = setting_str(conn, KEY_TERM);
    let week1 = setting_str(conn, KEY_WEEK1);
    let want = term.clone().unwrap_or_default();
    let mut stmt = conn.prepare(
        "SELECT weeks, name, start_min, period FROM classes
         WHERE deleted_at IS NULL AND (?1 = '' OR term = ?1) ORDER BY start_min",
    )?;
    let pairs: Vec<(String, String, i32, String)> = stmt
        .query_map(params![want], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(stmt);

    let total_weeks = max_week(pairs.iter().map(|(w, _, _, _)| w.clone()));

    // 节次骨架：同一 start_min 只留一条，标签取第一个非空的原文
    let mut seen: std::collections::HashMap<i32, String> = std::collections::HashMap::new();
    for (_, _, start, period) in &pairs {
        let e = seen.entry(*start).or_insert_with(|| period.clone());
        if e.is_empty() && !period.is_empty() {
            *e = period.clone();
        }
    }
    let mut periods: Vec<PeriodLabel> = seen
        .into_iter()
        .map(|(start_min, label)| PeriodLabel { start_min, label })
        .collect();
    periods.sort_by_key(|p| p.start_min);

    let current_week = match (week1.as_deref(), total_weeks) {
        (Some(iso), Some(tw)) => iso_to_day(iso)
            .ok()
            .and_then(|monday| week_of(monday, now.today(), tw)),
        _ => None,
    };

    let courses = pairs
        .iter()
        .map(|(_, n, _, _)| n.as_str())
        .collect::<std::collections::HashSet<_>>()
        .len();
    Ok(StateView {
        term,
        week1_monday: week1,
        current_week,
        total_weeks,
        rows: pairs.len(),
        courses,
        periods,
    })
}

pub fn set_week1(conn: &Connection, now: &dyn Now, device: &str, iso: &str) -> Result<StateView> {
    let trimmed = iso.trim();
    if trimmed.is_empty() {
        // 清空 = 回到"还没设"，界面重新提示，而不是留一个假日期在那儿算周次
        put_setting(conn, KEY_WEEK1, "null", now.ms(), device)?;
    } else {
        iso_to_day(trimmed)?;
        // 存 ISO 原样：input[type=date] 回填要的就是这个格式，整数日留给算周次时现转
        put_setting(conn, KEY_WEEK1, &format!("\"{trimmed}\""), now.ms(), device)?;
    }
    state(conn, now)
}

/// 某一周的课。`week=None` 用当前周；没设开学日时返回空而不是猜。
pub fn week_rows(conn: &Connection, now: &dyn Now, week: Option<i32>) -> Result<(Option<i32>, Vec<ClassRow>)> {
    let st = state(conn, now)?;
    let Some(week) = week.or(st.current_week) else {
        return Ok((None, Vec::new()));
    };
    let term = st.term.unwrap_or_default();
    let mut stmt = conn.prepare(
        "SELECT name, teacher, room, day, start_min, end_min, period, weeks
         FROM classes WHERE deleted_at IS NULL AND term = ?1 ORDER BY day, start_min",
    )?;
    let all: Vec<ClassRow> = stmt
        .query_map(params![term], |r| {
            Ok(ClassRow {
                name: r.get(0)?,
                teacher: r.get(1)?,
                room: r.get(2)?,
                day: r.get(3)?,
                start_min: r.get(4)?,
                end_min: r.get(5)?,
                period: r.get(6)?,
                weeks: r.get(7)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(stmt);
    Ok((Some(week), all.into_iter().filter(|c| weeks_include(&c.weeks, week)).collect()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 与真实导出同形状（格子内带换行、整格加引号），名字全是假的 —— 仓库里不留任何人的课表。
    const CSV: &str = r#"烟台某学院 张三 学生个人课表,,,,,,,
学年学期：2026-2027-1        班级：24级计算机,,,,,,
,星期一,星期二,星期三,星期四,星期五,星期六,星期日
"第一大节
08:00-09:40","
操作系统
徐老师
1-10,12-16[周]
[主校区]二教202
[01-02]节","
大学体育5
郭老师
1,3,5,7,9[周]
[主校区]北操场
[01-02]节",,,"
体系结构
程老师
2,4,6[周]
[主校区]三教101
[01-02]节",,
"第二大节
10:00-11:40",,"
人工智能
刘老师
1-8[周]
[主校区]一教101
[03-04]节
人工智能
刘老师
9-16[周]
[主校区]一教102
[03-04]节",,,,,,
"#;

    fn rows() -> Vec<ClassRow> {
        parse(CSV).expect("该能解析").1
    }

    #[test]
    fn reads_the_grid_not_rows() {
        let (term, got) = parse(CSV).unwrap();
        assert_eq!(term, "2026-2027-1");
        // 周一 08:00 两门 + 周五 08:00 一门 + 周二 10:00 两段
        assert_eq!(got.len(), 5, "格子拆出来的段数：{got:?}");
        let os = got.iter().find(|c| c.name == "操作系统").unwrap();
        assert_eq!((os.day, os.start_min, os.end_min), (1, 480, 580));
        assert_eq!(os.teacher, "徐老师");
        assert_eq!(os.room, "[主校区]二教202");
        assert_eq!(os.weeks, "1-10,12-16", "[周] 这个尾巴要去掉");
        assert!(os.period.contains("第一大节"));
    }

    #[test]
    fn one_cell_can_hold_two_blocks_of_the_same_course() {
        let ai = rows().into_iter().filter(|c| c.name == "人工智能").collect::<Vec<_>>();
        assert_eq!(ai.len(), 2, "同一格两段都要进来");
        assert_eq!(ai[0].weeks, "1-8");
        assert_eq!(ai[1].weeks, "9-16");
        assert_eq!(ai[0].day, 2);
    }

    #[test]
    fn week_ranges_are_inclusive_and_gaps_stay_gaps() {
        assert!(weeks_include("1-10,12-16", 1));
        assert!(weeks_include("1-10,12-16", 10));
        assert!(!weeks_include("1-10,12-16", 11), "空出来的那一周不该算有课");
        assert!(weeks_include("1-10,12-16", 16));
        assert!(weeks_include("1,3,5,7,9", 5));
        assert!(!weeks_include("1,3,5,7,9", 6));
        assert!(weeks_include("17", 17));
    }

    #[test]
    fn a_shaped_wrong_cell_says_which_one_instead_of_guessing() {
        // 第 3 段不是周次 = 字段错位。宁可拒绝导入，也不能把教师当教室存进去。
        let bad = CSV.replace("1-10,12-16[周]", "徐老师");
        let err = parse(&bad).unwrap_err().to_string();
        assert!(err.contains("星期1"), "{err}");
        assert!(err.contains("不是周次"), "{err}");
    }

    #[test]
    fn a_file_that_is_not_a_timetable_is_rejected_not_ignored() {
        let err = parse("姓名,学号\n张三,20240101\n").unwrap_err().to_string();
        assert!(err.contains("学年学期") || err.contains("表头"), "{err}");
    }

    /// 拿真文件验一次的钩子，默认不跑：它读的是本机路径，仓库里不留任何人的课表。
    /// 用法见 PROGRESS「复现验证的命令」：先转成 UTF-8，再带 env 跑这一条。
    #[test]
    #[ignore = "要 ANVIL_TIMETABLE_FILE 指向一份 UTF-8 的教务导出"]
    fn parses_a_real_export_when_told_to() {
        let path = std::env::var("ANVIL_TIMETABLE_FILE").expect("没设 ANVIL_TIMETABLE_FILE");
        let text = std::fs::read_to_string(&path).expect("读文件");
        let (term, rows) = parse(&text).expect("真文件该能解析");
        let names: std::collections::BTreeSet<&str> = rows.iter().map(|r| r.name.as_str()).collect();
        // 只报聚合：学期、时段数、门数。课程名与教师不打印到任何地方。
        eprintln!("学期 {term} · {} 个时段 · {} 门课", rows.len(), names.len());
        assert!(rows.len() >= 5, "真课表至少该有时段");
        assert!(rows.iter().all(|r| (1..=7).contains(&r.day)), "星期必须落在 1..7");
        assert!(rows.iter().all(|r| r.start_min < r.end_min), "开始要早于结束");
        assert!(rows.iter().all(|r| !r.name.is_empty() && !r.room.is_empty()));
    }

    #[test]
    fn importing_replaces_the_same_term_only() {
        let mut conn = Connection::open_in_memory().unwrap();
        crate::db::migrations::migrate(&mut conn).unwrap();
        let now = crate::time::FixedNow(1_700_000_000_000);
        let first = import(&conn, &now, "pc", CSV).unwrap();
        assert_eq!(first.rows, 5);
        assert!(first.needs_week1, "第一次导入后开学日还没设，界面要提示");

        // 再导一次同一学期：不该越导越多
        let again = import(&conn, &now, "pc", CSV).unwrap();
        assert_eq!(again.rows, first.rows);
        assert_eq!(again.courses, first.courses);
        let n: i64 = conn
            .query_row("SELECT count(*) FROM classes", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 5);
        // §6.7 边界 2：课不是待办。导入一行都不该往 todos 里写。
        let todos: i64 = conn.query_row("SELECT count(*) FROM todos", [], |r| r.get(0)).unwrap();
        assert_eq!(todos, 0, "导入课表不该顺手造待办");
    }

    #[test]
    fn an_extra_non_day_column_does_not_become_an_eighth_weekday() {
        // 「周末安排」这类列以前会被当成第 8 个星期：编号撞 CHECK(day BETWEEN 1 AND 7)，
        // 而那时整学期的 DELETE 已经执行完了。现在它只是一列被忽略掉的东西。
        let with_extra = CSV.replace(
            ",星期一,星期二,星期三,星期四,星期五,星期六,星期日",
            ",星期一,星期二,星期三,星期四,星期五,星期六,星期日,周末安排",
        );
        let mut conn = Connection::open_in_memory().unwrap();
        crate::db::migrations::migrate(&mut conn).unwrap();
        let now = crate::time::FixedNow(1_700_000_000_000);

        let report = import(&conn, &now, "pc", &with_extra).expect("多一列不是星期的列，不该毁掉整次导入");
        assert_eq!(report.rows, 5);
        let out_of_range: i64 = conn
            .query_row(
                "SELECT count(*) FROM classes WHERE day NOT BETWEEN 1 AND 7",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(out_of_range, 0);
    }

    #[test]
    fn a_failed_insert_leaves_the_previous_term_intact() {
        let mut conn = Connection::open_in_memory().unwrap();
        crate::db::migrations::migrate(&mut conn).unwrap();
        let now = crate::time::FixedNow(1_700_000_000_000);
        import(&conn, &now, "pc", CSV).unwrap();
        let count = |conn: &Connection| -> i64 {
            conn.query_row("SELECT count(*) FROM classes", [], |r| r.get(0)).unwrap()
        };
        assert_eq!(count(&conn), 5);

        // 「删与插是不是原子」没有别的证法：让 DELETE 之后的第一条 INSERT 必然失败。
        // 没有事务时这里会变成 0 条课 —— 界面报一句错，一学期的表却空了。
        conn.execute_batch(
            "CREATE TRIGGER boom BEFORE INSERT ON classes BEGIN SELECT RAISE(ABORT, 'injected'); END;",
        )
        .unwrap();
        assert!(import(&conn, &now, "pc", CSV).is_err(), "注入的失败该被报出来");
        assert_eq!(count(&conn), 5, "半个导入不该把一学期的课删空");
    }

    #[test]
    fn a_week_is_seven_days_counted_from_the_first_monday() {
        // 2026-09-07 是周一
        assert_eq!(week_of(20260907, 20260907, 19), Some(1));
        assert_eq!(week_of(20260907, 20260913, 19), Some(1), "周日还在第 1 周");
        assert_eq!(week_of(20260907, 20260914, 19), Some(2));
        assert_eq!(week_of(20260907, 20260830, 19), None, "还没开学别说第 0 周");
        assert_eq!(week_of(20260907, 20270201, 19), None, "学期结束了也不该继续编号");
    }

    /// 2026-10-30 上午十点（本地）。周次算的是本地日，所以从日整数反推毫秒，
    /// 不用裸常量 —— `FixedNow(1_700_000_000_000)` 其实是 2023-11-14，写测试时很容易记错。
    fn at_oct_30() -> crate::time::FixedNow {
        crate::time::FixedNow(crate::time::days::day_start_ms(20261030) + 10 * 3_600_000)
    }

    #[test]
    fn no_current_week_until_the_first_monday_is_set() {
        let mut conn = Connection::open_in_memory().unwrap();
        crate::db::migrations::migrate(&mut conn).unwrap();
        let now = at_oct_30();
        import(&conn, &now, "pc", CSV).unwrap();

        let st = state(&conn, &now).unwrap();
        assert_eq!(st.term.as_deref(), Some("2026-2027-1"));
        assert_eq!(st.rows, 5);
        assert_eq!(st.courses, 4, "五段里 人工智能 出现两次，门数按名字去重");
        assert_eq!(st.total_weeks, Some(16), "总周数从周次集合的最大端推");
        assert_eq!(st.current_week, None, "开学日没设就不该有当前周");
        assert!(st.week1_monday.is_none());

        // 设了开学日才算得出这一周：10/26 周一开学 → 10/30 是第 1 周
        let st2 = set_week1(&conn, &now, "pc", "2026-10-26").unwrap();
        assert_eq!(st2.current_week, Some(1));
        assert_eq!(st2.week1_monday.as_deref(), Some("2026-10-26"), "回填 input[type=date] 要的就是它");

        let (week, rows) = week_rows(&conn, &now, None).unwrap();
        assert_eq!(week, Some(1));
        // 3 而不是 5：体系结构是 2,4,6 周、人工智能的第二段是 9-16 周，第 1 周都不该出现
        assert_eq!(rows.len(), 3);

        // 清空回到"没设"，而不是留一个假日期在那儿算周次
        let st3 = set_week1(&conn, &now, "pc", "").unwrap();
        assert_eq!(st3.current_week, None);
        assert_eq!(st3.rows, 5, "清开学日不该动课");
    }

    #[test]
    fn a_week_between_ranges_has_no_classes() {
        let mut conn = Connection::open_in_memory().unwrap();
        crate::db::migrations::migrate(&mut conn).unwrap();
        let now = at_oct_30();
        import(&conn, &now, "pc", CSV).unwrap();
        // 操作系统 1-10,12-16 / 体育 1,3,5,7,9：第 11 周它俩都没有
        let (week, rows) = week_rows(&conn, &now, Some(11)).unwrap();
        assert_eq!(week, Some(11));
        let names: Vec<&str> = rows.iter().map(|r| r.name.as_str()).collect();
        assert!(!names.contains(&"操作系统"), "空出来的那一周不该出现");
        assert!(!names.contains(&"大学体育5"), "1,3,5,7,9 不含 11");
        assert!(names.contains(&"人工智能"), "9-16 含 11，这门该在");
        let (_, rows16) = week_rows(&conn, &now, Some(16)).unwrap();
        assert!(rows16.iter().any(|r| r.name == "操作系统"));
    }

    #[test]
    fn a_junk_school_start_date_is_refused_not_stored() {
        assert_eq!(iso_to_day("2026-09-07").unwrap(), 20260907);
        for bad in ["2026-9-7", "20260907", "2026-13-01", "开学", ""] {
            assert!(iso_to_day(bad).is_err(), "{bad} 不该被接受");
        }
        // 格式对、日历里没有：这类值以前能进来，然后把周次卡死在第 1 周
        for ghost in ["2026-02-31", "2026-04-31", "2026-02-30"] {
            assert!(iso_to_day(ghost).is_err(), "{ghost} 不存在，不该被接受");
        }
        // 闰年那一天是真的存在，别误杀
        assert_eq!(iso_to_day("2028-02-29").unwrap(), 20280229);
    }
}
