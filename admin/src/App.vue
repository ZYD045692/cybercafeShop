<template>
  <div class="shell">
    <el-menu :default-active="cur" mode="horizontal" :ellipsis="false" @select="k => cur = k" class="topnav">
      <el-menu-item index="orders">
        <el-badge :value="pending" :hidden="!pending" :max="99">
          <el-icon><List /></el-icon><span>订单管理</span>
        </el-badge>
      </el-menu-item>
      <el-menu-item index="products"><el-icon><Goods /></el-icon><span>商品管理</span></el-menu-item>
      <el-menu-item index="records"><el-icon><DataAnalysis /></el-icon><span>销售记录</span></el-menu-item>
      <el-menu-item index="settings"><el-icon><Setting /></el-icon><span>设置</span></el-menu-item>
    </el-menu>
    <main>
      <Orders v-if="cur === 'orders'" ref="ordersRef" @pending="pending = $event" />
      <Products v-else-if="cur === 'products'" />
      <Records v-else-if="cur === 'records'" />
      <Settings v-else />
    </main>
  </div>
</template>

<script setup>
import { ref, onMounted } from 'vue'
import { listen } from '@tauri-apps/api/event'
import { List, Goods, DataAnalysis, Setting } from '@element-plus/icons-vue'
import Orders from './views/Orders.vue'
import Products from './views/Products.vue'
import Records from './views/Records.vue'
import Settings from './views/Settings.vue'

const cur = ref('orders')
const pending = ref(0)
const ordersRef = ref(null)

onMounted(async () => {
  // 新订单/呼叫 → 刷新订单列表和角标
  await listen('tf-event', () => ordersRef.value?.reload())
})
</script>

<style>
* { margin: 0; padding: 0; box-sizing: border-box; }
body { background: #f0f2f5; font-family: "Microsoft YaHei", sans-serif; }
/* 隐藏所有滚动条但保留滚动（原生 + Element Plus 自定义滚动条） */
* { scrollbar-width: none; -ms-overflow-style: none; }
*::-webkit-scrollbar { width: 0; height: 0; display: none; -webkit-appearance: none; }
.el-scrollbar__bar { display: none; }
.shell { min-height: 100vh; background: #fff; }
.topnav { padding: 0; }
.topnav .el-menu-item { font-size: 16px; display: flex; align-items: center; gap: 6px; }
/* 待处理角标：默认 top:0 + translateY(-50%) 会把数字顶出菜单上边界，
   改成往下挪、完整落在菜单条内 */
.topnav .el-badge__content {
  top: 10px;
  transform: translateX(100%);
}
main { padding: 18px 22px 24px; }
</style>
