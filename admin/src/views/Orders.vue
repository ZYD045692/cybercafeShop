<template>
  <section>
    <h2>订单管理 <span class="hint">未处理置顶，点「已处理」后完成</span></h2>
    <el-table :data="orders" :row-class-name="rowCls" size="large" empty-text="暂无订单">
      <el-table-column label="状态" width="100">
        <template #default="{ row }">
          <el-tag :type="row.status ? 'success' : 'danger'">{{ row.status ? '已处理' : '未处理' }}</el-tag>
        </template>
      </el-table-column>
      <el-table-column label="机器" width="110">
        <template #default="{ row }"><b>{{ row.machine }}</b></template>
      </el-table-column>
      <el-table-column label="商品明细" min-width="260">
        <template #default="{ row }">{{ row.items.map(i => `${i.name}×${i.qty}`).join('、') }}</template>
      </el-table-column>
      <el-table-column label="支付方式" width="100">
        <template #default="{ row }">{{ payName(row.pay_method) }}</template>
      </el-table-column>
      <el-table-column label="金额" width="100">
        <template #default="{ row }">¥{{ row.total.toFixed(1) }}</template>
      </el-table-column>
      <el-table-column label="时间" width="110">
        <template #default="{ row }">{{ row.created_at.slice(11) || row.created_at }}</template>
      </el-table-column>
      <el-table-column label="操作" width="110">
        <template #default="{ row }">
          <el-button v-if="!row.status" type="primary" @click="done(row.id)">已处理</el-button>
          <span v-else style="color:#bbb">—</span>
        </template>
      </el-table-column>
    </el-table>
  </section>
</template>

<script setup>
import { ref, onMounted, onBeforeUnmount } from 'vue'
import { api, post } from '../api'

const orders = ref([])
const emit = defineEmits(['pending'])

const rowCls = ({ row }) => row.status ? 'row-done' : ''

// 加载失败只弹一次提示（有 60 秒轮询兜底，不刷屏）；恢复后重置标志
let errShown = false
async function reload() {
  try {
    const d = await api('/api/orders')
    orders.value = d.orders
    emit('pending', d.orders.filter(o => !o.status).length)
    errShown = false
  } catch (e) {
    if (!errShown) {
      errShown = true
      ElMessage.error('订单加载失败：' + e.message + '（每分钟自动重试）')
    }
  }
}
async function done(id) {
  try {
    await post(`/api/order/${id}/status`, { status: 1 })
    reload()
  } catch (e) {
    ElMessage.error('操作失败：' + e.message)
  }
}
const payName = m => ({ wechat: '微信', alipay: '支付宝', cash: '现金' }[m] || m)

// 60 秒定时轮询兜底：防止事件广播偶发丢失导致订单页漏刷新
let pollTimer = null
onMounted(() => {
  reload()
  pollTimer = setInterval(reload, 60000)
})
onBeforeUnmount(() => { if (pollTimer) clearInterval(pollTimer) })
defineExpose({ reload })
</script>

<style scoped>
h2 { font-size: 17px; margin-bottom: 14px; }
.hint { font-size: 13px; color: #999; font-weight: normal; }
:deep(.row-done) { color: #9ca3af; }
</style>
