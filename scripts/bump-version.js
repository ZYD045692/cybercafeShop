// 一键修改项目版本号：把所有 package.json / tauri.conf.json / Cargo.toml 里的 version 同步为新值。
// 用法：node scripts/bump-version.js 0.3.0
// 说明：三段式 semver（x.y.z），前短后短即各段不带前导零。
// 覆盖 workspace：root + admin + client + mobile 的 package.json、两端 tauri.conf.json、三个 Cargo.toml、
// 三个 Cargo.lock 里的本包版本、package-lock.json 顶层版本、pack.js 默认回退值。
const fs = require('fs')
const path = require('path')

const newVer = process.argv[2]
if (!newVer || !/^\d+\.\d+\.\d+$/.test(newVer)) {
  console.error('用法: node scripts/bump-version.js 0.2.0  （三段式版本号，如 0.2.0）')
  process.exit(1)
}

const root = path.join(__dirname, '..')

// 目标文件：JSON 用 "version": "x.y.z"；TOML 用 version = "x.y.z"
const jsonFiles = [
  'package.json',
  'admin/package.json',
  'client/package.json',
  'mobile/package.json',
  'admin/src-tauri/tauri.conf.json',
  'client/src-tauri/tauri.conf.json',
]
const tomlFiles = [
  'admin/src-tauri/Cargo.toml',
  'admin/src-tauri/server/Cargo.toml',
  'client/src-tauri/Cargo.toml',
]

let changed = 0

function bumpJson(file) {
  const p = path.join(root, file)
  if (!fs.existsSync(p)) return
  const text = fs.readFileSync(p, 'utf8')
  const next = text.replace(/"version"\s*:\s*"\d+\.\d+\.\d+"/, `"version": "${newVer}"`)
  if (next !== text) {
    fs.writeFileSync(p, next)
    changed++
    console.log(`  ✓ ${file}`)
  } else {
    console.log(`  - ${file}（未匹配到版本号）`)
  }
}

function bumpToml(file) {
  const p = path.join(root, file)
  if (!fs.existsSync(p)) return
  const text = fs.readFileSync(p, 'utf8')
  const next = text.replace(/^version\s*=\s*"\d+\.\d+\.\d+"/m, `version = "${newVer}"`)
  if (next !== text) {
    fs.writeFileSync(p, next)
    changed++
    console.log(`  ✓ ${file}`)
  } else {
    console.log(`  - ${file}（未匹配到版本号）`)
  }
}

console.log(`将版本号统一改为 ${newVer}：\n`)
jsonFiles.forEach(bumpJson)
tomlFiles.forEach(bumpToml)

// 三个 Cargo.lock 里的本包版本（Cargo.lock 不会随 Cargo.toml 自动重写顶级包版本，需手工同步。
// 只匹配 [[package]] 块里 name 为 cybercafeShop-* 的版本；不会误改依赖项的版本号。
const cargoLocks = [
  'admin/src-tauri/Cargo.lock',
  'admin/src-tauri/server/Cargo.lock',
  'client/src-tauri/Cargo.lock',
]
for (const f of cargoLocks) {
  const p = path.join(root, f)
  if (!fs.existsSync(p)) continue
  const text = fs.readFileSync(p, 'utf8')
  // 把 "cybercafeShop-server|-admin|-client" 的 [[package]] 块版本统一为新值
  const next = text.replace(
    /(name = "cybercafeShop-(?:admin|client|server)"\nversion = ")\d+\.\d+\.\d+(")/g,
    (m, a, b) => a + newVer + b
  )
  if (next !== text) {
    fs.writeFileSync(p, next)
    changed++
    console.log(`  ✓ ${f}（Cargo.lock 本包版本）`)
  } else {
    console.log(`  - ${f}（Cargo.lock 未匹配到本包版本）`)
  }
}

// package-lock.json 顶层 version（根 workspace 版本；npm install 也会同步，这里主动改一次）
const pkgLock = path.join(root, 'package-lock.json')
if (fs.existsSync(pkgLock)) {
  const pt = fs.readFileSync(pkgLock, 'utf8')
  // 只改顶层包（"name": "cybercafeShop" 紧跟的 version），不碰内部依赖包的版本
  const pn = pt.replace(/("name": "cybercafeShop"\n\s*"version": ")\d+\.\d+\.\d+(")/, (m, a, b) => a + newVer + b)
  if (pn !== pt) {
    fs.writeFileSync(pkgLock, pn)
    changed++
    console.log('  ✓ package-lock.json（顶层版本）')
  }
}

// 同步 pack.js 里的默认回退版本（NSIS 提取失败时用）
const packJs = path.join(root, 'scripts/pack.js')
const pk = fs.readFileSync(packJs, 'utf8')
const pkNext = pk.replace(/ver = verMatch \? verMatch\[1\] : '[^']+'/, `ver = verMatch ? verMatch[1] : '${newVer}'`)
if (pkNext !== pk) {
  fs.writeFileSync(packJs, pkNext)
  changed++
  console.log('  ✓ scripts/pack.js（默认版本回退值）')
}

console.log(`\n完成，共修改 ${changed} 个文件。`)
console.log('已一并同步：package.json / tauri.conf.json / Cargo.toml / Cargo.lock(本包) / package-lock.json / pack.js 回退值。')
console.log('若 Cargo.lock 里还有其它本包版本残留（如历史名称），可重跑一次 cargo build/test 让其收敛。')
