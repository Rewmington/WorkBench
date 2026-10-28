use std::path::Path;

use serde_json::{Map, Value};

use super::{empty_payload, normalized, sig, Snapshot, Status};
use crate::time::Now;

pub const SOURCE: &str = "blog";

/// 固定清单。**不读 `out/content/manifest.json`** —— 实测它列了 `notes`，
/// 而 `src/content/` 里根本没有这个文件（正文在 commit 8a033bd 被清掉了），
/// 拿它当权威会天天误报缺失。
const KINDS: &[(&str, &str)] = &[
    ("projects", "projects.json"),
    ("repos", "github-repos.json"),
    ("photos", "photos.json"),
    ("friends", "friends.json"),
    ("profile", "profile.json"),
    ("music", "music.json"),
    ("notes", "notes.json"),
];

pub fn kinds() -> &'static [(&'static str, &'static str)] {
    KINDS
}

/// 把每类形状不同的 JSON 归一成 `{items, count, meta}`。
pub fn normalize_kind(kind: &str, raw: &Value) -> String {
    let mut meta = Map::new();
    let items = match kind {
        // github-repos.json 是对象：{fetchedAt, repos:[…]}
        "repos" => {
            if let Some(fetched) = raw.get("fetchedAt") {
                meta.insert("remoteFetchedAt".into(), fetched.clone());
            }
            raw.get("repos").cloned().unwrap_or_else(|| Value::Array(Vec::new()))
        }
        // profile.json 是单个对象，包成一条，消费方就不用再判形状
        "profile" => {
            if raw.is_object() {
                Value::Array(vec![raw.clone()])
            } else {
                raw.clone()
            }
        }
        // 友链清单里那条「添加友链」占位项没有 url，不该显示成一个空卡片
        "friends" => Value::Array(
            raw.as_array()
                .map(|list| {
                    list.iter()
                        .filter(|f| f.get("url").and_then(Value::as_str).is_some_and(|u| !u.trim().is_empty()))
                        .cloned()
                        .collect()
                })
                .unwrap_or_default(),
        ),
        "projects" | "photos" | "music" | "notes" => {
            Value::Array(raw.as_array().cloned().unwrap_or_default())
        }
        _ => raw.clone(),
    };
    normalized(items, meta)
}

/// 读本地仓库。目录不存在 / 单个文件缺失都只产生一条 `missing` 快照，
/// 保留上一次的 payload 由调用方（命令层）决定 —— 这里不碰数据库。
pub fn read_repo(repo_dir: &Path, now: &dyn Now) -> Vec<Snapshot> {
    let ms = now.ms();
    let dir_exists = repo_dir.is_dir();

    KINDS
        .iter()
        .map(|(kind, file)| {
            let path = repo_dir.join("src").join("content").join(file);
            let (status, detail, payload) = read_one(&path, dir_exists, kind, file);
            Snapshot {
                source: SOURCE.to_owned(),
                kind: (*kind).to_owned(),
                fetched_at: ms,
                sig: sig::content_sig(&payload),
                status,
                detail,
                payload_json: payload,
            }
        })
        .collect()
}

/// 读一个清单文件：读不到 / 解析不了都只产出一个状态，不 panic、不冒泡到界面。
fn read_one(path: &Path, dir_exists: bool, kind: &str, file: &str) -> (Status, String, String) {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) => {
            let detail = if !dir_exists {
                format!("本地仓库目录不可用（{file}）")
            } else if e.kind() == std::io::ErrorKind::NotFound {
                format!("{kind} 清单不在仓库里")
            } else {
                format!("读 {file} 失败：{e}")
            };
            return (Status::Missing, detail, empty_payload());
        }
    };

    match serde_json::from_str::<Value>(&text) {
        Ok(value) => (Status::Ok, String::new(), normalize_kind(kind, &value)),
        Err(e) => (Status::Error, format!("{file} 不是合法 JSON：{e}"), empty_payload()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time::FixedNow;

    fn fixture() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let content = dir.path().join("src").join("content");
        std::fs::create_dir_all(&content).unwrap();
        std::fs::write(
            content.join("projects.json"),
            r#"[{"name":"Rewmington Desk","description":"个人工作台","tags":["Next.js"],"url":"https://example.com","status":"live"}]"#,
        )
        .unwrap();
        std::fs::write(
            content.join("github-repos.json"),
            r#"{"fetchedAt":"2026-10-25T00:14:00Z","repos":[{"name":"a","stars":3},{"name":"b","stars":1}]}"#,
        )
        .unwrap();
        std::fs::write(
            content.join("friends.json"),
            r#"[{"name":"真友链","url":"https://x.dev"},{"name":"添加友链","url":""}]"#,
        )
        .unwrap();
        std::fs::write(content.join("profile.json"), r#"{"authorName":"Rewmington","techStack":["Vue"]}"#).unwrap();
        std::fs::write(content.join("music.json"), "[]\n").unwrap();
        std::fs::write(content.join("photos.json"), "[{bad json").unwrap();
        // notes.json 故意不写：真实仓库里就是没有
        dir
    }

    fn find<'a>(list: &'a [Snapshot], kind: &str) -> &'a Snapshot {
        list.iter().find(|s| s.kind == kind).expect("kind in table")
    }

    #[test]
    fn arrays_objects_and_placeholders_all_come_back_normalized() {
        let dir = fixture();
        let snaps = read_repo(dir.path(), &FixedNow(1_700_000_000_000));

        let projects = find(&snaps, "projects");
        assert_eq!(projects.status, Status::Ok);
        let v: Value = serde_json::from_str(&projects.payload_json).unwrap();
        assert_eq!(v["count"], 1);
        assert_eq!(v["items"][0]["name"], "Rewmington Desk");

        let repos = find(&snaps, "repos");
        let v: Value = serde_json::from_str(&repos.payload_json).unwrap();
        assert_eq!(v["count"], 2, "对象里的 repos 数组要被抬到 items");
        assert_eq!(v["meta"]["remoteFetchedAt"], "2026-10-25T00:14:00Z");

        let friends = find(&snaps, "friends");
        let v: Value = serde_json::from_str(&friends.payload_json).unwrap();
        assert_eq!(v["count"], 1, "没有 url 的占位项要被滤掉");

        let profile = find(&snaps, "profile");
        let v: Value = serde_json::from_str(&profile.payload_json).unwrap();
        assert!(v["items"].is_array(), "单个对象也要包成数组，消费方只判一种形状");
    }

    #[test]
    fn missing_notes_and_broken_json_degrade_without_panicking() {
        let dir = fixture();
        let snaps = read_repo(dir.path(), &FixedNow(1));

        let notes = find(&snaps, "notes");
        assert_eq!(notes.status, Status::Missing);
        assert!(notes.detail.contains("notes"), "{}", notes.detail);

        let photos = find(&snaps, "photos");
        assert_eq!(photos.status, Status::Error);
        assert!(photos.detail.contains("不是合法 JSON"));
    }

    #[test]
    fn renamed_repo_dir_reports_missing_not_error() {
        let dir = fixture();
        let gone = dir.path().join("仓库已改名");
        let snaps = read_repo(&gone, &FixedNow(1));
        assert!(snaps.iter().all(|s| s.status == Status::Missing));
        assert!(snaps
            .iter()
            .all(|s| s.detail.contains("本地仓库目录不可用")));
    }

    #[test]
    fn signature_is_stable_across_two_reads_of_the_same_content() {
        let dir = fixture();
        let a = read_repo(dir.path(), &FixedNow(1));
        let b = read_repo(dir.path(), &FixedNow(2));
        assert_eq!(find(&a, "projects").sig, find(&b, "projects").sig);
        assert_ne!(find(&a, "projects").sig, find(&a, "repos").sig);
    }
}
