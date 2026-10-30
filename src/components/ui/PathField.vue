<script setup lang="ts">
import { ref, watch } from 'vue'

/**
 * 外部源的路径输入。三处共用（博客仓库 / 药箱库 / 药箱的坚果云目录），
 * 因为"这台机器的东西放在哪"这件事在三个地方必须是同一套措辞与同一套反馈：
 * 空串就是恢复默认、后端拒绝时把原话显示出来而不是自造一句。
 */
const props = defineProps<{
  label: string
  value: string
  /** 落盘并返回规范化后的新值；抛错则原样显示那句话 */
  save: (raw: string) => Promise<string>
  /** 默认值。相等时不显示「恢复默认」，否则那个按钮点了没反应 */
  fallback: string
}>()

const draft = ref(props.value)
const busy = ref(false)
const error = ref<string | null>(null)
const done = ref<string | null>(null)

// 外部改完（比如保存后后端返回了展开的绝对路径）要把输入框拉回来
watch(
  () => props.value,
  (v) => {
    draft.value = v
  },
)

async function commit(raw: string) {
  if (busy.value) return
  busy.value = true
  error.value = null
  done.value = null
  try {
    const next = await props.save(raw)
    draft.value = next
    done.value = raw.trim() ? `已改用 ${next}` : '已恢复默认'
  } catch (e) {
    // 后端已经把该拒的拒了（比如地址指向了工作台的备份目录），这里只转述
    error.value = String(e)
  } finally {
    busy.value = false
  }
}
</script>

<template>
  <div class="pf">
    <span class="lab">{{ label }}</span>
    <div class="pf-row">
      <input v-model="draft" spellcheck="false" autocomplete="off" :disabled="busy" />
      <button type="button" class="y-ghost btn" :disabled="busy" @click="commit(draft)">保存</button>
      <button
        v-if="draft.trim() !== fallback"
        type="button"
        class="y-ghost btn"
        :disabled="busy"
        @click="commit('')"
      >
        恢复默认
      </button>
    </div>
    <span v-if="done" class="pf-ok">{{ done }}</span>
    <span v-if="error" class="pf-bad">{{ error }}</span>
  </div>
</template>

<style scoped>
.pf {
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.pf > .lab {
  font-size: 10.5px;
  color: var(--t3);
}
.pf-row {
  display: flex;
  gap: 8px;
  align-items: center;
}
.pf-row input {
  height: 30px;
  min-width: 0;
  flex: 1;
  padding: 0 10px;
  border-radius: var(--r-sm);
  border: 1px solid var(--line-2);
  background: var(--card);
  color: var(--t1);
  font-size: 12px;
  user-select: text;
}
.pf-row input:focus {
  outline: none;
  border-color: color-mix(in srgb, var(--accent) 45%, transparent);
}
.pf-row .btn {
  flex: none;
  height: 30px;
  width: auto;
  padding: 0 11px;
  font-size: 11.5px;
}
.pf-ok,
.pf-bad {
  font-size: 11px;
  line-height: 1.5;
  word-break: break-all;
}
.pf-ok {
  color: var(--accent);
}
.pf-bad {
  color: var(--red);
}
</style>
