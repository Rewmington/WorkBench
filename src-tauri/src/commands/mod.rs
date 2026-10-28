//! 命令层共用的小工具。
//!
//! 约定（见 `db/mod.rs`）：repo 只收 `&Connection`，事务由这里持有。
//! 需要多语句原子性的命令走 `in_tx`，单条 UPDATE 的直接用 `&Connection`。

pub mod launcher;
pub mod palette;
pub mod settings;
pub mod system;
pub mod todos;

use rusqlite::Transaction;

use crate::error::Error;
use crate::state::AppState;

pub fn in_tx<T>(
    state: &AppState,
    f: impl FnOnce(&Transaction) -> Result<T, Error>,
) -> Result<T, Error> {
    let mut guard = state
        .db
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let tx = guard.transaction()?;
    let out = f(&tx)?;
    tx.commit()?;
    Ok(out)
}
