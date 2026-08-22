<template>
  <!-- 卡片层叠：被压住的卡从窗口顶部依次排（只露 32px 标题栏），最前面的卡钉在窗口底部 -->
  <div class="deck" v-show="cards.length">
    <div v-for="(c, i) in cards" :key="c.key" class="card"
      :style="i === cards.length - 1
        ? { bottom: '0px', zIndex: i + 1, height: cardHeight(c) + 'px' }
        : { top: i * OFFSET + 'px', zIndex: i + 1, height: cardHeight(c) + 'px' }"
      :class="{ behind: i < cards.length - 1, front: i === cards.length - 1 }"
      @click="bringFront(i)">
      <div class="hd" :class="c.type === 'call' ? 'orange' : 'blue'">
        <span>{{ c.type === 'call' ? '📢 ' + c.machine + ' 呼叫网管' : '🛒 ' + c.machine + ' 下单' }}</span>
        <span>{{ c.time }}</span>
      </div>
      <template v-if="i === cards.length - 1">
        <div v-if="c.type === 'call'" class="callbody">
          <b>{{ c.machine }}</b>
          <span>呼叫网管，请前往该机位</span>
        </div>
        <div v-if="c.items" class="items">
          <div v-for="(it, j) in c.items" :key="j">
            <span>{{ it.name }} ×{{ it.qty }}</span><span>¥{{ (it.price * it.qty).toFixed(1) }}</span>
          </div>
        </div>
        <div v-if="c.type === 'order'" class="ft1">
          <span>{{ payName(c.pay) }}</span><span>合计 <b>¥{{ c.total.toFixed(1) }}</b></span>
        </div>
        <div class="ft2">
          <button class="btn green" @click.stop="ship(c)">{{ c.type === 'call' ? '确认' : '已处理' }}</button>
          <button v-if="c.type === 'order'" class="btn gray" @click.stop="dismiss(c)">稍后</button>
        </div>
      </template>
    </div>
    <div v-if="cards.length > 1" class="count">{{ cards.length }} 条待处理</div>
  </div>
</template>

<script setup>
import { ref, watch, onMounted } from 'vue'
import { listen } from '@tauri-apps/api/event'
import { invoke } from '@tauri-apps/api/core'
import { api, post } from './api'

const OFFSET = 32 // 被压住的卡露出的高度 = 标题栏高度（32px），严丝合缝不露白边
// 卡片高度用常量推导（与下方 CSS 写死一致），不量 DOM——量 DOM 有竞态，
// 内容未渲染完会量出 0，窗口就被调成一条线，看起来「卡片没弹出来」
const HD = 32        // 标题栏
const ITEM_ROW = 26  // 每个商品行
const ITEMS_PAD = 20 // 商品区上下 padding
const MAX_ROWS = 6   // 商品区最多显示 6 行，超出滚动
const FT1 = 40       // 支付/合计行（仅订单卡）
const FT2 = 56       // 按钮行
const CALL_BODY = 64 // 呼叫卡的大机台号区域
function cardHeight(c) {
  if (c.type === 'call') return HD + CALL_BODY + FT2
  let h = HD + FT2
  if (c.items && c.items.length) h += Math.min(c.items.length, MAX_ROWS) * ITEM_ROW + ITEMS_PAD
  if (c.type === 'order') h += FT1
  return h
}

const cards = ref([])
const dismissed = ref(new Set())
const payName = m => ({ wechat: '微信', alipay: '支付宝', cash: '现金' }[m] || m)

// 内容变化 → 按常量算出窗口高度通知壳调整：窗口多大卡片就多大，不留透明边
function syncWindow() {
  const n = cards.value.length
  if (!n) { invoke('notify_sync', { height: 0 }); return }
  const h = cardHeight(cards.value[n - 1]) + (n - 1) * OFFSET
  invoke('notify_sync', { height: Math.min(h, window.screen.availHeight - 80) })
}
watch(() => cards.value.map(c => c.key).join(','), syncWindow)

async function loadOrders() {
  const d = await api('/api/orders')
  const orderCards = d.orders
    .filter(o => !o.status && !dismissed.value.has(o.id))
    .map(o => ({
      key: 'o' + o.id, type: 'order', id: o.id, machine: o.machine,
      pay: o.pay_method, total: o.total, items: o.items,
      time: (o.created_at.slice(11) || o.created_at),
    }))
  // 保留呼叫卡在前，订单卡按时间排
  cards.value = [...cards.value.filter(c => c.type === 'call'), ...orderCards]
}

// 点被压住的卡 → 翻到最前
function bringFront(i) {
  if (i === cards.value.length - 1) return
  const [c] = cards.value.splice(i, 1)
  cards.value.push(c)
}

async function ship(c) {
  if (c.type === 'order') {
    await post(`/api/order/${c.id}/status`, { status: 1 })
  }
  cards.value = cards.value.filter(x => x.key !== c.key)
}

function dismiss(c) {
  dismissed.value.add(c.id)
  cards.value = cards.value.filter(x => x.key !== c.key)
}

onMounted(async () => {
  // 复制限制：屏蔽右键菜单 + 复制/剪切/全选 快捷键与事件（与用户端一致）
  document.addEventListener('contextmenu', e => e.preventDefault())
  document.addEventListener('copy', e => e.preventDefault())
  document.addEventListener('cut', e => e.preventDefault())
  document.addEventListener('selectstart', e => e.preventDefault())
  await loadOrders().catch(() => {})
  try {
    await listen('tf-event', ev => {
      if (ev.payload.type === 'call') {
        // 同一台机器的呼叫不叠卡：已有未处理的呼叫卡就只刷新时间并置顶
        const exist = cards.value.findIndex(c => c.type === 'call' && c.machine === ev.payload.machine)
        if (exist >= 0) {
          const [c] = cards.value.splice(exist, 1)
          c.time = new Date().toTimeString().slice(0, 8)
          cards.value.push(c)
        } else {
          cards.value.push({ key: 'c' + Date.now(), type: 'call', machine: ev.payload.machine, time: new Date().toTimeString().slice(0, 8) })
        }
      } else {
        loadOrders()
      }
    })
  } catch (e) {
    console.error('事件监听失败', e)
  }
})
</script>

<style>
* { margin: 0; padding: 0; box-sizing: border-box; font-family: "Microsoft YaHei", sans-serif; }
html, body { background: transparent; overflow: hidden; }
/* 禁止选中文字（与用户端一致） */
body { user-select: none; -webkit-user-select: none; -moz-user-select: none; }
/* 以下高度与 script 顶部常量一一对应，改一边必须改另一边 */
/* 整卡深色护眼系（网管可能通宵值守，低刺激）：深底 + 白字/浅灰字 */
.deck { position: fixed; inset: 0; background: #0c1220; overflow: hidden; }
/* 卡片贴满窗口，深色底；标题栏比卡片略亮一档区分层次 */
.card { position: absolute; left: 0; right: 0; background: #0c1220;
  overflow: hidden; display: flex; flex-direction: column; }
.card.behind { cursor: pointer; }
.card.behind .hd { filter: brightness(.85); }
/* 整卡统一深色：标题栏与主体同色，用细分隔线区分层次，文字白/浅灰 */
.hd { padding: 0 14px; font-size: 15px; font-weight: bold; color: #fff; display: flex; justify-content: space-between; height: 32px; line-height: 32px; flex-shrink: 0; border-bottom: 1px solid #1a2540; }
.hd.blue { background: #0c1220 } .hd.orange { background: #92400e }.callbody { height: 64px; flex-shrink: 0; display: flex; flex-direction: column; justify-content: center; padding: 0 16px; gap: 2px; }
.callbody b { font-size: 24px; color: #fbbf24; }
.callbody span { font-size: 13px; color: #94a3b8; }
.items { padding: 10px 14px; flex: 1; min-height: 0; overflow-y: auto; font-size: 15px; color: #e2e8f0; }
.items div { display: flex; justify-content: space-between; height: 26px; line-height: 26px; }
.ft1 { display: flex; justify-content: space-between; align-items: center; padding: 0 14px; font-size: 15px; height: 40px; flex-shrink: 0; color: #e2e8f0; }
.ft1 b { color: #fbbf24; font-size: 19px; }
.ft2 { display: flex; gap: 10px; padding: 6px 14px 12px; height: 56px; flex-shrink: 0; }
.btn { flex: 1; height: 38px; font-size: 16px; border: none; border-radius: 6px; cursor: pointer; color: #fff; }
.btn.green { background: #1e2a47 } .btn.gray { background: #1f2937 }
.count { position: fixed; top: 7px; right: 10px; background: rgba(0,0,0,.55); color: #fff;
  font-size: 13px; padding: 3px 12px; border-radius: 12px; z-index: 9999; }
/* 被压住的卡只露标题栏，时间藏起来，避免和「N 条待处理」角标叠字 */
.card.behind .hd span:last-child { visibility: hidden; }
</style>
