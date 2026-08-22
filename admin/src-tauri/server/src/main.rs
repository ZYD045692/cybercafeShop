//! 无界面开发/调试用入口：直接跑 HTTP 服务。
//! 正式版由 Tauri 壳内嵌本库启动服务，此 bin 仅用于开发与排查。

use cybercafe_shop::config::{AppDirs, Config};
use cybercafe_shop::server;

#[tokio::main]
async fn main() {
    let dirs = AppDirs::from_env();
    let cfg = Config::load(&dirs.base);
    let st = server::build_state(dirs).expect("初始化失败");
    // 生产环境永不主动关闭
    let (_tx, rx) = tokio::sync::oneshot::channel::<()>();
    println!("cybercafeShop-server 监听 :{}（Ctrl+C 退出）", cfg.port);
    if let Err(e) = server::run(st, cfg.port, rx).await {
        eprintln!("服务退出: {e}");
    }
}
