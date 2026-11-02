import { api } from '../api'
import { useBackup } from '../composables/useBackup'
import { NAV } from '../composables/useShell'
import { useShell } from '../composables/useShell'
import { toggleMode } from '../composables/useTheme'

export interface Action {
  /** 稳定 id：按钮、原生菜单、快捷键都指向它（§2 原则 1） */
  id: string
  label: string
  hint?: string
  group: string
  run: () => void | Promise<void>
}

/**
 * 动作注册表。规则：**能一键完成、不需要看住后果的能力都要在这里有 id**，
 * 按钮只是它的别名 —— 这样命令面板覆盖的是「全部这类功能」，而不是想起来的那些。
 *
 * 边界写在近处（2026-11-02 复核命令面板时确认过的事实，别再当成漏项）：
 * - 破坏性动作**故意不在这里**。删磁贴 / 删分区 / 彻底删除笔记 / 清理对端信封都要
 *   两步内联确认，而且确认的是「屏幕上那一个东西」；放进一个靠键盘选中的列表里，
 *   确认就退化成"再按一次回车"，比没有确认更糟。
 * - 原生托盘/右键菜单那几项（退出、重载、切显示、打开设置、复制选中）也不在这里：
 *   它们由 `ui/menus.rs` 走，命令面板这条路不是它们的入口。
 *   所以下面这份清单不等于「全部功能」，别拿它当覆盖率分母。
 */
export function useActions(): Action[] {
  const shell = useShell()
  const backup = useBackup()

  const navigate: Action[] = NAV.flatMap((group) =>
    group.items.map((item) => ({
      id: `nav.${item.key}`,
      label: `去 ${item.label}`,
      hint: group.title ? `${group.title} · ${item.view}` : item.view,
      group: '跳转',
      run: () => shell.go(item.view),
    })),
  )

  const create: Action[] = [
    {
      id: 'todo.new',
      label: '新建待办',
      hint: '直接在输入框里打字',
      group: '新建',
      run: async () => {
        shell.go('todos')
        shell.requestComposer('todo')
      },
    },
    {
      id: 'resource.new',
      label: '新建速达条目',
      hint: '也可以把 exe 直接拖进窗口',
      group: '新建',
      run: async () => {
        shell.go('launcher')
        shell.requestComposer('resource')
      },
    },
  ]

  const system: Action[] = [
    { id: 'theme.toggle', label: '切换深色 / 浅色', group: '外观', run: () => toggleMode() },
    { id: 'data.openDir', label: '打开数据文件夹', hint: '库文件与图标缓存', group: '系统', run: () => api.openDataDir() },
    {
      id: 'backup.now',
      label: '立即备份到坚果云',
      hint: '加密后 PUT 到 /dav/workbench-backup/',
      group: '系统',
      // 先跳过去再跑：结果与流水都渲染在备份页，站在别的视图点它会得到「什么都没发生」
      run: async () => {
        shell.go('backup')
        await backup.runNow()
      },
    },
    {
      id: 'backup.test',
      label: '测试坚果云连接',
      hint: '只列目录，不写任何东西',
      group: '系统',
      run: async () => {
        shell.go('backup')
        await backup.testConnection()
      },
    },
  ]

  return [...create, ...navigate, ...system]
}
