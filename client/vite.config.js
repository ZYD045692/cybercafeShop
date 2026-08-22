import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'
import AutoImport from 'unplugin-auto-import/vite'
import Components from 'unplugin-vue-components/vite'
import { ElementPlusResolver } from 'unplugin-vue-components/resolvers'

// 开发端口 14202；生产打包到 dist/
export default defineConfig({
  // 生产页面挂在管理端 HTTP 服务的 /shop/ 子路径下，资源引用必须是相对路径
  // （默认 '/' 会让 /shop/ 页面去请求 /assets/* → 404 白屏）
  base: './',
  plugins: [
    vue(),
    // Element Plus 按需加载（参考 Landisk）：用到哪个组件才打包哪个
    AutoImport({ resolvers: [ElementPlusResolver()] }),
    Components({ resolvers: [ElementPlusResolver()] })
  ],
  clearScreen: false,
  server: {
    host: true, // 监听局域网：dev 模式让其他电脑可访问 http://本机IP:14202
    port: 14202,
    strictPort: true,
    watch: { ignored: ['**/src-tauri/target/**'] }
  }
})
