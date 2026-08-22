import { pinyin } from 'pinyin-pro'

// 缩拼：汉字取拼音首字母（不带声调、不管多音字），字母/数字转小写，其它符号跳过；最多 20 位。
export function genAbbr(name = '') {
  let out = ''
  for (const ch of String(name)) {
    if (out.length >= 20) break
    if (/[a-zA-Z0-9]/.test(ch)) out += ch.toLowerCase()
    else if (/[一-龥]/.test(ch)) out += pinyin(ch, { pattern: 'first', toneType: 'none' })
  }
  return out
}
