/**
 * 公钥格式转换：OpenSSH 单行公钥 → RFC4716 (IETF SECSH) 多行格式
 * @param line  "ssh-rsa AAAAB3... [comment]" 或 "type blob"
 */
export function toRfc4716(line: string, comment: string): string {
  const parts = line.trim().split(/\s+/)
  const b64 = parts[1] ?? parts[0] ?? ''
  const wrapped = b64.match(/.{1,72}/g) ?? []
  let out = '---- BEGIN SSH2 PUBLIC KEY ----\n'
  if (comment) out += `Comment: "${comment}"\n`
  out += wrapped.join('\n') + '\n'
  out += '---- END SSH2 PUBLIC KEY ----\n'
  return out
}
