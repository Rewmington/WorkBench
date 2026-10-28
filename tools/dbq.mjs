// 只读查询本机 Anvil 库，用来自证聚合结果（M7 的「逐图核对 SQL 与列表一致」靠它）。
// 只跑聚合：这个脚本的存在不是为了看用户的待办内容，别用它 SELECT * 。
//   node tools/dbq.mjs "SELECT count(*) c FROM todos"
import { DatabaseSync } from 'node:sqlite'
import process from 'node:process'

const sql = process.argv[2]
if (!sql) {
  console.error('用法：node tools/dbq.mjs "<SQL>"')
  process.exit(2)
}

const dbPath = process.argv[3]
  ?? `${process.env.LOCALAPPDATA}\\dev.rewmington.anvil\\app.db`

const db = new DatabaseSync(dbPath, { readOnly: true })
try {
  const rows = db.prepare(sql).all()
  console.log(JSON.stringify(rows, null, 1))
} finally {
  db.close()
}
