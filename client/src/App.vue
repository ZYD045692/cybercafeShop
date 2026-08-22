<template>
  <div class="shell">
    <header class="bar" @mousedown="onBarMousedown">
      <div class="brand">
        <h1>{{ shopName || '商品点购' }}</h1>
        <span v-if="welcome" class="welcome">{{ welcome }}</span>
      </div>
      <div class="head-right">
        <span class="machine"><b>{{ MACHINE }}</b>号机</span>
        <button class="callbtn" @click="callDlg = true">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"><path d="M3 11v2a1 1 0 0 0 1 1h2l4 5V5L6 10H4a1 1 0 0 0-1 1z"/><path d="M14 8a4 4 0 0 1 0 8"/></svg>
          呼叫网管
        </button>
      </div>
      <div class="winbtns">
        <button class="winbtn" @click="minWin" title="最小化"><svg viewBox="0 0 24 24" stroke="currentColor" stroke-width="1.4" stroke-linecap="round"><line x1="5" y1="12" x2="19" y2="12"/></svg></button>
        <button class="winbtn" @click="maxWin" :title="isMax ? '还原' : '最大化'"><svg v-if="isMax" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round"><rect x="8" y="8" width="11" height="11" rx="1"/><path d="M6 16V6a1 1 0 0 1 1-1h10"/></svg><svg v-else viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round"><rect x="5" y="5" width="14" height="14" rx="1"/></svg></button>
        <button class="winbtn close" @click="closeWin" title="关闭"><svg viewBox="0 0 24 24" stroke="currentColor" stroke-width="1.4" stroke-linecap="round"><line x1="6" y1="6" x2="18" y2="18"/><line x1="18" y1="6" x2="6" y2="18"/></svg></button>
      </div>
    </header>

    <!-- 加载失败 -->
    <div v-if="loadErr" class="loaderr">
      <el-result icon="error" title="连不上吧台主机" sub-title="请直接去吧台购买，或点下方按钮重试">
        <template #extra>
          <button class="checkout reconnect" @click="init">重新连接</button>
        </template>
      </el-result>
    </div>

    <template v-else>
      <div class="toolbar">
        <div class="cats">
          <button class="cat" :class="{ on: !curCat && !kw }" @click="curCat = ''">全部商品</button>
          <button v-for="c in categories.filter(x => x.name !== '全部商品')" :key="c.name" class="cat" :class="{ on: curCat === c.name && !kw }" @click="curCat = c.name">{{ c.name }}</button>
        </div>
        <div class="search">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round"><circle cx="11" cy="11" r="7"/><path d="m20 20-3.5-3.5"/></svg>
          <input v-model="kw" placeholder="输入商品名或者首字母如娃哈哈（whh）">
        </div>
      </div>
      <div class="kbody">
        <div class="kgrid">
          <div v-for="p in shown" :key="p.id" class="pcard" @click="add(p)">
            <div class="picwrap">
              <img v-if="p.pic" :src="imgUrl(p.pic)" loading="lazy">
              <span v-else class="nopic"><b class="abbr">{{ p.name.slice(0, 3) }}</b><span>暂无图片</span></span>
            </div>
            <div class="pinfo">
              <div class="pn">{{ p.name }}</div>
              <div class="meta">
                <span class="pp mono"><small>¥</small>{{ p.price.toFixed(1) }}</span>
                <span class="sold mono">已售 {{ p.sold }}</span>
              </div>
            </div>
          </div>
          <div v-if="kw && !shown.length" class="nohit">没有搜到「{{ kw }}」相关商品</div>
        </div>

        <aside class="cart">
          <div class="cart-head">
            <div class="t">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="9" cy="20" r="1.5"/><circle cx="18" cy="20" r="1.5"/><path d="M2 3h3l2.6 12.5a1 1 0 0 0 1 .5h9.9a1 1 0 0 0 1-.8L21.5 7H6"/></svg>
              购物车
            </div>
            <span v-if="count" class="n mono">{{ count }} 件</span>
          </div>
          <template v-if="cartList.length">
            <div class="cart-cols"><span>商品</span><span>单价</span><span>数量</span><span>小计</span></div>
            <div class="cart-body">
              <div class="item" v-for="it in cartList" :key="it.id">
                <div class="name">{{ it.p.name }}</div>
                <div class="unit mono">{{ it.p.price.toFixed(1) }}</div>
                <div class="qty">
                  <button @click="dec(it.id)">−</button>
                  <b>{{ it.qty }}</b>
                  <button @click="inc(it.id)">＋</button>
                </div>
                <div class="sub mono">{{ (it.p.price * it.qty).toFixed(1) }}</div>
              </div>
            </div>
          </template>
          <div v-else class="cart-empty">点左边商品加入</div>
          <div class="cart-foot">
            <div class="total">
              <span class="label">Total 总金额</span>
              <span class="amt mono">¥{{ total.toFixed(1) }}<small>元</small></span>
            </div>
            <button class="checkout" @click="openConfirm">去结算</button>
            <div class="pay-hint">支持微信 / 支付宝 / 现金</div>
          </div>
        </aside>
      </div>
    </template>

    <!-- 确认清单 -->
    <el-dialog v-model="confirmDlg" title="请确认您的订单" width="440px" align-center>
      <div class="clist">
        <div v-for="it in cartList" :key="it.id">
          <span>{{ it.p.name }} ×{{ it.qty }}</span><span class="mono">¥{{ (it.p.price * it.qty).toFixed(1) }}</span>
        </div>
      </div>
      <p class="ctotal">合计 <b class="mono">¥{{ total.toFixed(1) }}</b></p>
      <p class="pay-label">选择支付方式</p>
      <div class="paybtns">
        <button :class="{ on: pay === 'wechat' }" @click="pay = 'wechat'">微信支付</button>
        <button :class="{ on: pay === 'alipay' }" @click="pay = 'alipay'">支付宝</button>
        <button :class="{ on: pay === 'cash' }" @click="pay = 'cash'">现金</button>
      </div>
      <template #footer>
        <button class="checkout" @click="confirm">确认无误{{ pay === 'cash' ? '，现金支付' : '，去付款' }}</button>
      </template>
    </el-dialog>

    <!-- 扫码支付（二维码为页面加载时已缓存） -->
    <el-dialog v-model="qrDlg" width="420px" align-center>
      <template #header>
        请用{{ payName }}扫码支付 <b class="mono" style="color:var(--amber);font-size:24px">¥{{ total.toFixed(1) }}</b>
      </template>
      <div style="text-align:center">
        <img v-if="qr[pay]" :src="qr[pay]" class="qrimg">
        <el-alert v-else type="error" :closable="false" :title="`吧台还没配置${payName}收款码，请返回改用其他支付方式`" />
        <p style="color:var(--muted);font-size:14px;margin-top:14px">请先付款，再提交订单</p>
      </div>
      <template #footer>
        <button class="checkout" :disabled="!qr[pay]" @click="submit">我已付款，提交订单</button>
        <button class="linkbtn" @click="qrDlg = false; confirmDlg = true">← 返回修改付款方式</button>
      </template>
    </el-dialog>

    <!-- 呼叫网管确认 -->
    <el-dialog v-model="callDlg" title="呼叫网管" width="380px" align-center>
      <p style="color:var(--muted);font-size:14px">网管会听到语音播报并过来您的机器</p>
      <template #footer>
        <button class="checkout" :disabled="calling" @click="doCall">确认呼叫</button>
        <button class="linkbtn" @click="callDlg = false">取消</button>
      </template>
    </el-dialog>
  </div>
</template>

<script setup>
import { ref, computed, onMounted } from 'vue'
import { getCurrentWindow } from '@tauri-apps/api/window'
// ElMessageBox 由 unplugin-auto-import 自动导入并注入样式（手动 import 会丢 CSS）
import { loadInit, submitOrder, callNet, imgUrl, MACHINE } from './api'

const categories = ref([]), products = ref([]), qr = ref({})
const shopName = ref(''), welcome = ref('')
const curCat = ref(''), kw = ref(''), cart = ref({}), pay = ref('wechat')
const confirmDlg = ref(false), qrDlg = ref(false), callDlg = ref(false), loadErr = ref(false)
const calling = ref(false) // 呼叫按钮防连点：一次呼叫只发一个请求

// 搜索：纯字母/数字 → 按缩拼匹配（whh → 娃哈哈）；否则按名称包含。搜索时忽略分类。
const shown = computed(() => {
  const q = kw.value.trim().toLowerCase()
  let list = curCat.value ? products.value.filter(p => p.class === curCat.value) : products.value
  if (q) {
    const byAbbr = /^[a-z0-9]+$/.test(q)
    list = products.value.filter(p => byAbbr
      ? (p.abbr || '').toLowerCase().includes(q)
      : p.name.toLowerCase().includes(q))
  }
  return list
})
const cartList = computed(() => Object.entries(cart.value).map(([id, it]) => ({ id: Number(id), ...it })))
const count = computed(() => cartList.value.reduce((s, i) => s + i.qty, 0))
const total = computed(() => cartList.value.reduce((s, i) => s + i.p.price * i.qty, 0))
const payName = computed(() => ({ wechat: '微信', alipay: '支付宝' }[pay.value]))

// 居中提示弹窗（软件内，不用系统通知）
function showMsg(text) {
  ElMessageBox.alert(text, '提示', {
    confirmButtonText: '确认',
    center: true,
    showClose: false, // 去掉右上角关闭叉号，标题才会水平居中
    customStyle: { fontSize: '20px', fontWeight: '700', lineHeight: 1.7 }
  }).catch(() => {})
}

async function init() {
  loadErr.value = false
  try {
    const d = await loadInit()
    shopName.value = d.shopName
    welcome.value = d.welcome
    categories.value = d.categories
    products.value = d.products
    qr.value = d.qr
  } catch {
    loadErr.value = true
  }
}

function add(p) {
  const it = cart.value[p.id]
  if (it) { if (it.qty < 99) it.qty++ } else cart.value[p.id] = { p, qty: 1 }
  cart.value = { ...cart.value }
}
function inc(id) { if (cart.value[id].qty < 99) cart.value[id].qty++ }
function dec(id) { if (--cart.value[id].qty <= 0) delete cart.value[id]; cart.value = { ...cart.value } }

function openConfirm() {
  if (!count.value) { showMsg('购物车是空的\n先点选您要的商品'); return }
  pay.value = 'wechat' // 默认微信
  confirmDlg.value = true
}

function confirm() {
  confirmDlg.value = false
  if (pay.value === 'cash') {
    submit() // 现金：不用扫码，直接提交
  } else {
    qrDlg.value = true
  }
}

async function submit() {
  qrDlg.value = false
  const items = cartList.value.map(it => ({ id: it.id, qty: it.qty }))
  const amount = total.value.toFixed(1)
  try {
    await submitOrder(pay.value, items)
    cart.value = {}
    showMsg(pay.value === 'cash'
      ? `下单成功！\n请准备好现金 ¥${amount}，吧台收款后给您送货`
      : '下单成功！\n吧台已收到您的订单，请稍等送货')
  } catch (e) {
    showMsg('提交失败：' + e.message + '\n请稍后再试，或直接去吧台购买')
  }
}

async function doCall() {
  if (calling.value) return
  calling.value = true
  callDlg.value = false
  try {
    await callNet()
    showMsg('已呼叫网管，请稍等')
  } catch {
    showMsg('呼叫失败，请稍后再试')
  } finally {
    calling.value = false
  }
}

// 无边框窗口：自绘标题栏的拖拽 + 最小化/最大化/还原/关闭
const isMax = ref(false)
const hasTauri = () => !!window.__TAURI_INTERNALS__
function onBarMousedown(e) {
  if (!hasTauri()) return
  if (e.target.closest('button')) return // 按钮不触发拖拽（呼叫/最小化/最大化/关闭）
  getCurrentWindow().startDragging()
}
async function minWin() { if (!hasTauri()) return; await getCurrentWindow().minimize() }
async function maxWin() {
  if (!hasTauri()) return
  const w = getCurrentWindow()
  if (await w.isMaximized()) { await w.unmaximize(); isMax.value = false }
  else { await w.maximize(); isMax.value = true }
}
async function closeWin() { if (!hasTauri()) return; await getCurrentWindow().close() }

onMounted(async () => {
  init()
  // 复制限制：屏蔽右键菜单 + 复制/剪切/全选 快捷键与事件
  document.addEventListener('contextmenu', e => e.preventDefault())
  document.addEventListener('copy', e => e.preventDefault())
  document.addEventListener('cut', e => e.preventDefault())
  document.addEventListener('selectstart', e => e.preventDefault())
  // 无边框窗口：初始最大化状态（决定"最大化/还原"图标）
  if (hasTauri()) { try { isMax.value = await getCurrentWindow().isMaximized() } catch {} }
})
</script>

<style>
:root {
  --bg: #050810;
  --panel: #0c1220;
  --panel2: #111a2b;
  --line: #1a2540;
  --line2: #263453;
  --text: #e7ecf5;
  --muted: #8494ad;
  --dim: #5a6a85;
  --amber: #fbbf24;
  --amber-deep: #b45309;
  --green: #34d399;
}
* { margin: 0; padding: 0; box-sizing: border-box; }
body { background: var(--bg); color: var(--text); font-family: -apple-system, "PingFang SC", "Microsoft YaHei", "Segoe UI", sans-serif; }
/* 隐藏所有滚动条但保留滚动（原生 + Element Plus 自定义滚动条） */
* { scrollbar-width: none; -ms-overflow-style: none; }
*::-webkit-scrollbar { width: 0; height: 0; display: none; -webkit-appearance: none; }
.el-scrollbar__bar { display: none; }
.mono { font-family: ui-monospace, "Cascadia Code", Consolas, "Courier New", monospace; }
/* 复制限制：禁止选中文字（body 上，弹窗/弹层也会继承）；输入框保留可编辑 */
body { user-select: none; -webkit-user-select: none; -moz-user-select: none; }
input, textarea { user-select: text; -webkit-user-select: text; -moz-user-select: text; }

.shell { height: 100vh; display: flex; flex-direction: column; overflow: hidden; }

/* 顶栏 */
.bar { height: 60px; flex: none; display: flex; align-items: center; gap: 16px; padding: 0 20px; background: var(--panel); border-bottom: 1px solid var(--line); }
.brand { display: flex; align-items: baseline; gap: 14px; min-width: 0; }
.brand h1 { font-size: 18px; font-weight: 700; letter-spacing: .02em; white-space: nowrap; }
.brand .welcome { font-size: 13px; color: var(--muted); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.head-right { margin-left: auto; display: flex; align-items: center; gap: 18px; }
.machine { font-size: 12px; color: var(--muted); }
.machine b { color: var(--text); font-family: ui-monospace, "Cascadia Code", Consolas, monospace; font-weight: 600; letter-spacing: .06em; }
.callbtn { display: flex; align-items: center; gap: 8px; height: 36px; padding: 0 16px; background: var(--amber); color: #1a1207; border: none; border-radius: 6px; cursor: pointer; font-size: 14px; font-weight: 700; letter-spacing: .04em; transition: filter .15s ease, transform .1s ease; font-family: inherit; }
.callbtn:hover { filter: brightness(1.1); }
.callbtn:active { transform: translateY(1px); }
.callbtn svg { width: 16px; height: 16px; }
/* 无边框窗口的自绘标题栏按钮（右贴边、全高、hover 高亮，关闭钮 hover 变红） */
.winbtns { display: flex; align-self: stretch; margin-right: -20px; }
.winbtn { width: 46px; height: 100%; display: flex; align-items: center; justify-content: center; background: transparent; color: var(--muted); border: none; cursor: pointer; font-family: inherit; transition: background .12s ease; }
.winbtn svg { width: 16px; height: 16px; }
.winbtn:hover { background: rgba(255,255,255,.08); color: var(--text); }
.winbtn.close:hover { background: #e81123; color: #fff; }

/* 分类 + 搜索 */
.toolbar { flex: none; display: flex; align-items: center; gap: 10px; padding: 12px 20px; background: var(--bg); border-bottom: 1px solid var(--line); }
.cats { display: flex; gap: 8px; }
.cat { height: 34px; padding: 0 16px; display: flex; align-items: center; border: 1px solid var(--line2); border-radius: 6px; background: transparent; color: var(--muted); font-size: 13px; cursor: pointer; transition: all .15s ease; white-space: nowrap; font-family: inherit; }
.cat:hover { color: var(--text); border-color: #3a4c75; }
.cat.on { background: var(--amber); border-color: var(--amber); color: #1a1207; font-weight: 700; }
.search { margin-left: 8px; flex: 0 1 380px; display: flex; align-items: center; gap: 8px; height: 34px; padding: 0 12px; background: var(--panel); border: 1px solid var(--line2); border-radius: 6px; transition: border-color .15s ease; }
.search:focus-within { border-color: var(--amber); }
.search svg { width: 14px; height: 14px; color: var(--dim); flex: none; }
.search input { flex: 1; background: none; border: none; outline: none; color: var(--text); font-size: 13px; font-family: inherit; }
.search input::placeholder { color: var(--dim); }

/* 主区 */
.kbody { flex: 1; display: flex; min-height: 0; }
.kgrid { flex: 1; display: grid; grid-template-columns: repeat(5, minmax(0, 300px)); gap: 14px; justify-content: start; align-content: start; padding: 16px 20px; overflow-y: auto; grid-auto-rows: max-content; }
.nohit { grid-column: 1/-1; text-align: center; color: var(--dim); font-size: 16px; padding: 60px 0; }
.pcard { background: var(--panel); border: 1px solid var(--line); border-radius: 8px; overflow: hidden; cursor: pointer; transition: border-color .15s ease, transform .15s ease; user-select: none; display: flex; flex-direction: column; }
.pcard:hover { border-color: var(--amber); transform: translateY(-2px); }
.pcard:active { transform: translateY(0); }
.picwrap { position: relative; width: 100%; padding-top: 100%; background: var(--panel2); }
.picwrap img { position: absolute; inset: 0; width: 100%; height: 100%; object-fit: cover; }
.nopic { position: absolute; inset: 0; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 8px; color: var(--dim); }
.nopic .abbr { font-family: ui-monospace, "Cascadia Code", Consolas, monospace; font-size: 24px; font-weight: 700; letter-spacing: .12em; color: #31435f; text-transform: uppercase; }
.nopic span { font-size: 13px; letter-spacing: .1em; }
.pinfo { flex: 1; padding: 10px 12px 12px; border-top: 1px solid var(--line); display: flex; flex-direction: column; gap: 4px; }
/* 名称占满剩余空间，把价格/销量压到卡片最底部保证各行对齐 */
.pn { flex: 1; font-size: 16px; font-weight: 600; line-height: 1.4; word-break: break-all; color: var(--text); }
.meta { display: flex; align-items: baseline; justify-content: space-between; margin-top: 6px; }
.pp { font-size: 22px; font-weight: 700; color: var(--amber); }
.pp small { font-size: 14px; font-weight: 400; margin-right: 1px; }
.sold { font-size: 13px; color: var(--dim); }

/* 购物车 */
.cart { flex: none; width: 340px; display: flex; flex-direction: column; background: var(--panel); border-left: 1px solid var(--line); }
.cart-head { padding: 14px 16px; border-bottom: 1px solid var(--line); display: flex; align-items: center; justify-content: space-between; }
.cart-head .t { display: flex; align-items: center; gap: 10px; font-size: 15px; font-weight: 700; }
.cart-head svg { width: 17px; height: 17px; color: var(--amber); }
.cart-head .n { font-size: 12px; color: var(--amber); }
.cart-cols, .item { display: grid; grid-template-columns: 1fr 64px 84px 64px; align-items: center; gap: 6px; padding: 0 16px; }
.cart-cols { padding-top: 10px; padding-bottom: 8px; border-bottom: 1px solid var(--line); }
.cart-cols span { font-size: 11px; letter-spacing: .14em; color: var(--dim); text-transform: uppercase; }
.cart-cols span:nth-child(2), .cart-cols span:nth-child(3), .cart-cols span:nth-child(4) { text-align: right; }
.cart-body { flex: 1; overflow-y: auto; }
.item { padding-top: 12px; padding-bottom: 12px; border-bottom: 1px solid #141d31; }
.item .name { font-size: 13px; line-height: 1.35; }
.item .unit { font-size: 12px; color: var(--muted); text-align: right; }
.item .sub { font-size: 13px; font-weight: 600; color: var(--text); text-align: right; }
.qty { display: flex; align-items: center; justify-content: flex-end; gap: 8px; }
.qty button { width: 22px; height: 22px; border-radius: 5px; border: 1px solid var(--line2); background: transparent; color: var(--muted); cursor: pointer; font-size: 13px; line-height: 1; transition: all .12s ease; }
.qty button:hover { border-color: var(--amber); color: var(--amber); }
.qty b { font-size: 13px; min-width: 16px; text-align: center; }
.cart-empty { flex: 1; display: flex; align-items: center; justify-content: center; color: var(--dim); font-size: 16px; font-weight: 600; }
.cart-foot { padding: 16px; border-top: 1px solid var(--line); background: var(--panel2); }
.total { display: flex; align-items: baseline; justify-content: space-between; margin-bottom: 12px; }
.total .label { font-size: 11px; letter-spacing: .18em; text-transform: uppercase; color: var(--dim); font-weight: 600; }
.total .amt { font-size: 26px; font-weight: 700; color: var(--amber); }
.total .amt small { font-size: 13px; margin-left: 2px; color: var(--muted); }
.checkout { width: 100%; height: 44px; border: none; border-radius: 7px; background: var(--amber); color: #1a1207; font-size: 15px; font-weight: 700; letter-spacing: .1em; cursor: pointer; transition: filter .15s ease, transform .1s ease; font-family: inherit; }
.checkout:hover { filter: brightness(1.08); }
.checkout:active { transform: translateY(1px); }
.checkout:disabled { opacity: .45; cursor: not-allowed; }
/* 重新连接按钮：更宽更醒目 */
.reconnect { width: 240px; height: 52px; font-size: 17px; }
.pay-hint { margin-top: 10px; text-align: center; font-size: 11px; color: var(--dim); letter-spacing: .06em; }
.linkbtn { display: block; width: 100%; margin-top: 8px; background: none; border: none; color: var(--muted); font-size: 13px; cursor: pointer; text-align: center; font-family: inherit; }
.linkbtn:hover { color: var(--text); }

/* 弹窗（dark 主题覆盖 Element Plus） */
.el-overlay { background: rgba(0,0,0,.6); }
.el-dialog { background: var(--panel); border: 1px solid var(--line); border-radius: 10px; box-shadow: 0 12px 40px rgba(0,0,0,.5); }
.el-dialog__title { color: var(--text); font-weight: 700; }
.el-dialog__headerbtn .el-dialog__close { color: var(--muted); }
.clist { max-height: 240px; overflow-y: auto; margin: 10px 0; font-size: 14px; color: var(--text); }
.clist div { display: flex; justify-content: space-between; padding: 8px 2px; border-bottom: 1px dashed var(--line); }
.clist span:first-child { color: var(--muted); }
.ctotal { text-align: right; font-size: 16px; margin: 12px 0 18px; color: var(--text); }
.ctotal b { color: var(--amber); font-size: 24px; }
.pay-label { font-size: 13px; color: var(--muted); margin-bottom: 10px; }
.paybtns { display: flex; gap: 8px; }
.paybtns button { flex: 1; height: 40px; border: 1px solid var(--line2); border-radius: 7px; background: transparent; color: var(--muted); font-size: 14px; cursor: pointer; transition: all .15s ease; font-family: inherit; }
.paybtns button:hover { color: var(--text); border-color: #3a4c75; }
.paybtns button.on { background: var(--amber); border-color: var(--amber); color: #1a1207; font-weight: 700; }
.el-alert { background: var(--panel2); border: 1px solid var(--line); }
.el-alert__title { color: var(--text); }
.qrimg { width: 260px; height: 260px; border-radius: 8px; }
/* 居中提示弹窗（ElMessageBox）dark —— !important 压过 Element Plus 默认 */
.el-message-box { background: var(--panel) !important; border: 1px solid var(--line) !important; border-radius: 10px !important; }
.el-message-box__title { color: var(--text) !important; }
.el-message-box__message { color: var(--text) !important; }
.el-message-box__btns .el-button--primary { background: var(--amber) !important; border-color: var(--amber) !important; color: #1a1207 !important; }
/* 连不上吧台 error 状态 */
.el-result__title { color: var(--text); }
.el-result__subtitle { color: var(--muted); }
</style>
