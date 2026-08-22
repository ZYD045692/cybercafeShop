// 开发数据隔离（参考 Landisk dev-data）：首次开发时生成 dev-data/
// 数据/图片来自 assets/dev-seed（几个测试商品），音频/收款码/config.ini 来自 assets
// dev 模式的管理端只读写 dev-data/，与生产发布目录完全隔开
const fs = require('fs')
const path = require('path')
const { execSync } = require('child_process')

const root = path.join(__dirname, '..')
const assets = path.join(root, 'assets')
const dst = path.join(root, 'dev-data')

// 手机端页面同步：每次跑 seed 都强制重新构建 mobile/dist 并同步到 dev-data/web/m，
// 改了 mobile/ 源码直接跑 dev-admin 即可，不用手动 build
syncMobile()
function syncMobile() {
  const mobileDist = path.join(root, 'mobile', 'dist')
  const webM = path.join(dst, 'web', 'm')
  console.log('[seed] 构建手机端页面（npm run build:mobile）…')
  execSync('npm run build:mobile', { cwd: root, stdio: 'inherit' })
  fs.rmSync(webM, { recursive: true, force: true })
  fs.mkdirSync(webM, { recursive: true })
  fs.cpSync(mobileDist, webM, { recursive: true })
  console.log('[seed] 手机端页面已同步到 dev-data/web/m')
}

if (fs.existsSync(path.join(dst, 'data', 'db', 'shop_db.db'))) {
  console.log('[seed] dev-data/ 已存在，跳过数据初始化')
  process.exit(0)
}

fs.mkdirSync(dst, { recursive: true })
// 目录结构与生产一致：data\db(库) data\image(图) data\qrcode(收款码) data\sound(音频) web\m(手机页)
fs.cpSync(path.join(assets, 'data', 'sound'), path.join(dst, 'data', 'sound'), { recursive: true })
fs.cpSync(path.join(assets, 'data', 'qrcode'), path.join(dst, 'data', 'qrcode'), { recursive: true })
fs.cpSync(path.join(assets, 'dev-seed', 'data'), path.join(dst, 'data', 'db'), { recursive: true })
fs.cpSync(path.join(assets, 'dev-seed', 'image'), path.join(dst, 'data', 'image'), { recursive: true })
// dev 环境的 config.ini：管理端读 port，用户端读 host+port+contact，手机端读 [mobile] pass，共用这一份
fs.writeFileSync(path.join(dst, 'config.ini'), [
  '; dev 开发环境配置（管理端/用户端/手机端都从这份读，与生产隔开）',
  '; host: 用户端要连的管理端 IP——本机联调 127.0.0.1；真机联测改成管理端机器的局域网 IP',
  '; port: 两端必须一致',
  '; contact: 连不上管理端时警告框里显示的联系方式',
  '[server]',
  'host = 127.0.0.1',
  'port = 21974',
  'contact = 13800000000',
  ''
].join('\r\n'))
console.log('[seed] 已初始化 dev-data/（测试商品来自 assets/dev-seed，音频/收款码来自 assets，config.ini 为 dev 专用）')
