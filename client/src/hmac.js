// 纯 JS SHA-256 + HMAC-SHA256（http 环境下 crypto.subtle 不可用，故自实现）。
// 仅用于 ASCII 输入（时间戳 + 内置密钥都是 ASCII）。sha256 实现为公知公共域版本。

/* eslint-disable */
function sha256(ascii) {
  function rightRotate(value, amount) {
    return (value >>> amount) | (value << (32 - amount))
  }
  var mathPow = Math.pow
  var maxWord = mathPow(2, 32)
  var lengthProperty = 'length'
  var i, j
  var result = ''

  var words = []
  var asciiBitLength = ascii[lengthProperty] * 8

  var hash = (sha256.h = sha256.h || [])
  var k = (sha256.k = sha256.k || [])
  var primeCounter = k[lengthProperty]
  var isComposite = {}
  for (var candidate = 2; primeCounter < 64; candidate++) {
    if (!isComposite[candidate]) {
      for (i = 0; i < 313; i += candidate) {
        isComposite[i] = candidate
      }
      hash[primeCounter] = (mathPow(candidate, 0.5) * maxWord) | 0
      k[primeCounter++] = (mathPow(candidate, 1 / 3) * maxWord) | 0
    }
  }

  ascii += '\x80'
  while ((ascii[lengthProperty] % 64) - 56) ascii += '\x00'
  for (i = 0; i < ascii[lengthProperty]; i++) {
    j = ascii.charCodeAt(i)
    if (j >> 8) throw new Error('sha256: 只支持单字节字符')
    words[i >> 2] |= j << (((3 - i) % 4) * 8)
  }
  words[words[lengthProperty]] = (asciiBitLength / maxWord) | 0
  words[words[lengthProperty]] = asciiBitLength

  for (j = 0; j < words[lengthProperty]; ) {
    var w = words.slice(j, (j += 16))
    var oldHash = hash
    hash = hash.slice(0, 8)

    for (i = 0; i < 64; i++) {
      var w15 = w[i - 15], w2 = w[i - 2]
      var a = hash[0], e = hash[4]
      var temp1 =
        hash[7] +
        (rightRotate(e, 6) ^ rightRotate(e, 11) ^ rightRotate(e, 25)) +
        ((e & hash[5]) ^ (~e & hash[6])) +
        k[i] +
        (w[i] =
          i < 16
            ? w[i]
            : (w[i - 16] +
                (rightRotate(w15, 7) ^ rightRotate(w15, 18) ^ (w15 >>> 3)) +
                w[i - 7] +
                (rightRotate(w2, 17) ^ rightRotate(w2, 19) ^ (w2 >>> 10))) |
              0)
      var temp2 =
        (rightRotate(a, 2) ^ rightRotate(a, 13) ^ rightRotate(a, 22)) +
        ((a & hash[1]) ^ (a & hash[2]) ^ (hash[1] & hash[2]))

      hash = [(temp1 + temp2) | 0].concat(hash)
      hash[4] = (hash[4] + temp1) | 0
    }

    for (i = 0; i < 8; i++) {
      hash[i] = (hash[i] + oldHash[i]) | 0
    }
  }

  for (i = 0; i < 8; i++) {
    for (j = 3; j + 1; j--) {
      var b = (hash[i] >> (j * 8)) & 255
      result += (b < 16 ? '0' : '') + b.toString(16)
    }
  }
  return result
}

function hexToStr(hex) {
  var s = ''
  for (var i = 0; i < hex.length; i += 2) {
    s += String.fromCharCode(parseInt(hex.substr(i, 2), 16))
  }
  return s
}

// HMAC-SHA256(key, message) → hex。key/message 均为单字节字符串。
export function hmacSha256(key, message) {
  var kh = key.length > 64 ? hexToStr(sha256(key)) : key
  var k64 = kh
  while (k64.length < 64) k64 += '\x00'
  var inner = '', outer = ''
  for (var n = 0; n < 64; n++) {
    inner += String.fromCharCode(k64.charCodeAt(n) ^ 0x36)
    outer += String.fromCharCode(k64.charCodeAt(n) ^ 0x5c)
  }
  return sha256(outer + hexToStr(sha256(inner + message)))
}
