// 手机端 API：同源（页面由管理端 HTTP 托管在 /m/），无口令鉴权
export const BASE = location.origin

async function json(path, opts = {}) {
  const r = await fetch(BASE + path, { ...opts })
  const d = await r.json().catch(() => ({}))
  if (!r.ok) {
    const err = new Error(d.error || `请求失败 ${r.status}`)
    err.status = r.status
    throw err
  }
  return d
}

export function getCategories() {
  return json('/api/m/categories')
}

export function addProduct(payload) {
  return json('/api/m/product', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(payload),
  })
}

// 商品建好后传图：文件名由服务端按最终缩拼生成，前端不指定（避免缩拼冲突时互覆图片）
export async function uploadProductImage(id, blob) {
  const r = await fetch(`${BASE}/api/m/product/${id}/image`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/octet-stream' },
    body: blob,
  })
  const d = await r.json().catch(() => ({}))
  if (!r.ok) throw new Error(d.error || `上传失败 ${r.status}`)
  return d
}
