// 管理端抠图 Worker：每次抠图由主线程新建、用完 terminate。
// onnxruntime 的 WASM 堆（WebAssembly.Memory 只 grow 不 shrink）随 worker 销毁整体归还 OS，
// 不再像主线程单例那样常驻 200MB。模型/图片/wasmPaths 都由主线程传入。
// 本文件只在抠图时被实例化，其 import 的 @planby-tech/rmbg-webgpu（含 onnxruntime-web）不会进主 bundle。
import { RmbgSession } from '@planby-tech/rmbg-webgpu'

self.onmessage = async (e) => {
  const { model, wasmPaths, image } = e.data
  try {
    const session = await RmbgSession.create({
      model: 'u2netp',
      modelUrl: model,             // 主线程从 IndexedDB/网络拿到的 ArrayBuffer
      executionProviders: ['wasm'],
      wasmPaths,                   // http://127.0.0.1:21974/m/bgrem/
    })
    const res = await session.remove(image)
    await session.dispose().catch(() => {}) // 清理 C++ 侧 session；真正释放 WASM 堆靠主线程 terminate 本 worker
    self.postMessage({ ok: true, blob: res.outputBlob })
  } catch (err) {
    self.postMessage({ ok: false, message: String(err?.message || err) })
  }
}
