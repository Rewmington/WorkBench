import { api } from '../api'
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
 * 动作注册表。规则：**任何能力都必须在这里有一个 id**，按钮只是它的别名 ——
 * 这样命令面板天然覆盖全部功能，而不是只覆盖「想起来的那些」。
 */
export function useActions(): Action[] {
  const shell = useShell()

  const navigate: Action[] = NAV.flatMap((group) =>
    group.items.map((item) => ({
      id: `nav.${item.key}`,
      label: `去 ${item.label}`,
      hint: group.title ? `${group.title} · ${item.view}` : item.view,
      group: '跳转',
      run: () => shell.go(item.view, item.focus),
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
  ]

  return [...create, ...navigate, ...system]
}
