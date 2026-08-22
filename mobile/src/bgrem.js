// @planby-tech/rmbg-webgpu 封装：浏览器本地 U²-Netp 抠图，模型+wasm 由吧台机托管（离线可用）。
// 强制用 WASM（WebGPU 需要 HTTPS/localhost，手机走 http://吧台IP 是明文 HTTP，不能用 WebGPU）。
// 成功返回透明 PNG Blob；失败抛异常，由调用方回退到"原图仅旋转/缩放"。

import { RmbgSession } from '@planby-tech/rmbg-webgpu'

// 模型与 onnxruntime wasm 资源位于页面同级的 bgrem/ 目录（构建时拷进 mobile/public/bgrem）。
// 必须基于 location.href 解析成绝对 URL 且带结尾斜杠：
// onnxruntime 对 wasmPaths 只做字符串拼接（不补 /），且相对路径会按「当前 JS 模块的 URL」
// （/m/assets/index-*.js）而不是页面 URL 解析 → 相对写法会拼出 /m/assets/bgremort-xxx 这种 404。
const BGREM_BASE = new URL('bgrem/', location.href).href
// 模型 URL：/m/bgrem/u2netp.onnx。改这里即换模型，无需改缓存键逻辑
const MODEL_URL = BGREM_BASE + 'u2netp.onnx'

// IndexedDB：把下载到的模型存起来，下次直接读内存，彻底离线、免重复请求。
// 键 = 模型 URL，值 = ArrayBuffer（onnxruntime 支持从内存加载模型）。
const DB_NAME = 'bgrem'
const STORE = 'models'

function openDb() {
  return new Promise((resolve, reject) => {
    const req = indexedDB.open(DB_NAME, 1)
    req.onupgradeneeded = () => req.result.createObjectStore(STORE)
    req.onsuccess = () => resolve(req.result)
    req.onerror = () => reject(req.error)
  })
}

/** 读模型缓存；没有则返回 null */
async function getCached(url) {
  try {
    const db = await openDb()
    return await new Promise((resolve) => {
      const tx = db.transaction(STORE, 'readonly')
      const rq = tx.objectStore(STORE).get(url)
      rq.onsuccess = () => resolve(rq.result ?? null)
      rq.onerror = () => resolve(null)
    })
  } catch {
    return null
  }
}

/** 把模型写入缓存；失败静默忽略（不影响抠图主流程） */
async function setCached(url, buf) {
  try {
    const db = await openDb()
    await new Promise((resolve, reject) => {
      const tx = db.transaction(STORE, 'readwrite')
      tx.objectStore(STORE).put(buf, url)
      tx.oncomplete = resolve
      tx.onerror = () => reject(tx.error)
    })
  } catch {
    /* 忽略：隐私模式/被禁用时仍走网络 */
  }
}

/** 取模型数据：优先 IndexedDB 缓存（source='cache'），否则流式下载（source='network'，onProgress 报下载进度） */
async function loadModel(url, onProgress) {
  const cached = await getCached(url)
  if (cached && cached.byteLength > 0) return { buf: cached, source: 'cache' }
  onProgress?.(0)
  const r = await fetch(url)
  if (!r.ok) throw new Error('模型下载失败 ' + r.status)
  const total = +r.headers.get('content-length') || 0
  const reader = r.body.getReader()
  const chunks = []
  let received = 0
  while (true) {
    const { done, value } = await reader.read()
    if (done) break
    chunks.push(value)
    received += value.length
    if (total) onProgress?.(Math.round((received / total) * 100))
  }
  const bytes = new Uint8Array(received)
  let off = 0
  for (const c of chunks) { bytes.set(c, off); off += c.length }
  const buf = bytes.buffer
  setCached(url, buf)
  return { buf, source: 'network' }
}

let activeSession = null // 当前活跃的抠图 session（组件卸载时主动释放，防内存泄漏）

/** 组件卸载时调用：释放仍在进行的模型 session（dispose 后其 run 会中断并抛错，被调用方 catch） */
export async function disposeBgrem() {
  const s = activeSession
  activeSession = null
  if (s) await s.dispose().catch(() => {})
}

/**
 * 抠图：返回透明背景 PNG Blob。
 * 回调 onProgress({stage})：download（首次下载模型，带 percent 0~100）/ image（开始处理图片）/ done（完成）。
 * 模型有缓存时直接读 IndexedDB（不报 download 阶段），无缓存才下载并缓存。
 * 该库无内建进度回调，处理图片阶段由调用方模拟进度。
 * @param {File} file 原图
 * @param {(e:{stage:string,percent?:number})=>void} onProgress
 */
export async function cutout(file, onProgress) {
  const { buf, source } = await loadModel(MODEL_URL, p => onProgress?.({ stage: 'download', percent: p }))
  if (source === 'network') onProgress?.({ stage: 'download', percent: 100 }) // 下载完成
  onProgress?.({ stage: 'image' }) // 模型就位，开始抠图
  const session = await RmbgSession.create({
    model: 'u2netp',
    modelUrl: buf,             // 直接喂内存里的模型，离线可用
    executionProviders: ['wasm'], // 强制 CPU/WASM，局域网 HTTP 下 WebGPU 不可用
    wasmPaths: BGREM_BASE,     // onnxruntime wasm（ort-wasm-simd-threaded.*）所在目录，结尾带 /
  })
  activeSession = session
  try {
    const res = await session.remove(file)
    return res.outputBlob
  } finally {
    activeSession = null
    await session.dispose().catch(() => {})
  }
}
