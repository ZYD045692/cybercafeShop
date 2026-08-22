// 管理端抠图：与手机端同一套 @planby-tech/rmbg-webgpu 方案，区别只在资源来源——
// 模型和 onnxruntime wasm 不打包进管理端 exe，而是直接用手机端页面托管的 /m/bgrem/
// （服务端 CorsLayer::permissive()，跨源 fetch 没问题），全系统只存一份模型。
// 缓存同样走 IndexedDB（键 = 模型 URL，换模型文件名旧缓存自然失效）。
// ★ 换模型时：mobile/src/bgrem.js 的 MODEL_URL 和这里要一起改。

import { removeBackground } from '@planby-tech/rmbg-webgpu'
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

async function loadModel(url) {
  const cached = await getCached(url)
  if (cached && cached.byteLength > 0) return cached
  const r = await fetch(url)
  if (!r.ok) throw new Error('模型下载失败 ' + r.status)
  const buf = await r.arrayBuffer()
  setCached(url, buf)
  return buf
}

/**
 * 抠图：返回透明背景 PNG Blob。失败抛异常，调用方回退为原图。
 * @param {File} file 原图
 * @param {(p:number)=>void} onProgress 0~100 进度
 */
export async function cutout(file, onProgress) {
  onProgress?.(20)
  const model = await loadModel(MODEL_URL)
  onProgress?.(50)
  const blob = await removeBackground(file, {
    model: 'u2netp',              // 预设参数（320×320 / ImageNet 归一化 / minmax 输出）
    modelUrl: model,              // 模型本体从 IndexedDB / /m/bgrem/ 拿
    executionProviders: ['wasm'], // 与手机端一致，强制 WASM
    wasmPaths: BGREM_BASE,        // wasm 也从手机端目录加载（结尾带 /）
  })
  onProgress?.(100)
  return blob
}
