//! 管理端 Tauri 壳：
//! - 单实例（参考 Landisk：第二次启动只唤醒已有窗口）
//! - 点 X = 收起到托盘（不退出）；托盘左键单击 = 重新打开；右键菜单可真正退出
//! - 内嵌 HTTP 服务（cybercafeShop-server 库），订单/呼叫事件广播转发到前端窗口
//! - 开发模式数据目录隔离到 dev-data/，与生产环境隔开（参考 Landisk dev-data）

use std::path::PathBuf;
use std::sync::Arc;

use tauri::menu::{MenuBuilder, MenuItemBuilder};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{Emitter, Manager, WindowEvent};
use tauri_plugin_autostart::ManagerExt;

use cybercafe_shop::config::{AppDirs, Config};
use cybercafe_shop::server::{self, AppState};

struct SharedState {
    #[allow(dead_code)]
    server: Arc<AppState>,
    port: u16,
}

/// 数据目录：dev 模式用项目根 dev-data/（隔离生产），打包后用 exe 所在目录。
fn data_dir() -> AppDirs {
    if cfg!(debug_assertions) {
        let base = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../dev-data");
        let _ = std::fs::create_dir_all(&base);
        AppDirs::new(base)
    } else {
        AppDirs::from_env()
    }
}

/// 首启播种（仅生产）：安装包里的 seed\ 是初始数据，数据目录缺什么补什么。
/// config.ini 不存在则写默认（只含端口）。
#[cfg(not(debug_assertions))]
fn seed_if_missing(base: &std::path::Path) {
    let seed = base.join("seed");
    for d in ["data/db", "data/image", "data/qrcode", "data/sound", "web/m"] {
        let dst = base.join(d);
        let empty = std::fs::read_dir(&dst).map(|mut i| i.next().is_none()).unwrap_or(true);
        if empty {
            let src = seed.join(d);
            if src.is_dir() {
                let _ = std::fs::create_dir_all(&dst);
                copy_dir(&src, &dst);
            }
        }
    }
    let cfg = base.join("config.ini");
    if !cfg.exists() {
        let _ = std::fs::write(&cfg, "; 管理端配置\n; port: HTTP 服务端口（用户端 config.ini 里填同一个），改完重启管理端生效\n[server]\nport = 21974\r\n");
    }
}

#[cfg(not(debug_assertions))]
fn copy_dir(src: &std::path::Path, dst: &std::path::Path) {
    if let Ok(entries) = std::fs::read_dir(src) {
        for e in entries.flatten() {
            let (s, d) = (e.path(), dst.join(e.file_name()));
            if s.is_dir() {
                let _ = std::fs::create_dir_all(&d);
                copy_dir(&s, &d);
            } else if !d.exists() {
                let _ = std::fs::copy(&s, &d);
            }
        }
    }
}

/// 通知窗口的当前卡片区高度（逻辑像素），供鼠标穿透判定用
struct NotifyCtl {
    zone_h: std::sync::Mutex<f64>,
}

// 卡片圆角由前端 CSS 实现（WebView2 抗锯齿，边缘平滑）。
// 窗口本身不再 SetWindowRgn 裁剪，也不透明：改为不透明窗口 + CSS 圆角 + 圆角外同色深底，
// 四角由 .deck 的圆角背景填实，杜绝透明/灰缝。

/// 前端量好卡片实际高度后调用：调整窗口大小、摆到右下角、显示/隐藏。
/// 窗口有多大内容就占多大，不占多余桌面。
#[tauri::command]
fn notify_sync(app: tauri::AppHandle, height: f64) {
    let Some(w) = app.get_webview_window("notify") else { return };
    if height <= 1.0 {
        *app.state::<NotifyCtl>().zone_h.lock().unwrap() = 0.0;
        let _ = w.hide();
        return;
    }
    let _ = w.set_size(tauri::LogicalSize::new(376.0, height));
    // 先算好位置再 show（避免闪现）；显示器信息取不到时保持原位置也要保证可见
    let mon = w
        .current_monitor()
        .ok()
        .flatten()
        .or_else(|| w.primary_monitor().ok().flatten());
    if let Some(m) = mon {
        let size = m.size();
        let scale = m.scale_factor();
        let logical_w = size.width as f64 / scale;
        let logical_h = size.height as f64 / scale;
        let _ = w.set_position(tauri::LogicalPosition::new(
            (logical_w - 388.0).max(0.0),
            // 底部留 60px 避开 Windows 任务栏，卡片不贴屏幕底边
            (logical_h - height - 60.0).max(0.0),
        ));
    }
    *app.state::<NotifyCtl>().zone_h.lock().unwrap() = height;
    let _ = w.show();
}

/// 通知窗口透明区域鼠标穿透（参考 WPF 弹幕软件的做法）：
/// 轮询全局光标位置，光标在卡片区内 = 可点击，在透明区 = 直接点穿到桌面。
/// 不能用 set_ignore_cursor_events 一次了事——开了穿透网页就收不到鼠标事件，
/// 光标回到卡片上时无法自动恢复，所以用全局轮询来切换。
#[cfg(windows)]
fn spawn_passthrough_polling(app: tauri::AppHandle) {
    std::thread::spawn(move || {
        // 状态去抖：只在「可交互/穿透」状态变化时才调 Win32 切换，不每帧刷
        let mut interactive: Option<bool> = None;
        loop {
            std::thread::sleep(std::time::Duration::from_millis(50));
            let Some(w) = app.get_webview_window("notify") else { continue };
            let zone = *app.state::<NotifyCtl>().zone_h.lock().unwrap();
            if !w.is_visible().unwrap_or(false) || zone <= 0.0 {
                // 窗口隐藏时兜底复位为可交互，防止穿透状态卡死
                if interactive != Some(true) {
                    let _ = w.set_ignore_cursor_events(false);
                    interactive = Some(true);
                }
                continue;
            }
            let cursor = unsafe {
                let mut p = winapi::shared::windef::POINT { x: 0, y: 0 };
                winapi::um::winuser::GetCursorPos(&mut p);
                p
            };
            let (Ok(pos), Ok(size)) = (w.outer_position(), w.outer_size()) else { continue };
            let scale = w.scale_factor().unwrap_or(1.0);
            let in_x = cursor.x >= pos.x && cursor.x < pos.x + size.width as i32;
            // 卡片钉在窗口底部，卡片区 = 窗口底部向上 zone 高的区域
            let card_top = pos.y + size.height as i32 - (zone * scale) as i32;
            let in_zone = in_x && cursor.y >= card_top && cursor.y < pos.y + size.height as i32;
            if interactive != Some(in_zone) {
                let _ = w.set_ignore_cursor_events(!in_zone);
                interactive = Some(in_zone);
            }
        }
    });
}

#[cfg(not(windows))]
fn spawn_passthrough_polling(_app: tauri::AppHandle) {}

#[tauri::command]
fn test_announce(state: tauri::State<SharedState>) {
    state.server.announcer.announce("PC-08", cybercafe_shop::announce::Kind::Order);
}

/// 设置页「测试提醒弹窗」：往事件总线发一条假呼叫，走完整链路（广播→转发→通知卡片）
#[tauri::command]
fn test_notify(state: tauri::State<SharedState>) {
    let _ = state
        .server
        .events
        .send(serde_json::json!({ "type": "call", "machine": "PC-08" }));
}

#[tauri::command]
fn get_autostart(app: tauri::AppHandle) -> bool {
    app.autolaunch().is_enabled().unwrap_or(false)
}

#[tauri::command]
fn set_autostart(app: tauri::AppHandle, enabled: bool) {
    let al = app.autolaunch();
    let _ = if enabled { al.enable() } else { al.disable() };
}

#[tauri::command]
fn get_port(state: tauri::State<SharedState>) -> u16 {
    state.port
}

pub fn run() {
    tauri::Builder::default()
        // 单实例：第二次启动只把已有主窗口唤到前台，新进程直接退出
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.unminimize();
                let _ = w.show();
                let _ = w.set_focus();
            }
        }))
        .plugin(tauri_plugin_autostart::Builder::new().arg("--hidden").build())
        .setup(|app| {
            let dirs = data_dir();
            #[cfg(not(debug_assertions))]
            seed_if_missing(&dirs.base);
            let cfg = Config::load(&dirs.base);
            let port = cfg.port;
            // 生产开门禁（本机免票/外网卡验 HMAC 时间票）；dev 关闭方便调试
            let mode = if cfg!(debug_assertions) {
                cybercafe_shop::auth::AuthMode::Off
            } else {
                cybercafe_shop::auth::AuthMode::TicketOrLocalhost
            };
            let state = server::build_state_with(dirs, mode).expect("初始化服务状态失败");

            // 内嵌启动 HTTP 服务（独立线程跑 tokio runtime）
            let st = state.clone();
            std::thread::spawn(move || {
                let rt = tokio::runtime::Runtime::new().unwrap();
                rt.block_on(async move {
                    let (_tx, rx) = tokio::sync::oneshot::channel::<()>();
                    if let Err(e) = server::run(st, port, rx).await {
                        eprintln!("[服务] 退出: {e}");
                    }
                });
            });

            // 事件转发线程：服务广播 → 前端窗口
            let mut rx = state.events.subscribe();
            let app_handle = app.handle().clone();
            std::thread::spawn(move || {
                let rt = tokio::runtime::Runtime::new().unwrap();
                rt.block_on(async move {
                    loop {
                        match rx.recv().await {
                            Ok(ev) => {
                                let _ = app_handle.emit_to("notify", "tf-event", ev.clone());
                                let _ = app_handle.emit_to("main", "tf-event", ev);
                                // 通知窗口的显示/尺寸由 notify 页面前端量好内容后调 notify_sync 完成
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                            Err(_) => break,
                        }
                    }
                });
            });

            // 开机自启被拉起时（--hidden）不显示主窗口
            let is_autostart = std::env::args().any(|a| a == "--hidden");
            // KEY 注入页面内存：管理端页面签名 /image /qrcode 请求用（密钥不进任何静态文件）
            let key = std::str::from_utf8(cybercafe_shop::auth::ACCESS_KEY).unwrap_or("");
            let init_js = format!("window.__LSWSHOP_PORT__={port};window.__LSWSHOP_KEY__='{key}';");
            let _main = tauri::WebviewWindowBuilder::new(app, "main", tauri::WebviewUrl::App("index.html".into()))
                .title("莱尚网电竞馆 · 点购管理端")
                .inner_size(1280.0, 800.0)
                .min_inner_size(1024.0, 640.0)
                .center()
                .visible(!is_autostart)
                .focused(!is_autostart)
                .initialization_script(&init_js)
                .build()?;

            // 通知窗口（无边框、置顶、不抢焦点），初始隐藏
            let _notify = tauri::WebviewWindowBuilder::new(app, "notify", tauri::WebviewUrl::App("notify.html".into()))
                .title("订单提醒")
                .inner_size(376.0, 10.0)
                .resizable(false)
                .decorations(false)
                .transparent(false) // 不透明：四角由 .deck 圆角深色背景填实，避免透明/灰缝
                .always_on_top(true)
                .skip_taskbar(true)
                .visible(false)
                .focusable(false)
                .initialization_script(&init_js)
                .build()?;

            app.manage(SharedState { server: state, port });
            app.manage(NotifyCtl { zone_h: std::sync::Mutex::new(0.0) });
            spawn_passthrough_polling(app.handle().clone());

            // 托盘：左键单击=打开主窗口；右键菜单=显示/退出
            let show_item = MenuItemBuilder::with_id("show", "显示主窗口").build(app)?;
            let quit_item = MenuItemBuilder::with_id("quit", "退出").build(app)?;
            let menu = MenuBuilder::new(app).item(&show_item).separator().item(&quit_item).build()?;
            let icon = app.default_window_icon().unwrap().clone();
            TrayIconBuilder::new()
                .icon(icon)
                .tooltip("莱尚网电竞馆 · 点购管理端")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id().as_ref() {
                    "show" => {
                        if let Some(w) = app.get_webview_window("main") {
                            let _ = w.unminimize();
                            let _ = w.show();
                            let _ = w.set_focus();
                        }
                    }
                    "quit" => app.exit(0), // 真正退出（HTTP 服务随进程结束）
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        if let Some(w) = tray.app_handle().get_webview_window("main") {
                            let _ = w.unminimize();
                            let _ = w.show();
                            let _ = w.set_focus();
                        }
                    }
                })
                .build(app)?;

            Ok(())
        })
        .on_window_event(|window, event| {
            // 点 X：主窗口收起到托盘（不退出、不最小化到任务栏）
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            notify_sync, test_announce, test_notify, get_autostart, set_autostart, get_port
        ])
        .run(tauri::generate_context!())
        .expect("error running cybercafeShop-admin");
}
