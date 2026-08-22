import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'
import AutoImport from 'unplugin-auto-import/vite'
import Components from 'unplugin-vue-components/vite'
import { ElementPlusResolver } from 'unplugin-vue-components/resolvers'

// 手机端添加商品页面：构建产物由管理端 HTTP 服务托管在 /m/ 子路径下，
// 资源引用必须是相对路径（base:'./'），否则 /m/ 页面去请求 /assets/* → 404 白屏
export default defineConfig({
  base: './',
  resolve: {
    alias: [
      // rmbg-webgpu 源码写死 import "onnxruntime-web/webgpu"，但我们强制 WASM（局域网 HTTP 用不了 WebGPU），
      // 且 webgpu 入口运行时加载的是 asyncify 版 wasm（24MB 冗余）。把 webgpu 子路径重定向到纯 wasm 入口，
      // onnxruntime 只保留运行时真正需要的 ort-wasm-simd-threaded.wasm（托管在 /m/bgrem/）。
      // 注意：Vite 的 alias 不支持 webpack 的 "$" 精确匹配后缀，字符串键只按精确/前缀匹配，
      // "xxx$" 这种键永远匹配不到 → 必须用正则 find。
      { find: /^onnxruntime-web\/webgpu$/, replacement: 'onnxruntime-web/wasm' }
    ]
  },
  plugins: [
    vue(),
    AutoImport({ resolvers: [ElementPlusResolver()] }),
    Components({ resolvers: [ElementPlusResolver()] }),
    // 剔除被 vite 当资产打包进 dist/assets 的 onnxruntime wasm（ort-wasm-*.wasm）。
    // 运行时 wasm 统一由 wasmPaths 指向 /m/bgrem/ 加载（public/bgrem 里已有一份），
    // 再打包一份纯属重复死重（13~24MB）。
    {
      name: 'strip-ort-wasm-assets',
      apply: 'build',
      generateBundle(_, bundle) {
        for (const k of Object.keys(bundle)) {
          if (/ort-wasm.*\.wasm$/.test(k)) delete bundle[k]
        }
      }
    }
  ],
  clearScreen: false,
  server: {
    host: true, // 开发时手机浏览器可访问 http://本机IP:14203 调试
    port: 14203,
    strictPort: true,
    watch: { ignored: ['**/src-tauri/target/**'] }
  }
})
