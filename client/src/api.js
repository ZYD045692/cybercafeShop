// 用户端 API：生产模式页面由管理端主机托管（同源），dev 模式走 vite（用壳注入的 host:port）
import { hmacSha256 } from './hmac'

export const MACHINE = window.__MACHINE__ || 'unknown'
const KEY = window.__KEY__ || ''
const OFFSET = Number(window.__OFFSET__ || 0) // 服务器时间偏移（秒），客户机时钟不准也能签名

// 页面内嵌 exe（tauri 协议，origin 不是 http），API 一律走壳注入的管理端地址
export const BASE = `http://${window.__HOST__ || '127.0.0.1'}:${window.__PORT__ || 21974}`

// 时间票：ts = 服务器时间，sig = HMAC(密钥, ts)。60 秒缓存复用，服务端窗口 ±300 秒。
let cached = { ts: 0, sig: '' }
export function ticket() {
  const now = Math.floor(Date.now() / 1000) + OFFSET
  if (now - cached.ts > 60) {
    cached = { ts: now, sig: hmacSha256(KEY, String(now)) }
  }
  return cached
}

const authHeaders = () => {
  const t = ticket()
  return { 'x-ts': String(t.ts), 'x-sig': t.sig }
}

// 图片/收款码等 <img> 标签带不了 header，用 query 票据
export const ticketQ = () => {
  const t = ticket()
  return `ts=${t.ts}&sig=${t.sig}`
}

async function getJson(path) {
  const r = await fetch(BASE + path, { headers: authHeaders() })
  const d = await r.json().catch(() => ({}))
  if (!r.ok) throw new Error(d.error || '连接吧台主机失败')
  return d
}

export async function loadInit() {
  // 页面加载时一次性拉取：店铺信息 + 商品 + 分类 + 两个收款码（缓存为本地对象 URL，下单时直接用）
  const [info, d] = await Promise.all([getJson('/api/shopinfo'), getJson('/api/products')])
  const qr = {}
  for (const k of ['wechat', 'alipay']) {
    try {
      const resp = await fetch(`${BASE}/qrcode/${k}`, { headers: authHeaders() })
      if (resp.ok) qr[k] = URL.createObjectURL(await resp.blob())
    } catch { /* 某个收款码未配置 */ }
  }
  return { shopName: info.shop_name || '', welcome: info.welcome || '', categories: d.categories, products: d.products, qr }
}

export async function submitOrder(payMethod, items) {
  const r = await fetch(BASE + '/api/order', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json', ...authHeaders() },
    body: JSON.stringify({ machine: MACHINE, pay_method: payMethod, items }),
  })
  const d = await r.json().catch(() => ({}))
  if (!r.ok) throw new Error(d.error || '下单失败')
  return d
}

export async function callNet() {
  const r = await fetch(BASE + '/api/call', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json', ...authHeaders() },
    body: JSON.stringify({ machine: MACHINE }),
  })
  if (!r.ok) throw new Error('呼叫失败')
}

export const imgUrl = name => `${BASE}/image/${name}?${ticketQ()}`
