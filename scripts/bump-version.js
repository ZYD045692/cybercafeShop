// 一键修改项目版本号：把所有 package.json / tauri.conf.json / Cargo.toml 里的 version 同步为新值。
// 用法：node scripts/bump-version.js 0.2.0
// 说明：三段式 semver（x.y.z），前短后短即各段不带前导零；改完记得重新 npm install 同步 package-lock。
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
console.log('注意：package-lock.json 里的版本在下次 npm install 时自动同步；若需立即同步可重跑 install。')
