/** 输入合法性校验：主机（IPv4 / IPv6 / 域名 分派式校验）、端口、整数范围 */

export type HostKind = 'ipv4' | 'ipv6' | 'domain' | 'invalid'

/**
 * 主机格式分派校验（第一层分派 + 浏览器引擎裁决）：
 * 1) 点分十进制 → IPv4 严格校验（每段 0–255）
 * 2) 含冒号或 [方括号] → IPv6，交给 WHATWG URL 解析器权威裁决
 *    （完整支持 :: 压缩、::ffff:1.2.3.4 混合写法，免维护复杂正则）
 * 3) 其余 → 域名（宽松：允许内网短主机名、下划线、中文域名；
 *    字符集/结构做快速路径，URL 解析器做 IDNA 与结构最终裁决）
 * 可达性（DNS/在线状态）不属于输入校验范畴，连接时自然报错。
 */
export function detectHostKind(raw: string | number): HostKind {
  const s = String(raw ?? '').trim().replace(/^\[/, '').replace(/\]$/, '')
  if (!s || s.length > 253) return 'invalid'

  // 1) IPv4：点分十进制
  if (/^\d{1,3}(\.\d{1,3}){3}$/.test(s)) {
    return s.split('.').every((o) => +o <= 255) ? 'ipv4' : 'invalid'
  }

  // 2) IPv6：含冒号（或方括号形式已剥离）
  if (s.includes(':')) {
    try {
      return new URL(`http://[${s}]/`).hostname.includes(':') ? 'ipv6' : 'invalid'
    } catch {
      return 'invalid'
    }
  }

  // 3) 域名：字符集/结构快速路径（允许内网短名、下划线、结尾根点）
  if (!/^[A-Za-z0-9_](?:[A-Za-z0-9_-]*[A-Za-z0-9_])?(?:\.[A-Za-z0-9_](?:[A-Za-z0-9_-]*[A-Za-z0-9_])?)*\.?$/.test(s)) {
    return 'invalid'
  }
  try {
    // URL 解析器做 IDNA（中文域名）与结构最终裁决；
    // 注意：WHATWG 对"结尾是数字"的主机名会尝试按 IPv4 解析（如 1.2.3.4.5 → 失败），符合预期
    return new URL(`http://${s}/`).hostname ? 'domain' : 'invalid'
  } catch {
    return 'invalid'
  }
}

export function isValidHost(raw: string | number): boolean {
  return detectHostKind(raw) !== 'invalid'
}

/** 端口：1–65535 整数 */
export function isValidPort(v: string | number): boolean {
  const n = Number(v)
  return Number.isInteger(n) && n >= 1 && n <= 65535
}

/** 整数范围 */
export function isValidInt(v: string | number, min: number, max: number): boolean {
  const n = Number(v)
  return Number.isInteger(n) && n >= min && n <= max
}
