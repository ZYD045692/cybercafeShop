// 管理端 API 封装：一律走本机回环（/api/admin/* 有本机守卫；公开路径生产门禁对本机回环免票）
export const PORT = window.__PORT__ || 21974
export const API = `http://127.0.0.1:${PORT}`

export async function api(path, opts = {}) {
  const r = await fetch(API + path, opts)
  const body = await r.json().catch(() => ({}))
  if (!r.ok) throw new Error(body.error || `请求失败 ${r.status}`)
  return body
}

export const post = (path, data) =>
  api(path, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(data) })

export const del = path => api(path, { method: 'DELETE' })

export const upload = (path, blob) =>
  api(path, { method: 'POST', headers: { 'Content-Type': 'application/octet-stream' }, body: blob })

export const imgUrl = name => `${API}/image/${name}`
export const qrUrl = kind => `${API}/qrcode/${kind}?t=${Date.now()}`

// 任意图片文件 → 300x300 白底 JPEG/PNG Blob（canvas 处理，不占后端资源）
export function to300(file, type = 'image/jpeg') {
  return new Promise((resolve, reject) => {
    const url = URL.createObjectURL(file)
    const img = new Image()
    img.onload = () => {
      const c = document.createElement('canvas')
      c.width = c.height = 300
      const ctx = c.getContext('2d')
      ctx.fillStyle = '#fff'
      ctx.fillRect(0, 0, 300, 300)
      const s = Math.min(290 / img.width, 290 / img.height)
      const w = img.width * s, h = img.height * s
      ctx.drawImage(img, (300 - w) / 2, (300 - h) / 2, w, h)
      URL.revokeObjectURL(url)
      c.toBlob(b => b ? resolve(b) : reject(new Error('图片处理失败')), type, 0.9)
    }
    img.onerror = () => { URL.revokeObjectURL(url); reject(new Error('无法读取图片')) }
    img.src = url
  })
}
