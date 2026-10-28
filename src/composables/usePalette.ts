import { ref } from 'vue'

const open = ref(false)

export function usePalette() {
  return {
    open,
    show() {
      open.value = true
    },
    hide() {
      open.value = false
    },
    toggle() {
      open.value = !open.value
    },
  }
}
