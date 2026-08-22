// 管理端抠图：与手机端同一套 @planby-tech/rmbg-webgpu 方案，区别只在资源来源——
// 模型和 onnxruntime wasm 不打包进管理端 exe，而是直接用手机端页面托管的 /m/bgrem/
// （服务端 CorsLayer::permissive()，跨源 fetch 没问题），全系统只存一份模型。
// 缓存同样走 IndexedDB（键 = 模型 URL，换模型文件名旧缓存自然失效）。
// ★ 换模型时：mobile/src/bgrem.js 的 MODEL_URL 和这里要一起改。
//
// 抠图放在一次性 Web Worker（bgrem-worker.js）里跑、用完 terminate：
// onnxruntime 的 WASM 堆只 grow 不 shrink，dispose() 拿不回内存，唯有销毁 worker 才能整体归还 OS，
// 避免管理端 24h 常驻时抠图一次就永久占用 200MB。本文件不 import rmbg，它只在 worker 里加载。

import { API } from './api'

const BGREM_BASE = `${API}/m/bgrem/`   // 吧台机 HTTP 服务托管的手机端资源目录
const MODEL_URL = BGREM_BASE + 'u2netp.onnx'

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

async function getCached(url) {
  try {
    const db = await openDb()
    return await new Promise((resolve) => {
      const rq = db.transaction(STORE, 'readonly').objectStore(STORE).get(url)
      rq.onsuccess = () => resolve(rq.result ?? null)
      rq.onerror = () => resolve(null)
    })
  } catch {
    return null
  }
}

async function setCached(url, buf) {
  try {
    const db = await openDb()
    await new Promise((resolve, reject) => {
      const tx = db.transaction(STORE, 'readwrite')
      tx.objectStore(STORE).put(buf, url)
      tx.oncomplete = resolve
      tx.onerror = () => reject(tx.error)
    })
  } catch { /* 隐私模式等场景静默跳过缓存 */ }
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

/**
 * 抠图：返回透明背景 PNG Blob。失败抛异常，调用方处理。
 * 回调 onProgress({stage})：download（首次下载模型，带 percent 0~100）/ image（开始处理图片）。
 * 模型有缓存时直接读 IndexedDB（不报 download 阶段），无缓存才下载并缓存。
 * 抠图在一次性 Worker 里执行，拿到结果即 terminate，释放 onnxruntime 的 WASM 堆（处理进度由调用方模拟）。
 * @param {File} file 原图
 * @param {(e:{stage:string,percent?:number})=>void} onProgress
 */
export async function cutout(file, onProgress) {
  const { buf, source } = await loadModel(MODEL_URL, p => onProgress?.({ stage: 'download', percent: p }))
  if (source === 'network') onProgress?.({ stage: 'download', percent: 100 }) // 下载完成
  onProgress?.({ stage: 'image' }) // 模型就位，开始抠图
  return await new Promise((resolve, reject) => {
    const worker = new Worker(new URL('./bgrem-worker.js', import.meta.url), { type: 'module' })
    worker.onmessage = (e) => {
      worker.terminate() // 抠完立刻销毁 worker，WASM 堆整体归还 OS
      if (e.data?.ok) resolve(e.data.blob)
      else reject(new Error(e.data?.message || '抠图失败'))
    }
    worker.onerror = (e) => {
      worker.terminate()
      reject(new Error(e.message || '抠图 Worker 异常'))
    }
    // transfer 模型 ArrayBuffer 给 worker，主线程不再持有（避免双份拷贝）
    worker.postMessage({ model: buf, wasmPaths: BGREM_BASE, image: file }, [buf])
  })
}
