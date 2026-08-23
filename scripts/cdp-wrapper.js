/**
 * CDP wrapper — 自动启动 Chrome CDP，提供 nav(url) / safe(js) / sleep(ms) 全局函数
 * 用法: node -r ./cdp-wrapper.js test-crawl.js
 */
const http = require('http');
const WebSocket = require('ws');
const { execSync, spawn } = require('child_process');
const fs = require('fs');
const path = require('path');

const CDP_PORT = 9222;
let ws = null;
let msgId = 0;
let pending = new Map();

function cdp(method, params = {}) {
  return new Promise((resolve, reject) => {
    const id = ++msgId;
    pending.set(id, { resolve, reject });
    ws.send(JSON.stringify({ id, method, params }));
    setTimeout(() => { if (pending.has(id)) { pending.delete(id); reject(new Error('CDP timeout')); } }, 10000);
  });
}

function onMessage(data) {
  try {
    const msg = JSON.parse(data.toString());
    if (msg.id && pending.has(msg.id)) {
      const { resolve, reject } = pending.get(msg.id);
      pending.delete(msg.id);
      if (msg.error) reject(new Error(msg.error.message));
      else resolve(msg.result);
    }
  } catch {}
}

/** 检测 CDP 是否已就绪 */
function cdpReady() {
  return new Promise(resolve => {
    const req = http.get(`http://localhost:${CDP_PORT}/json/version`, res => {
      let d = '';
      res.on('data', c => d += c);
      res.on('end', () => resolve(true));
    });
    req.on('error', () => resolve(false));
    req.setTimeout(2000, () => { req.destroy(); resolve(false); });
  });
}

/** 自动查找 Chrome 并启动 CDP */
function launchChrome() {
  const candidates = [
    'C:\\Program Files\\Google\\Chrome Dev\\Application\\chrome.exe',
    'C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe',
    'C:\\Program Files (x86)\\Google\\Chrome\\Application\\chrome.exe',
    process.env.LOCALAPPDATA + '\\Google\\Chrome\\Application\\chrome.exe',
    process.env.LOCALAPPDATA + '\\Google\\Chrome Dev\\Application\\chrome.exe',
  ];

  let chromePath = null;
  for (const p of candidates) {
    if (fs.existsSync(p)) { chromePath = p; break; }
  }

  if (!chromePath) {
    try {
      chromePath = execSync('where chrome 2>nul', { encoding: 'utf8' }).split('\n')[0].trim();
    } catch {}
  }

  if (!chromePath) {
    console.error('  ✗ 未找到 Chrome，请手动启动: chrome --remote-debugging-port=9222');
    return false;
  }

  console.error(`  🚀 启动 Chrome: ${path.basename(chromePath)}`);

  // 杀旧 Chrome（仅杀当前用户会话的）
  try { execSync('taskkill /F /IM chrome.exe 2>nul', { stdio: 'pipe' }); } catch {}
  // 等进程退出
  const waitUntil = Date.now() + 3000;
  while (Date.now() < waitUntil) {
    try {
      execSync('tasklist /FI "IMAGENAME eq chrome.exe" 2>nul | findstr chrome >nul', { stdio: 'pipe' });
      // still running
      require('child_process').execSync('timeout /t 1 /nobreak >nul', { stdio: 'pipe' });
    } catch {
      break; // no more chrome processes
    }
  }

  // 清理残留目录，启动新 Chrome
  const userDataDir = require('os').tmpdir() + '\\chrome-cdp-test';
  try { fs.rmSync(userDataDir, { recursive: true, force: true }); } catch {}
  spawn(chromePath, [
    `--remote-debugging-port=${CDP_PORT}`,
    '--no-first-run',
    `--user-data-dir=${userDataDir}`,
  ], { detached: true, stdio: 'ignore' });

  return true;
}

async function connectCDP(targetUrl) {
  // 如果 CDP 未就绪，自动启动 Chrome
  if (!(await cdpReady())) {
    if (!launchChrome()) {
      console.error('  ✗ 无法启动 Chrome CDP');
      process.exit(1);
    }
    // 等待 CDP 就绪（最多 15 秒）
    console.error('  ⏳ 等待 CDP 就绪...');
    for (let i = 0; i < 15; i++) {
      await new Promise(r => setTimeout(r, 1000));
      if (await cdpReady()) { console.error('  ✅ CDP 就绪'); break; }
      if (i === 14) { console.error('  ✗ CDP 启动超时'); process.exit(1); }
    }
  }

  const tabs = await new Promise((resolve, reject) => {
    http.get(`http://localhost:${CDP_PORT}/json`, res => {
      let d = '';
      res.on('data', c => d += c);
      res.on('end', () => resolve(JSON.parse(d)));
    }).on('error', reject);
  });

  let target = tabs.find(t => t.type === 'page');
  if (!target) {
    target = await new Promise((resolve, reject) => {
      http.get(`http://localhost:${CDP_PORT}/json/new?${encodeURIComponent(targetUrl || 'about:blank')}`, res => {
        let d = '';
        res.on('data', c => d += c);
        res.on('end', () => resolve(JSON.parse(d)));
      }).on('error', reject);
    });
  }

  ws = new WebSocket(target.webSocketDebuggerUrl);
  await new Promise((resolve, reject) => {
    ws.on('open', resolve);
    ws.on('error', reject);
  });
  ws.on('message', onMessage);
  ws.on('close', () => { ws = null; });
  await cdp('Runtime.enable');
  await resizeWindow(1024, 730);
}

/** 调整浏览器窗口大小（截图脚本可随时调用，如切移动端尺寸） */
global.resizeWindow = async function (width, height) {
  if (!ws) await connectCDP();
  try {
    const { windowId } = await cdp('Browser.getWindowForTarget');
    await cdp('Browser.setWindowBounds', { windowId, bounds: { width, height } });
    console.error(`  📐 窗口已设为 ${width}x${height}`);
  } catch (e) {
    console.error(`  ⚠️ 窗口大小设置失败 (${width}x${height}):`, e.message);
  }
};

let connecting = null;
async function ensureCDP() {
  if (ws && ws.readyState === WebSocket.OPEN) return;
  if (connecting) return connecting;
  connecting = connectCDP();
  try { await connecting; } finally { connecting = null; }
}
global.nav = async function(url) {
  await ensureCDP();
  await cdp('Page.navigate', { url });
};

global.safe = async function(js) {
  if (!ws) await connectCDP();
  try {
    const result = await cdp('Runtime.evaluate', {
      expression: js,
      returnByValue: true,
      awaitPromise: true,
    });
    if (result.exceptionDetails) {
      return result.exceptionDetails.text || 'CDP eval error';
    }
    return result.result?.value;
  } catch (e) {
    return 'CDP error: ' + e.message;
  }
};

global.sleep = (ms) => new Promise(r => setTimeout(r, ms));

// Expose raw CDP call for trusted input events
global.cdpRaw = cdp;
global.waitCDP = ensureCDP;

// 自动连接（等待就绪）
ensureCDP().then(() => { console.error('  ✅ CDP 窗口已设置 1000x618'); }).catch(() => {});