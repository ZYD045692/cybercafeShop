// 一键打包：
//   1. 构建手机端页面（mobile/dist，打进管理端安装包 seed/web/m，管理端托管 /m/）
//   2. 编译管理端（产出 NSIS 安装包：内含管理端 exe + 内嵌网页 + seed 首启数据）
//   3. 编译用户端（网页内嵌 exe，绿色单文件，不安装）
//   4. 组装 dist\：cybercafeShop-admin_v版本_setup.exe + 用户端\莱尚网电竞馆点购.exe+config.ini
const fs = require('fs')
const path = require('path')
const { execSync } = require('child_process')

const root = path.join(__dirname, '..')
const run = cmd => { console.log(`[pack] ${cmd}`); execSync(cmd, { cwd: root, stdio: 'inherit' }) }

// 1. 手机端页面先构建（管理端安装包会把它作为 seed/web/m 带进去）
run('npm run build:mobile')
// 2+3. 两端 Tauri 编译（用户端网页已内嵌 exe，由 tauri build 的 beforeBuildCommand 自动构建；
//    管理端安装包带 assets/seed 首启数据）
run('npm run tauri:build -w cybercafeShop-admin')
run('npm run tauri:build -w cybercafeShop-client')

// 3. 组装 dist/
const dist = path.join(root, 'dist')
const clientOut = path.join(dist, '用户端')
const adminExe = path.join(root, 'admin/src-tauri/target/release/莱尚网电竞馆点购管理端.exe')
const clientExe = path.join(root, 'client/src-tauri/target/release/莱尚网电竞馆点购.exe')
const nsisDir = path.join(root, 'admin/src-tauri/target/release/bundle/nsis')

for (const [name, p] of [['管理端', adminExe], ['用户端', clientExe]]) {
  if (!fs.existsSync(p)) {
    console.error(`[pack] 找不到 ${name} 编译产物: ${p}`)
    process.exit(1)
  }
}
const setupExe = fs.readdirSync(nsisDir).find(f => f.endsWith('-setup.exe'))
if (!setupExe) {
  console.error('[pack] 找不到管理端 NSIS 安装包（target/release/bundle/nsis/*-setup.exe）')
  process.exit(1)
}
// 从 NSIS 原始名提取版本号（形如 "xxx_0.1.0_x64-setup.exe"）
const verMatch = setupExe.match(/_(\d+\.\d+\.\d+)_/)
const ver = verMatch ? verMatch[1] : '0.4.0'

fs.rmSync(dist, { recursive: true, force: true })
fs.mkdirSync(clientOut, { recursive: true })

fs.copyFileSync(path.join(nsisDir, setupExe), path.join(dist, `cybercafeShop-admin_v${ver}_setup.exe`))
fs.copyFileSync(clientExe, path.join(clientOut, '莱尚网电竞馆点购.exe'))
fs.copyFileSync(path.join(root, 'client/src-tauri/config.ini'), path.join(clientOut, 'config.ini'))

console.log('[pack] 完成！')
console.log(`  dist\\cybercafeShop-admin_v${ver}_setup.exe  ->  吧台主机运行安装（当前用户安装，无需管理员）`)
console.log('  dist\\用户端\\           ->  先改 config.ini 的 host/contact，再拷到每台客户机')
