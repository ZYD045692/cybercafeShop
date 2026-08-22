//! 手机端 API（/api/m/*）与 hostinfo 的端到端测试。
//! 编号接 api_adversarial(t01-t37)/auth_shopinfo(t40-t47)，从 t50 开始。

use serde_json::{json, Value};
use std::net::{Ipv4Addr, TcpStream};
use std::sync::atomic::{AtomicU16, Ordering};
use std::time::{Duration, Instant};

use cybercafe_shop::config::AppDirs;
use cybercafe_shop::server;

static NEXT_PORT: AtomicU16 = AtomicU16::new(24917);

fn next_port() -> u16 {
    NEXT_PORT.fetch_add(1, Ordering::SeqCst)
}

struct TestEnv {
    _dir: tempfile::TempDir,
    base: std::path::PathBuf,
    port: u16,
    shutdown: Option<tokio::sync::oneshot::Sender<()>>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl TestEnv {
    fn new() -> TestEnv {
        let port = next_port();
        let dir = tempfile::tempdir().expect("创建临时目录失败");
        let base = dir.path().to_path_buf();
        let dirs = AppDirs::new(base.clone());
        dirs.ensure_dirs().unwrap();
        std::fs::write(base.join("config.ini"), format!("[server]\nport={port}\n")).unwrap();

        // 夹具数据：一个分类 + 一个在售商品（缩拼 bskl）
        let conn = rusqlite::Connection::open(dirs.db_path()).unwrap();
        conn.execute_batch(
            "CREATE TABLE shop_fl (id INTEGER PRIMARY KEY, class_name TEXT, class_px INTEGER);
             CREATE TABLE shop_list (
               id INTEGER PRIMARY KEY, gds_number TEXT, gds_class TEXT, gds_name TEXT,
               gds_bt_count INTEGER, gds_ck_count INTEGER, gds_jhj REAL, gds_xsj REAL,
               gds_gys TEXT, gds_pic TEXT, gds_px INTEGER, gds_state INTEGER,
               gds_out INTEGER, gds_js TEXT);",
        )
        .unwrap();
        conn.execute("INSERT INTO shop_fl VALUES (1,'饮料',8)", []).unwrap();
        conn.execute(
            "INSERT INTO shop_list (id,gds_number,gds_class,gds_name,gds_jhj,gds_xsj,gds_gys,gds_pic,gds_px,gds_state,gds_out,gds_js)
             VALUES (1,'bskl','饮料','测试可乐',2.5,3.5,'默认','bskl.jpg',6,1,100,'')",
            [],
        )
        .unwrap();
        drop(conn);

        let state = server::build_state(dirs).expect("build_state 失败");
        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        let st2 = state.clone();
        let handle = std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async move {
                server::run(st2, port, rx).await.expect("服务运行失败");
            });
        });

        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if TcpStream::connect(("127.0.0.1", port)).is_ok() {
                break;
            }
            assert!(Instant::now() < deadline, "服务启动超时");
            std::thread::sleep(Duration::from_millis(50));
        }

        TestEnv { _dir: dir, base, port, shutdown: Some(tx), handle: Some(handle) }
    }

    fn url(&self, path: &str) -> String {
        format!("http://127.0.0.1:{}{}", self.port, path)
    }
}

impl Drop for TestEnv {
    fn drop(&mut self) {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

fn unwrap_resp(r: Result<ureq::Response, ureq::Error>) -> (u16, Value) {
    match r {
        Ok(resp) => {
            let code = resp.status();
            let body: Value = serde_json::from_str(&resp.into_string().unwrap_or_default()).unwrap_or(json!({}));
            (code, body)
        }
        Err(ureq::Error::Status(code, resp)) => {
            let body: Value = serde_json::from_str(&resp.into_string().unwrap_or_default()).unwrap_or(json!({}));
            (code, body)
        }
        Err(e) => panic!("请求失败: {e}"),
    }
}

fn get(url: &str) -> (u16, Value) {
    unwrap_resp(ureq::get(url).call())
}

fn post_json(url: &str, body: Value) -> (u16, Value) {
    unwrap_resp(
        ureq::post(url)
            .set("Content-Type", "application/json")
            .send_string(&body.to_string()),
    )
}

fn post_bytes(url: &str, bytes: &[u8]) -> (u16, Value) {
    unwrap_resp(
        ureq::post(url)
            .set("Content-Type", "application/octet-stream")
            .send_bytes(bytes),
    )
}

/// 最小合法 JPG/PNG 魔数（服务端只验魔数不验完整格式）
const TINY_JPG: &[u8] = &[0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, 0x4A, 0x46, 0x49, 0x46];
const TINY_PNG: &[u8] = &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00];

// ---------- 手机端分类 ----------

#[test]
fn t50_mobile_categories() {
    let env = TestEnv::new();
    let (code, body) = get(&env.url("/api/m/categories"));
    assert_eq!(code, 200);
    assert_eq!(body["ok"], true);
    let names: Vec<&str> = body["categories"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"饮料"), "应返回夹具分类，实际: {names:?}");
}

// ---------- 手机端加商品 → 传图 完整链路 ----------

#[test]
fn t51_mobile_add_then_upload_image_chain() {
    let env = TestEnv::new();

    // 第 1 步：建商品
    let (code, body) = post_json(
        &env.url("/api/m/product"),
        json!({"name":"娃哈哈AD钙奶","class":"饮料","abbr":"whh","jhj":1.8,"price":2.5}),
    );
    assert_eq!(code, 200, "建商品失败: {body}");
    let id = body["id"].as_i64().unwrap();

    // 新商品应立即在售（gds_state=1）、销量 0
    let (_, prods) = get(&env.url("/api/products"));
    let p = prods["products"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == id)
        .expect("新商品应出现在在售列表");
    assert_eq!(p["abbr"], "whh");
    assert_eq!(p["sold"], 0);

    // 第 2 步：传图，文件名由服务端按缩拼取
    let (code, body) = post_bytes(&env.url(&format!("/api/m/product/{id}/image")), TINY_JPG);
    assert_eq!(code, 200, "传图失败: {body}");
    assert_eq!(body["pic"], "whh.jpg");

    // 磁盘文件内容一致
    let on_disk = std::fs::read(env.base.join("data/image/whh.jpg")).expect("图片应落盘");
    assert_eq!(on_disk, TINY_JPG);

    // 管理端列表里 pic 已回填
    let (_, admin) = get(&env.url("/api/admin/products"));
    let p = admin["products"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == id)
        .unwrap();
    assert_eq!(p["pic"], "whh.jpg");

    // 图片可通过 /image/ 访问且字节一致
    let resp = ureq::get(&env.url("/image/whh.jpg")).call().unwrap();
    assert_eq!(resp.status(), 200);
    let mut buf = Vec::new();
    std::io::Read::read_to_end(&mut resp.into_reader(), &mut buf).unwrap();
    assert_eq!(buf, TINY_JPG);
}

// ---------- 手机端加商品校验 ----------

#[test]
fn t52_mobile_add_validation() {
    let env = TestEnv::new();
    // 带 id → 拒绝（手机端只新增不编辑）
    let (code, body) = post_json(
        &env.url("/api/m/product"),
        json!({"id":1,"name":"x","class":"饮料","price":1.0}),
    );
    assert_eq!(code, 400);
    assert!(body["error"].as_str().unwrap().contains("仅支持新增"));

    // 空名 / 超长名
    assert_eq!(post_json(&env.url("/api/m/product"), json!({"name":"  ","class":"饮料","price":1.0})).0, 400);
    assert_eq!(
        post_json(&env.url("/api/m/product"), json!({"name":"x".repeat(61),"class":"饮料","price":1.0})).0,
        400
    );
    // 负价 / 超上限价
    assert_eq!(post_json(&env.url("/api/m/product"), json!({"name":"a","class":"饮料","price":-1})).0, 400);
    assert_eq!(post_json(&env.url("/api/m/product"), json!({"name":"a","class":"饮料","price":100000})).0, 400);
    // 不存在的分类
    let (code, body) = post_json(&env.url("/api/m/product"), json!({"name":"a","class":"军火","price":1.0}));
    assert_eq!(code, 400);
    assert!(body["error"].as_str().unwrap().contains("分类"));
    // 缩拼含大写 / 中文
    assert_eq!(post_json(&env.url("/api/m/product"), json!({"name":"a","class":"饮料","price":1.0,"abbr":"WHH"})).0, 400);
    assert_eq!(post_json(&env.url("/api/m/product"), json!({"name":"a","class":"饮料","price":1.0,"abbr":"娃哈哈"})).0, 400);
}

// ---------- 缩拼冲突唯一化 ----------

#[test]
fn t53_abbr_conflict_gets_suffix() {
    let env = TestEnv::new();
    let (_, b1) = post_json(&env.url("/api/m/product"), json!({"name":"娃哈哈1","class":"饮料","abbr":"whh","price":2.0}));
    assert_eq!(b1["ok"], true);
    let (_, b2) = post_json(&env.url("/api/m/product"), json!({"name":"娃哈哈2","class":"饮料","abbr":"whh","price":2.0}));
    assert_eq!(b2["ok"], true);

    let (_, admin) = get(&env.url("/api/admin/products"));
    let abbrs: Vec<&str> = admin["products"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|p| p["name"].as_str().unwrap().starts_with("娃哈哈"))
        .map(|p| p["abbr"].as_str().unwrap())
        .collect();
    assert!(abbrs.contains(&"whh") && abbrs.contains(&"whh_1"), "冲突缩拼应追加 _1，实际: {abbrs:?}");
}

// ---------- 手机端传图校验 ----------

#[test]
fn t54_mobile_image_validation() {
    let env = TestEnv::new();

    // 空 body
    assert_eq!(post_bytes(&env.url("/api/m/product/1/image"), b"").0, 400);
    // 假图片（无魔数）
    let (code, body) = post_bytes(&env.url("/api/m/product/1/image"), b"this is not a jpg");
    assert_eq!(code, 400);
    assert!(body["error"].as_str().unwrap().contains("JPG/PNG"));
    // PNG 魔数也接受
    assert_eq!(post_bytes(&env.url("/api/m/product/1/image"), TINY_PNG).0, 200);
    // 超限（>3MB）：axum DefaultBodyLimit 直接 413
    let big = vec![0xFFu8; 3 * 1024 * 1024 + 1];
    let r = ureq::post(&env.url("/api/m/product/1/image"))
        .set("Content-Type", "application/octet-stream")
        .send_bytes(&big);
    match r {
        Err(ureq::Error::Status(code, _)) => assert_eq!(code, 413, "超 3MB 应 413"),
        Ok(resp) => panic!("超 3MB 不应成功: {}", resp.status()),
        Err(e) => panic!("请求异常: {e}"),
    }
    // 不存在的商品 id
    assert_eq!(post_bytes(&env.url("/api/m/product/9999/image"), TINY_JPG).0, 400);
    // 缩拼为空的商品不能传图（建商品不带缩拼 → 传图被拒）
    let (_, b) = post_json(&env.url("/api/m/product"), json!({"name":"无缩拼商品","class":"饮料","price":1.0}));
    let id = b["id"].as_i64().unwrap();
    let (code, body) = post_bytes(&env.url(&format!("/api/m/product/{id}/image")), TINY_JPG);
    assert_eq!(code, 400);
    assert!(body["error"].as_str().unwrap().contains("缩拼为空"));
}

// ---------- 重复传图覆盖同一文件（不互覆别人） ----------

#[test]
fn t55_mobile_image_reupload_overwrites_own_file() {
    let env = TestEnv::new();
    let v1 = [TINY_JPG, b"VERSION1"].concat();
    let v2 = [TINY_JPG, b"VERSION2"].concat();
    assert_eq!(post_bytes(&env.url("/api/m/product/1/image"), &v1).0, 200);
    assert_eq!(std::fs::read(env.base.join("data/image/bskl.jpg")).unwrap(), v1);
    assert_eq!(post_bytes(&env.url("/api/m/product/1/image"), &v2).0, 200);
    assert_eq!(std::fs::read(env.base.join("data/image/bskl.jpg")).unwrap(), v2, "重传应覆盖自己的图");
}

// ---------- hostinfo（二维码用） ----------

#[test]
fn t56_hostinfo_returns_usable_lan_ip() {
    let env = TestEnv::new();
    let (code, body) = get(&env.url("/api/admin/hostinfo"));
    assert_eq!(code, 200);
    assert_eq!(body["ok"], true);
    let ip: Ipv4Addr = body["lan_ip"]
        .as_str()
        .expect("lan_ip 必须是字符串")
        .parse()
        .expect("lan_ip 必须是合法 IPv4 地址");
    let o = ip.octets();
    // 绝不能返回手机永远连不上的地址：链路本地 169.254.* / 代理 fake-ip 198.18-19.*
    assert!(!(o[0] == 169 && o[1] == 254), "lan_ip 不应是链路本地地址: {ip}");
    assert!(!(o[0] == 198 && (o[1] == 18 || o[1] == 19)), "lan_ip 不应是 fake-ip: {ip}");
    // 只会是回环兜底或私网地址
    assert!(ip.is_loopback() || ip.is_private(), "lan_ip 应是回环或私网地址: {ip}");
}
