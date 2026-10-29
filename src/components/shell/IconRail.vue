<script setup lang="ts">
import { NAV, useShell } from '../../composables/useShell'

const { view, go } = useShell()
</script>

<template>
  <!-- < 720px 时侧栏收成图标轨（§8.4，退回 x-hub 形态） -->
  <aside class="y-rail">
    <div class="b">A</div>
    <button
      v-for="group in NAV"
      :key="group.title ?? 'top'"
      class="grpwrap"
      :class="{ sep: !!group.title }"
      :title="group.title"
      :aria-label="group.title"
    >
      <span
        v-for="item in group.items"
        :key="item.key"
        class="it"
        :class="{ on: view === item.view }"
        role="button"
        tabindex="0"
        :title="item.label"
        @click="go(item.view)"
        @keydown.enter.prevent="go(item.view)"
      >
        <component :is="item.icon" :size="16" />
      </span>
    </button>
  </aside>
</template>

<style scoped>
.y-rail {
  background: var(--panel);
  border-right: 1px solid var(--line);
  padding: 12px 0;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 6px;
}
.b {
  width: 24px;
  height: 24px;
  border-radius: 7px;
  display: grid;
  place-items: center;
  background: linear-gradient(150deg, var(--accent), #0a7a45);
  color: var(--accent-ink);
  font-size: 12px;
  font-weight: 800;
  margin-bottom: 6px;
}
.grpwrap {
  display: flex;
  flex-direction: column;
  gap: 2px;
}
.grpwrap.sep {
  border-top: 1px solid var(--line);
  padding-top: 6px;
  margin-top: 2px;
  width: 26px;
  align-items: center;
}
.it {
  width: 32px;
  height: 28px;
  border-radius: 8px;
  display: grid;
  place-items: center;
  color: var(--t3);
  cursor: pointer;
}
.it:hover {
  background: var(--card);
  color: var(--t1);
}
.it.on {
  color: var(--accent);
  background: var(--accent-soft);
}
</style>
