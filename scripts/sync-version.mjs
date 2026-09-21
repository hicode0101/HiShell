#!/usr/bin/env node
/**
 * 版本号同步：以 tag 版本为准，写入三处代码版本
 *   1. package.json            → version
 *   2. src-tauri/tauri.conf.json → version（决定安装包命名与 about 内嵌版本）
 *   3. src-tauri/Cargo.toml    → [package] version
 *
 * 用法：node scripts/sync-version.mjs <version>   （如 0.2.0，不带 v 前缀）
 */
import { readFileSync, writeFileSync } from 'node:fs'

const version = process.argv[2]?.trim().replace(/^v/i, '')
if (!version || !/^\d+\.\d+\.\d+(-[\w.]+)?$/.test(version)) {
  console.error(`无效版本号: ${process.argv[2] ?? '(缺失)'}，期望形如 1.2.3 或 1.2.3-beta.1`)
  process.exit(1)
}

// 1. package.json
const pkgPath = 'package.json'
const pkg = JSON.parse(readFileSync(pkgPath, 'utf8'))
pkg.version = version
writeFileSync(pkgPath, JSON.stringify(pkg, null, 2) + '\n')

// 2. tauri.conf.json
const confPath = 'src-tauri/tauri.conf.json'
const conf = JSON.parse(readFileSync(confPath, 'utf8'))
conf.version = version
writeFileSync(confPath, JSON.stringify(conf, null, 2) + '\n')

// 3. Cargo.toml（仅替换 [package] 段的第一个 version）
const cargoPath = 'src-tauri/Cargo.toml'
let cargo = readFileSync(cargoPath, 'utf8')
if (!/^version\s*=\s*"[^"]*"/m.test(cargo)) {
  console.error('未能在 Cargo.toml 中找到 version 字段')
  process.exit(1)
}
const current = cargo.match(/^version\s*=\s*"([^"]*)"/m)[1]
cargo = cargo.replace(/^version\s*=\s*"[^"]*"/m, `version = "${version}"`)
writeFileSync(cargoPath, cargo)
if (current === version) {
  console.log(`Cargo.toml 已是 ${version}，无需修改`)
}

console.log(`版本已同步为 ${version}（package.json / tauri.conf.json / Cargo.toml）`)
