//! 门禁（HMAC 时间票）+ 店铺信息 + 缩拼自动生成 + 用户端网页托管 测试。
//! 与 api_adversarial.rs 相同的隔离原则：独立临时目录 + 独立端口。

use serde_json::{json, Value};
use std::net::TcpStream;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use cybercafe_shop::auth::{self, AuthMode};
use cybercafe_shop::config::AppDirs;
use cybercafe_shop::server;

use std::sync::atomic::{AtomicU16, Ordering};
static NEXT_PORT: AtomicU16 = AtomicU16::new(24417);
fn next_port() -> u16 {
    NEXT_PORT.fetch_add(1, Ordering::SeqCst)
}

struct Env {
    _dir: tempfile::TempDir,
    port: u16,
    shutdown: Option<tokio::sync::oneshot::Sender<()>>,
    handle: Option<std::thread::JoinHandle<()>>,
}

/// 指定门禁模式的环境
fn new_env_mode(mode: AuthMode, with_web: bool) -> Env {
    let port = next_port();
    let dir = tempfile::tempdir().unwrap();
    let base: PathBuf = dir.path().to_path_buf();
    let dirs = AppDirs::new(base.clone());
    dirs.ensure_dirs().unwrap();

    // 夹具：分类 + 商品（一个缩拼为空，验证后端不再自动回填/生成，缩拼由前端生成后上传）
    let conn = rusqlite::Connection::open(dirs.db_path()).unwrap();
    conn.execute_batch(
        "CREATE TABLE shop_fl (id INTEGER PRIMARY KEY, class_name TEXT, class_px INTEGER);
         CREATE TABLE shop_list (
           id INTEGER PRIMARY KEY, gds_number TEXT, gds_class TEXT, gds_name TEXT,
           gds_bt_count INTEGER, gds_ck_count INTEGER, gds_jhj REAL, gds_xsj REAL,
           gds_gys TEXT, gds_pic TEXT, gds_px INTEGER, gds_state INTEGER,
           gds_out INTEGER, gds_js TEXT);
         INSERT INTO shop_fl VALUES (1,'饮料',8);
         INSERT INTO shop_list (id,gds_number,gds_class,gds_name,gds_jhj,gds_xsj,gds_gys,gds_pic,gds_px,gds_state,gds_out,gds_js)
           VALUES (1,'','饮料','娃哈哈AD钙奶',1.5,3.0,'默认','',6,1,0,'');",
    )
    .unwrap();
    drop(conn);

    if with_web {
        let shop = dirs.web_dir().join("shop");
        std::fs::create_dir_all(&shop).unwrap();
        std::fs::write(shop.join("index.html"), "<html>shop page</html>").unwrap();
    }

    let state = server::build_state_with(dirs, mode).unwrap();
    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    let st2 = state.clone();
    let handle = std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async move { server::run(st2, port, rx).await.unwrap() });
    });
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if TcpStream::connect(("127.0.0.1", port)).is_ok() {
            break;
        }
        assert!(Instant::now() < deadline, "服务启动超时");
        std::thread::sleep(Duration::from_millis(50));
    }
    Env { _dir: dir, port, shutdown: Some(tx), handle: Some(handle) }
}

/// 严格验票模式（模拟外网卡视角：无票即 403）
fn new_env(with_web: bool) -> Env {
    new_env_mode(AuthMode::Ticket, with_web)
}

impl Drop for Env {
    fn drop(&mut self) {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

fn ticket() -> (String, String) {
    let ts = auth::now_ts();
    (ts.to_string(), auth::sign(ts))
}

fn get_signed(port: u16, path: &str) -> (u16, String) {
    let (ts, sig) = ticket();
    let r = ureq::get(&format!("http://127.0.0.1:{port}{path}"))
        .set("x-ts", &ts)
        .set("x-sig", &sig)
        .call();
    match r {
        Ok(resp) => (resp.status(), resp.into_string().unwrap_or_default()),
        Err(ureq::Error::Status(code, resp)) => (code, resp.into_string().unwrap_or_default()),
        Err(e) => panic!("请求失败: {e}"),
    }
}

fn get_raw(port: u16, path: &str) -> u16 {
    match ureq::get(&format!("http://127.0.0.1:{port}{path}")).call() {
        Ok(resp) => resp.status(),
        Err(ureq::Error::Status(code, _)) => code,
        Err(e) => panic!("请求失败: {e}"),
    }
}

// ---------- 门禁 ----------

#[test]
fn t40_unsigned_requests_rejected() {
    let env = new_env(true);
    // 浏览器裸开：公开 API / 图片 / 收款码 全部 403（用户端网页已内嵌 exe，无 /shop/ 托管）
    assert_eq!(get_raw(env.port, "/api/products"), 403);
    assert_eq!(get_raw(env.port, "/api/shopinfo"), 403);
    assert_eq!(get_raw(env.port, "/image/x.jpg"), 403);
    assert_eq!(get_raw(env.port, "/qrcode/wechat"), 403);
    // ping 不加密（存活探测+对时，无业务数据）
    assert_eq!(get_raw(env.port, "/api/ping"), 200);
}

#[test]
fn t41_signed_requests_allowed() {
    let env = new_env(true);
    let (code, body) = get_signed(env.port, "/api/products");
    assert_eq!(code, 200);
    let v: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(v["ok"], true);
    // 用户端网页已内嵌 exe，无 /shop/ 托管
}

#[test]
fn t42_query_ticket_for_img_tags() {
    let env = new_env(true);
    // <img> 无法带 header → query 参数票据也要认
    let (ts, sig) = ticket();
    assert_eq!(get_raw(env.port, &format!("/image/x.jpg?ts={ts}&sig={sig}")), 404); // 404=过了门禁但文件不存在
}

#[test]
fn t43_expired_or_forged_ticket_rejected() {
    let env = new_env(true);
    let old = auth::now_ts() - 10000; // 超窗
    let sig = auth::sign(old);
    assert_eq!(get_raw(env.port, &format!("/api/products?ts={old}&sig={sig}")), 403);
    let now = auth::now_ts();
    assert_eq!(get_raw(env.port, &format!("/api/products?ts={now}&sig=deadbeef")), 403);
}

#[test]
fn t44_admin_api_still_localhost_guarded() {
    let env = new_env(true);
    // 管理 API 走本机守卫而非门禁：带不带票都能从本机访问（127.0.0.1）
    assert_eq!(get_raw(env.port, "/api/admin/products"), 200);
    assert_eq!(get_raw(env.port, "/api/orders"), 200);
}

#[test]
fn t47_production_mode_loopback_exempt() {
    // 生产模式 TicketOrLocalhost：本机回环免票（管理端页面不带签名直接用），
    // 带票的外网卡也放行
    let env = new_env_mode(AuthMode::TicketOrLocalhost, true);
    assert_eq!(get_raw(env.port, "/api/products"), 200); // 本机无票也放行
    let (code, _) = get_signed(env.port, "/api/products");
    assert_eq!(code, 200);
}

// ---------- 店铺信息 ----------

#[test]
fn t45_shopinfo_defaults_and_update() {
    let env = new_env(true);
    // 默认值
    let (code, body) = get_signed(env.port, "/api/shopinfo");
    assert_eq!(code, 200);
    let v: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(v["shop_name"], "莱尚网电竞馆");
    assert!(v["welcome"].as_str().unwrap().contains("欢迎"));

    // 管理端修改（两个独立项）
    let (ts, sig) = ticket();
    let r = ureq::post(&format!("http://127.0.0.1:{}/api/admin/shopinfo", env.port))
        .set("Content-Type", "application/json")
        .set("x-ts", &ts)
        .set("x-sig", &sig)
        .send_string(&json!({"shop_name":"测试网咖","welcome":"hello"}).to_string())
        .unwrap();
    assert_eq!(r.status(), 200);

    let (_, body) = get_signed(env.port, "/api/shopinfo");
    let v: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(v["shop_name"], "测试网咖");
    assert_eq!(v["welcome"], "hello");

    // 非法：店名为空
    let r = ureq::post(&format!("http://127.0.0.1:{}/api/admin/shopinfo", env.port))
        .set("Content-Type", "application/json")
        .send_string(&json!({"shop_name":"  ","welcome":"x"}).to_string());
    match r {
        Err(ureq::Error::Status(code, _)) => assert_eq!(code, 400),
        _ => panic!("应拒绝空店名"),
    }
}

// ---------- 缩拼透传（前端生成，后端原样存储，不再自动生成/回填） ----------

#[test]
fn t46_abbr_passthrough() {
    let env = new_env(false);
    // 后端不再自动回填：夹具商品 gds_number 为空 → 启动后仍为空
    let (code, body) = get_signed(env.port, "/api/products");
    assert_eq!(code, 200);
    let v: Value = serde_json::from_str(&body).unwrap();
    let abbr = v["products"][0]["abbr"].as_str().unwrap();
    assert_eq!(abbr, "");

    // 缩拼由前端生成后上传：后端原样保存，不再自动生成
    let r = ureq::post(&format!("http://127.0.0.1:{}/api/admin/product", env.port))
        .set("Content-Type", "application/json")
        .send_string(&json!({"name":"可口可乐330ml","class":"饮料","abbr":"kkkl330ml","price":3.0}).to_string())
        .unwrap();
    assert_eq!(r.status(), 200);
    let (_, body) = get_signed(env.port, "/api/products");
    let v: Value = serde_json::from_str(&body).unwrap();
    let list = v["products"].as_array().unwrap();
    let coke = list.iter().find(|p| p["name"] == "可口可乐330ml").unwrap();
    assert_eq!(coke["abbr"], "kkkl330ml");
}
