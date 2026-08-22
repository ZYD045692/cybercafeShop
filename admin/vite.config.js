import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'
import AutoImport from 'unplugin-auto-import/vite'
import Components from 'unplugin-vue-components/vite'
import { ElementPlusResolver } from 'unplugin-vue-components/resolvers'
import { visualizer } from 'rollup-plugin-visualizer'
import { resolve } from 'path'

// 开发端口 14201（不常见，避免冲突）；生产打包到 dist/
export default defineConfig({
  resolve: {
    alias: [
      // rmbg-webgpu 源码写死 import "onnxruntime-web/webgpu"，但我们强制 WASM（局域网 HTTP 用不了 WebGPU），
      // 且 webgpu 入口运行时加载的是 asyncify 版 wasm（24MB 冗余）。重定向到纯 wasm 入口。
      // 注意：Vite alias 不支持 webpack 的 "$" 后缀精确匹配，必须用正则 find。
      { find: /^onnxruntime-web\/webgpu$/, replacement: 'onnxruntime-web/wasm' }
    ]
  },
  plugins: [
    vue(),
    // Element Plus 按需加载（参考 Landisk）：用到哪个组件才打包哪个
    AutoImport({ resolvers: [ElementPlusResolver()] }),
    Components({ resolvers: [ElementPlusResolver()] }),
    // 仅生产 build 时生成打包体积分析报告（apply:'build' 限定，dev 不跑）；
    // 输出到 admin/ 根目录（dist 之外），避免被 tauri 打进安装包/exe
    visualizer({
      filename: resolve(__dirname, 'bundle-stats.html'),
      gzipSize: true,
      brotliSize: true,
      open: false,
      apply: 'build',
    }),
    // 剔除被打包进 dist/assets 的 onnxruntime wasm（ort-wasm-*.wasm）：
    // 运行时统一从手机端托管的 /m/bgrem/ 加载（见 src/bgrem.js 的 wasmPaths），
    // exe 里再打一份纯属死重（13~24MB）。与 mobile/vite.config.js 同理。
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
    host: true, // 监听局域网：dev 模式让其他设备可访问 http://本机IP:14201
    port: 14201,
    strictPort: true,
    // 不监视 Rust 编译输出目录（cargo 写 DLL 时 Windows 文件锁会让 watcher 崩溃）
    watch: { ignored: ['**/src-tauri/target/**'] }
  },
  build: {
    rollupOptions: {
      input: {
        main: resolve(__dirname, 'index.html'),
        notify: resolve(__dirname, 'notify.html')
      }
    }
  }
})
