<template>
  <section>
    <h2>销售记录</h2>
    <div class="bar">
      <el-date-picker v-model="from" type="date" value-format="YYYY-MM-DD" placeholder="开始日期" style="width:150px" />
      <span style="margin:0 6px">至</span>
      <el-date-picker v-model="to" type="date" value-format="YYYY-MM-DD" placeholder="结束日期" style="width:150px" />
      <el-select v-model="pay" placeholder="全部支付方式" clearable style="width:150px;margin-left:12px">
        <el-option label="微信" value="wechat" />
        <el-option label="支付宝" value="alipay" />
        <el-option label="现金" value="cash" />
      </el-select>
      <el-button type="primary" style="margin-left:12px" @click="query">查询</el-button>
      <span class="sum">共 <b>{{ orders.length }}</b> 笔，合计 <b class="money">¥{{ sum.toFixed(1) }}</b></span>
    </div>
    <el-table :data="orders" size="large" empty-text="没有符合条件的记录">
      <el-table-column label="时间" width="150">
        <template #default="{ row }">{{ row.created_at.slice(5, 16) }}</template>
      </el-table-column>
      <el-table-column prop="machine" label="机器" width="110" />
      <el-table-column label="商品明细" min-width="260">
        <template #default="{ row }">{{ row.items.map(i => `${i.name}×${i.qty}`).join('、') }}</template>
      </el-table-column>
      <el-table-column label="支付方式" width="110">
        <template #default="{ row }">{{ payName(row.pay_method) }}</template>
      </el-table-column>
      <el-table-column label="金额" width="110">
        <template #default="{ row }">¥{{ row.total.toFixed(1) }}</template>
      </el-table-column>
    </el-table>
  </section>
</template>

<script setup>
import { ref, onMounted } from 'vue'
import { api } from '../api'

const today = new Date().toLocaleDateString('sv-SE')
const from = ref(today), to = ref(today), pay = ref('')
const orders = ref([]), sum = ref(0)

async function query() {
  const q = new URLSearchParams()
  if (from.value) q.set('from', from.value)
  if (to.value) q.set('to', to.value)
  if (pay.value) q.set('pay', pay.value)
  const d = await api('/api/admin/records?' + q)
  orders.value = d.orders
  sum.value = d.sum
}
const payName = m => ({ wechat: '微信', alipay: '支付宝', cash: '现金' }[m] || m)
onMounted(query)
</script>

<style scoped>
h2 { font-size: 17px; margin-bottom: 14px; }
.bar { display: flex; align-items: center; margin-bottom: 14px; }
.sum { margin-left: auto; font-size: 15px; }
.money { color: #ef4444; font-size: 20px; }
</style>
