//! SQLite 接入。
//!
//! 纪律：碰 DB 的命令一律写同步 `fn`，绝不在 `.await` 两侧持锁；
//! 网络类命令（linked:* / backup:*）先 `spawn_blocking` 完成 IO，再在自己的作用域里取锁。
//! repo 只收 `&Connection`，不认识 `Mutex` —— 这样 `cargo test` 可以直接拿内存库跑真实 SQL。

pub mod migrations;
pub mod setup;

pub use setup::{journal_mode, open, open_in_memory};
