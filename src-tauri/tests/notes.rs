//! M6 笔记与文件夹。重点是**三态别混**：活动 / 回收站 / 墓碑。
//! 「回收站可恢复」是 M6 的验收项，所以还原后正文与标签必须一字不差。

use rusqlite::Connection;

use anvil_lib::db::migrations::migrate;
use anvil_lib::model::{NoteDraft, NotePatch};
use anvil_lib::repos::{note_folders, notes, tags};
use anvil_lib::time::{days, FixedNow};

const MS: i64 = 1_793_170_800_000; // 2026-10-28T15:00+08:00
const TODAY: i32 = 20261028;
const DEV: &str = "pc-a";

fn db() -> Connection {
    let mut c = Connection::open_in_memory().expect("in-memory");
    migrate(&mut c).expect("migrate");
    c
}

fn add(c: &Connection, title: &str, folder: Option<String>) -> String {
    notes::create(
        c,
        &FixedNow(MS),
        DEV,
        NoteDraft {
            title: title.into(),
            body_md: format!("{title} 的正文"),
            folder_id: folder,
            tags: vec![],
        },
    )
    .expect("create")
    .stamp
    .id
}

fn folder(c: &Connection, name: &str) -> String {
    note_folders::create(c, &FixedNow(MS), DEV, name, None)
        .expect("folder")
        .stamp
        .id
}

#[test]
fn create_stamps_rev_and_the_local_creation_day() {
    let c = db();
    let id = add(&c, "选型笔记", None);
    let note = notes::get(&c, &id).unwrap().unwrap();
    assert_eq!(note.stamp.rev, 1);
    assert_eq!(note.created_day, Some(i64::from(TODAY)));
    assert!(!note.pinned);
    assert_eq!(note.trashed_at, None);
    assert_eq!(note.body_md, "选型笔记 的正文");
}

#[test]
fn empty_title_is_refused() {
    let c = db();
    let err = notes::create(
        &c,
        &FixedNow(MS),
        DEV,
        NoteDraft { title: "   ".into(), ..Default::default() },
    );
    assert!(err.is_err());
    assert!(notes::list_active(&c, None).unwrap().is_empty());
}

#[test]
fn folder_filter_means_direct_children_not_everything() {
    // 「不过滤」和「只要没有文件夹的」是两回事，早期版本用一个 SQL 技巧把它们混了
    let c = db();
    let f = folder(&c, "工作");
    add(&c, "在里面", Some(f.clone()));
    add(&c, "在外面", None);

    assert_eq!(notes::list_active(&c, None).unwrap().len(), 2, "None = 不过滤");
    let only = notes::list_active(&c, Some(&f)).unwrap();
    assert_eq!(only.len(), 1);
    assert_eq!(only[0].title, "在里面");
}

#[test]
fn trash_then_restore_keeps_body_and_tags_intact() {
    let c = db();
    let id = add(&c, "要进回收站的", None);
    notes::set_tags(&c, &FixedNow(MS), DEV, &id, &["草稿".to_owned(), "重要".to_owned()]).unwrap();

    let trashed = notes::trash(&c, &FixedNow(MS + 1000), DEV, &id).unwrap();
    assert!(trashed.trashed_at.is_some());
    assert!(notes::list_active(&c, None).unwrap().is_empty(), "活动列表里不该还有它");
    assert_eq!(notes::list_trashed(&c).unwrap().len(), 1);

    let back = notes::restore(&c, &FixedNow(MS + 2000), DEV, &id).unwrap();
    assert_eq!(back.trashed_at, None);
    assert_eq!(back.body_md, "要进回收站的 的正文", "回收站不该动正文");
    let mut names = back.tags.clone();
    names.sort();
    assert_eq!(names, vec!["草稿".to_owned(), "重要".to_owned()], "标签要原样回来");

    // 没在回收站里的条目调 restore 不该有副作用
    let again = notes::restore(&c, &FixedNow(MS + 3000), DEV, &id).unwrap();
    assert_eq!(again.stamp.rev, back.stamp.rev, "无变化的还原不该推 rev");
}

#[test]
fn purge_leaves_a_tombstone_rather_than_deleting_the_row() {
    // v2 手机端要能收到这条删除，物理 DELETE 就没有东西可传播（PLAN §5.1）
    let c = db();
    let id = add(&c, "彻底删掉", None);
    notes::trash(&c, &FixedNow(MS), DEV, &id).unwrap();
    notes::purge(&c, &FixedNow(MS + 500), DEV, &id).unwrap();

    assert!(notes::get(&c, &id).unwrap().is_none());
    assert!(notes::list_active(&c, None).unwrap().is_empty());
    assert!(notes::list_trashed(&c).unwrap().is_empty());
    let raw: Option<i64> = c
        .query_row("SELECT deleted_at FROM notes WHERE id = ?1", rusqlite::params![id], |r| r.get(0))
        .unwrap();
    assert_eq!(raw, Some(MS + 500));
}

#[test]
fn update_can_clear_the_folder_but_not_by_omitting_the_field() {
    let c = db();
    let f = folder(&c, "临时");
    let id = add(&c, "挪来挪去", Some(f.clone()));

    // 外层 None = 不动这个字段
    let untouched = notes::update(&c, &FixedNow(MS + 1), DEV, &id, NotePatch::default()).unwrap();
    assert_eq!(untouched.folder_id, Some(f.clone()));

    // 内层 None = 清空它
    let cleared = notes::update(
        &c,
        &FixedNow(MS + 2),
        DEV,
        &id,
        NotePatch { folder_id: Some(None), ..Default::default() },
    )
    .unwrap();
    assert_eq!(cleared.folder_id, None);
}

#[test]
fn deleting_a_folder_moves_its_notes_and_promotes_children_but_deletes_no_content() {
    let c = db();
    let parent = folder(&c, "项目");
    let child = note_folders::create(&c, &FixedNow(MS), DEV, "子目录", Some(parent.clone()))
        .unwrap()
        .stamp
        .id;
    add(&c, "父目录里的笔记", Some(parent.clone()));
    add(&c, "子目录里的笔记", Some(child.clone()));

    let moved = note_folders::delete(&c, &FixedNow(MS + 10), DEV, &parent).unwrap();
    assert_eq!(moved, 1, "只有直属父目录的那篇被挪走");

    assert!(note_folders::list(&c).unwrap().iter().all(|f| f.stamp.id != parent));
    let kids = note_folders::list(&c).unwrap();
    assert_eq!(kids.len(), 1, "子目录要活着");
    assert_eq!(kids[0].parent_id, None, "子目录提到上一层");
    assert_eq!(kids[0].note_count, 1);

    let alive = notes::list_active(&c, None).unwrap();
    assert_eq!(alive.len(), 2, "删目录不该删掉任何一篇笔记");
}

#[test]
fn folder_cannot_be_moved_into_its_own_descendant() {
    let c = db();
    let a = folder(&c, "A");
    let b = note_folders::create(&c, &FixedNow(MS), DEV, "B", Some(a.clone())).unwrap().stamp.id;
    let d = note_folders::create(&c, &FixedNow(MS), DEV, "D", Some(b.clone())).unwrap().stamp.id;

    let err = note_folders::move_to(&c, &FixedNow(MS + 1), DEV, &a, Some(d.clone()));
    assert!(err.is_err(), "A 挪进 A 的孙目录会造出环，树一渲染就死循环");
    let self_err = note_folders::move_to(&c, &FixedNow(MS + 2), DEV, &b, Some(b.clone()));
    assert!(self_err.is_err());
    // 合法移动仍然可用
    assert!(note_folders::move_to(&c, &FixedNow(MS + 3), DEV, &b, None).is_ok());
}

#[test]
fn folder_list_counts_only_live_notes_per_direct_parent() {
    let c = db();
    let f = folder(&c, "有货");
    add(&c, "一", Some(f.clone()));
    add(&c, "二", Some(f.clone()));
    let trashed = add(&c, "三", Some(f.clone()));
    notes::trash(&c, &FixedNow(MS + 5), DEV, &trashed).unwrap();

    let listed = note_folders::list(&c).unwrap();
    assert_eq!(listed[0].note_count, 2, "回收站里的不该计数");
}

#[test]
fn tags_are_deduped_and_shared_between_notes_and_todos() {
    let c = db();
    let id = add(&c, "带标签", None);
    notes::set_tags(
        &c,
        &FixedNow(MS),
        DEV,
        &id,
        &["工作".to_owned(), " 工作 ".to_owned(), "v1".to_owned()],
    )
    .unwrap();
    let note = notes::get(&c, &id).unwrap().unwrap();
    assert_eq!(note.tags.len(), 2, "去空白后同名只算一个");

    // 同名标签在两张表之间共用同一行：每打一次就长一条新行是错的
    let todo_id = anvil_lib::repos::todos::create(
        &c,
        &FixedNow(MS),
        DEV,
        anvil_lib::model::TodoDraft { title: "也打工作标签".into(), ..Default::default() },
    )
    .unwrap()
    .stamp
    .id;
    tags::set_todo_tags(&c, &FixedNow(MS), DEV, &todo_id, &["工作".to_owned()]).unwrap();
    let distinct: i64 = c
        .query_row("SELECT count(DISTINCT name) FROM tags WHERE deleted_at IS NULL", [], |r| r.get(0))
        .unwrap();
    let total: i64 = c
        .query_row("SELECT count(*) FROM tags WHERE deleted_at IS NULL", [], |r| r.get(0))
        .unwrap();
    assert_eq!(distinct, total, "标签名唯一，不该有重复行");
    assert_eq!(tags::tags_for(&c, tags::Owner::Todo, &[todo_id]).unwrap().len(), 1);
}

#[test]
fn ordering_puts_pinned_first_then_newest_edit() {
    let c = db();
    let old = add(&c, "早编辑的", None);
    notes::update(&c, &FixedNow(MS + 10), DEV, &old, NotePatch::default()).unwrap();
    let fresh = add(&c, "刚编辑的", None);
    notes::update(&c, &FixedNow(MS + 20), DEV, &fresh, NotePatch::default()).unwrap();
    let pinned = add(&c, "置顶但最旧", None);
    notes::update(
        &c,
        &FixedNow(MS + 1),
        DEV,
        &pinned,
        NotePatch { pinned: Some(true), ..Default::default() },
    )
    .unwrap();

    let titles: Vec<String> = notes::list_active(&c, None).unwrap().into_iter().map(|n| n.title).collect();
    assert_eq!(titles, vec!["置顶但最旧", "刚编辑的", "早编辑的"]);
}

#[test]
fn empty_database_yields_empty_lists_not_errors() {
    let c = db();
    assert!(notes::list_active(&c, None).unwrap().is_empty());
    assert!(notes::list_active(&c, Some("nope")).unwrap().is_empty());
    assert!(notes::list_trashed(&c).unwrap().is_empty());
    assert!(note_folders::list(&c).unwrap().is_empty());
    assert!(notes::get(&c, "nope").unwrap().is_none());
    assert!(notes::trash(&c, &FixedNow(days::day_start_ms(TODAY)), DEV, "nope").is_err());
}
