//! 用户端 Tauri 壳（薄壳：窗口 + 配置注入，业务全在网页里）：
//! - 启动读 exe 同级 config.ini（管理端 IP/端口/网管联系方式）
//! - 启动先探测管理端（/api/ping，HMAC 签名），连不上弹 Windows 系统警告框并用 Tauri 优雅退出；
//!   连上后对时（记录服务器时间偏移），带时间票加载吧台托管的网页 http://host:port/shop/
//! - 网页不在本机：吧台更新 web\shop\ 后所有客户机自动用新界面
//! - 机台名取设备名称（COMPUTERNAME），下单/呼叫时带给管理端
//! - 单实例（重复启动只唤醒已有窗口）；点 X 直接退出，不进托盘，不影响顾客打游戏

use hmac::{Hmac, Mac};
use sha2::Sha256;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::Manager;

/// 访问密钥：与管理端 server/src/auth.rs 里的 ACCESS_KEY 保持一致。
/// 换密钥 = 两边一起改、重新编译两端。
const ACCESS_KEY: &[u8] = b"cybercafeShop-6f2a9c4e8b1d4a7f-k53p9q2w7e0r6t5y8";

fn now_ts() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn sign(ts: i64) -> String {
    let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(ACCESS_KEY).expect("HMAC 初始化失败");
    mac.update(ts.to_string().as_bytes());
    hex::encode(mac.finalize().into_bytes())
}

/// 读 config.ini 里的键（极简解析）
fn ini_get(text: &str, section: &str, key: &str) -> Option<String> {
    let mut in_sec = false;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            in_sec = line[1..line.len() - 1].trim() == section;
            continue;
        }
        if in_sec {
            if let Some((k, v)) = line.split_once('=') {
                if k.trim() == key {
                    return Some(v.trim().to_string());
                }
            }
        }
    }
    None
}

/// 探测管理端：GET /api/ping（带签名），成功返回服务器时间戳。
fn ping_server(host: &str, port: u16) -> Option<i64> {
    use std::io::{Read, Write};
    use std::net::ToSocketAddrs;
    let addr = (host, port).to_socket_addrs().ok()?.next()?;
    let ts = now_ts();
    let sig = sign(ts);
    let mut s = std::net::TcpStream::connect_timeout(&addr, Duration::from_secs(3)).ok()?;
    s.set_read_timeout(Some(Duration::from_secs(3))).ok()?;
    s.set_write_timeout(Some(Duration::from_secs(3))).ok()?;
    let req = format!(
        "GET /api/ping HTTP/1.1\r\nHost: {host}\r\nx-ts: {ts}\r\nx-sig: {sig}\r\nConnection: close\r\n\r\n"
    );
    s.write_all(req.as_bytes()).ok()?;
    let mut buf = Vec::new();
    s.read_to_end(&mut buf).ok()?;
    let text = String::from_utf8_lossy(&buf);
    if !text.starts_with("HTTP/1.1 200") && !text.starts_with("HTTP/1.0 200") {
        return None;
    }
    let pos = text.find("\"time\":")?;
    let num: String = text[pos + 7..]
        .chars()
        .skip_while(|c| !c.is_ascii_digit())
        .take_while(|c| c.is_ascii_digit())
        .collect();
    num.parse().ok()
}

/// Windows 系统警告框（连不上管理端时用）：置顶模态，点确定返回。
#[cfg(windows)]
fn fatal_popup(msg: &str) {
    let to_wide = |s: &str| -> Vec<u16> { s.encode_utf16().chain(Some(0)).collect() };
    unsafe {
        winapi::um::winuser::MessageBoxW(
            std::ptr::null_mut(),
            to_wide(msg).as_ptr(),
            to_wide("莱尚网电竞馆 · 商品点购").as_ptr(),
            winapi::um::winuser::MB_OK
                | winapi::um::winuser::MB_ICONWARNING
                | winapi::um::winuser::MB_SYSTEMMODAL
                | winapi::um::winuser::MB_SETFOREGROUND,
        );
    }
}

#[cfg(not(windows))]
fn fatal_popup(msg: &str) {
    eprintln!("[fatal] {msg}");
}

pub fn run() {
    // 配置：打包后读 exe 同级 config.ini；dev 模式与管理端一致，读项目根 dev-data/config.ini
    // （开发数据全在 dev-data 里改，与生产隔开）。都找不到时默认连本机 127.0.0.1。
    let cfg_path = if cfg!(debug_assertions) {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../dev-data/config.ini")
    } else {
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.to_path_buf()))
            .unwrap_or_else(|| PathBuf::from("."))
            .join("config.ini")
    };
    let text = std::fs::read_to_string(&cfg_path).unwrap_or_default();
    let host = ini_get(&text, "server", "host").unwrap_or_else(|| "127.0.0.1".into());
    let port = ini_get(&text, "server", "port")
        .and_then(|p| p.parse::<u16>().ok())
        .unwrap_or(21974);
    let contact = ini_get(&text, "server", "contact").unwrap_or_else(|| "吧台网管".into());

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.show();
                let _ = w.set_focus();
            }
        }))
        .setup(move |app| {
            // 启动先探测管理端（dev 与生产一致）：连不上 → 系统警告框 → Tauri 优雅退出，不进界面。
            // 连上则记录服务器时间偏移（客户机时钟可能不准，签名要用服务器时间）。
            let offset: i64;
            let mut server_time = None;
            for _ in 0..2 {
                if let Some(t) = ping_server(&host, port) {
                    server_time = Some(t);
                    break;
                }
                std::thread::sleep(Duration::from_secs(1));
            }
            match server_time {
                Some(t) => offset = t - now_ts(),
                None => {
                    fatal_popup(&format!(
                        "点购系统暂时无法使用\n\n有购买需求请到吧台\n\n祝你生活愉快\n\n网管电话：{contact}"
                    ));
                    app.handle().exit(1); // 走 Tauri 生命周期优雅退出，避免 std::process::exit 硬退
                    return Ok(());
                }
            }

            // 机台名用设备名称（COMPUTERNAME，播报用）；取不到退化为用户名
            let machine = std::env::var("COMPUTERNAME")
                .or_else(|_| std::env::var("USERNAME"))
                .unwrap_or_else(|_| "unknown".into());

            // 转义，防止机器名里带特殊字符破坏 JS 字符串
            let esc = |s: &str| s.replace('\\', "\\\\").replace('\'', "\\'");
            // KEY/OFFSET 注入页面内存：页面 JS 给后续 API/图片请求签名用（密钥不进任何静态文件）
            let init_js = format!(
                "window.__HOST__='{}';window.__PORT__={};window.__MACHINE__='{}';window.__KEY__='{}';window.__OFFSET__={};",
                esc(&host),
                port,
                esc(&machine),
                esc(std::str::from_utf8(ACCESS_KEY).unwrap_or("")),
                offset
            );

            // 页面内嵌进 exe（与生产/开发一致），不再加载管理端托管的 /shop/ 网页
            let page_url = tauri::WebviewUrl::App("index.html".into());

            tauri::WebviewWindowBuilder::new(app, "main", page_url)
                .title("莱尚网电竞馆 · 商品点购")
                .inner_size(1280.0, 800.0)
                .min_inner_size(1024.0, 640.0)
                .center()
                .decorations(false) // 无边框：用页面内自绘的深色标题栏（拖拽 + 最小化/最大化/关闭）
                .initialization_script(&init_js)
                .build()?;
            Ok(())
        })
        // 不做托盘：关闭即退出（默认行为），不占客户机资源
        .run(tauri::generate_context!())
        .expect("error running cybercafeShop-client");
}
