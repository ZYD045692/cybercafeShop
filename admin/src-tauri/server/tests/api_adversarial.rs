//! API 对抗性测试。
//!
//! 隔离原则（参考 Landisk 测试约定）：
//! - 每个用例自建独立临时数据目录（tempfile，结束自动清理），绝不触碰生产目录；
//! - 测试端口使用不常见端口，避免与本机其他服务/生产端口 21974 冲突；
//! - 每个环境独立起停服务实例，用例间互不影响。

use serde_json::{json, Value};
use std::net::TcpStream;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use cybercafe_shop::config::AppDirs;
use cybercafe_shop::server;

// 测试端口：从 23917 起原子自增，避开生产 21974 及常见端口；
// 每个用例独立端口，用例可并行互不影响
use std::sync::atomic::{AtomicU16, Ordering};
static NEXT_PORT: AtomicU16 = AtomicU16::new(23917);
fn next_port() -> u16 {
    NEXT_PORT.fetch_add(1, Ordering::SeqCst)
}

struct TestEnv {
    // _dir（TempDir）要放在 state 之后声明：drop 顺序 = 声明顺序，
    // state 先 drop（释放 SQLite 连接）→ _dir 最后 drop（才能删干净临时目录）。
    base: PathBuf,
    port: u16,
    state: std::sync::Arc<server::AppState>,
    shutdown: Option<tokio::sync::oneshot::Sender<()>>,
    handle: Option<std::thread::JoinHandle<()>>,
    _dir: tempfile::TempDir,
}

impl TestEnv {
    /// variant 区分夹具数据，用于验证多环境隔离
    fn new(variant: &str) -> TestEnv {
        let port = next_port();
        let dir = tempfile::tempdir().expect("创建临时目录失败");
        let base = dir.path().to_path_buf();
        let dirs = AppDirs::new(base.clone());
        dirs.ensure_dirs().unwrap();
        std::fs::write(base.join("config.ini"), format!("[server]\nport={port}\n")).unwrap();

        // 夹具数据：一个分类 + 两个商品（一个在售一个下架）
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
        conn.execute(
            "INSERT INTO shop_fl VALUES (1,'饮料',8)", [],
        ).unwrap();
        conn.execute(
            "INSERT INTO shop_list (id,gds_number,gds_class,gds_name,gds_jhj,gds_xsj,gds_gys,gds_pic,gds_px,gds_state,gds_out,gds_js)
             VALUES (1,'bskl','饮料',?1,2.5,3.5,'默认','bskl.jpg',6,1,100,'')",
            [format!("测试可乐{variant}")],
        ).unwrap();
        conn.execute(
            "INSERT INTO shop_list (id,gds_number,gds_class,gds_name,gds_jhj,gds_xsj,gds_gys,gds_pic,gds_px,gds_state,gds_out,gds_js)
             VALUES (2,'xsj','饮料','下架商品X',1.0,9.9,'默认','xsj.jpg',6,0,5,'')",
            [],
        ).unwrap();
        drop(conn);

        std::fs::write(dirs.image_dir().join("ok.jpg"), b"fake-jpg").unwrap();
        std::fs::write(dirs.qrcode_dir().join("wechat.png"), b"fake-png").unwrap();

        let state = server::build_state(dirs).expect("build_state 失败");
        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        let st2 = state.clone();
        let handle = std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async move {
                server::run(st2, port, rx).await.expect("服务运行失败");
            });
        });

        // 等服务就绪
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if TcpStream::connect(("127.0.0.1", port)).is_ok() {
                break;
            }
            assert!(Instant::now() < deadline, "服务启动超时");
            std::thread::sleep(Duration::from_millis(50));
        }

        TestEnv { _dir: dir, base, port, state, shutdown: Some(tx), handle: Some(handle) }
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

// ---------- HTTP 客户端辅助 ----------

fn post_json(url: &str, body: Value) -> (u16, Value) {
    let r = ureq::post(url)
        .set("Content-Type", "application/json")
        .send_string(&body.to_string());
    unwrap_resp(r)
}

fn post_raw(url: &str, content_type: &str, body: &str) -> (u16, Value) {
    let r = ureq::post(url).set("Content-Type", content_type).send_string(body);
    unwrap_resp(r)
}

fn unwrap_resp(r: Result<ureq::Response, ureq::Error>) -> (u16, Value) {
    match r {
        Ok(resp) => {
            let code = resp.status();
            let text = resp.into_string().unwrap_or_default();
            (code, serde_json::from_str(&text).unwrap_or(json!({"raw": text})))
        }
        Err(ureq::Error::Status(code, resp)) => {
            let text = resp.into_string().unwrap_or_default();
            (code, serde_json::from_str(&text).unwrap_or(json!({"raw": text})))
        }
        Err(e) => panic!("传输层错误: {e}"),
    }
}

fn get(url: &str) -> (u16, Value) {
    unwrap_resp(ureq::get(url).call())
}

fn valid_order() -> Value {
    json!({
        "machine": "PC-01",
        "pay_method": "wechat",
        "items": [{"id": 1, "qty": 2}]
    })
}

// ---------- 正常流程 ----------

#[test]
fn t01_ping() {
    let env = TestEnv::new("A");
    let (code, body) = get(&env.url("/api/ping"));
    assert_eq!(code, 200);
    assert_eq!(body["ok"], true);
}

#[test]
fn t02_products_only_on_sale_with_sold() {
    let env = TestEnv::new("A");
    let (code, body) = get(&env.url("/api/products"));
    assert_eq!(code, 200);
    let prods = body["products"].as_array().unwrap();
    assert_eq!(prods.len(), 1, "下架商品不应出现在顾客端");
    assert_eq!(prods[0]["name"], "测试可乐A");
    assert_eq!(prods[0]["price"], 3.5);
    assert_eq!(prods[0]["sold"], 100, "需要带销量给顾客参考");
    assert_eq!(body["categories"][0]["name"], "饮料");
}

#[test]
fn t03_order_happy_path_and_total_is_server_side() {
    let env = TestEnv::new("A");
    let (code, body) = post_json(&env.url("/api/order"), valid_order());
    assert_eq!(code, 200, "{body}");
    assert_eq!(body["ok"], true);
    assert_eq!(body["total"], 7.0, "2 × 3.5");
    let oid = body["order_id"].as_i64().unwrap();

    // 订单进库、状态未处理、明细完整
    let (code, list) = get(&env.url("/api/orders"));
    assert_eq!(code, 200);
    let orders = list["orders"].as_array().unwrap();
    let o = orders.iter().find(|o| o["id"] == oid).unwrap();
    assert_eq!(o["machine"], "PC-01");
    assert_eq!(o["status"], 0);
    assert_eq!(o["items"][0]["name"], "测试可乐A");
    assert_eq!(o["items"][0]["qty"], 2);

    // 销量累加
    let (_, body) = get(&env.url("/api/products"));
    assert_eq!(body["products"][0]["sold"], 102);

    // 处理订单
    let (code, _) = post_json(&env.url(&format!("/api/order/{oid}/status")), json!({"status": 1}));
    assert_eq!(code, 200);
    let (_, list) = get(&env.url("/api/orders"));
    let o = &list["orders"].as_array().unwrap()[0];
    assert_eq!(o["status"], 1);
}

#[test]
fn t04_order_event_broadcast() {
    let env = TestEnv::new("A");
    let mut rx = env.state.events.subscribe();
    let (code, _) = post_json(&env.url("/api/order"), valid_order());
    assert_eq!(code, 200);
    let ev = rx.try_recv().expect("下单后应广播事件驱动桌面弹窗");
    assert_eq!(ev["type"], "order");
    assert_eq!(ev["machine"], "PC-01");
}

// ---------- 对抗：价格/金额篡改 ----------

#[test]
fn t10_client_price_field_is_ignored() {
    let env = TestEnv::new("A");
    let mut body = valid_order();
    body["items"][0]["price"] = json!(0.01); // 客户端试图自报价格
    body["total"] = json!(0.01);
    let (code, resp) = post_json(&env.url("/api/order"), body);
    assert_eq!(code, 200);
    assert_eq!(resp["total"], 7.0, "金额必须以服务端数据库为准");
}

// ---------- 对抗：SQL 注入 ----------

#[test]
fn t11_sql_injection_in_machine_name() {
    let env = TestEnv::new("A");
    let evil = "x'); DROP TABLE orders; DROP TABLE shop_list;--";
    let mut body = valid_order();
    body["machine"] = json!(evil);
    let (code, _) = post_json(&env.url("/api/order"), body);
    assert_eq!(code, 200);
    // 表还在，后续功能正常
    let (code, prods) = get(&env.url("/api/products"));
    assert_eq!(code, 200);
    assert_eq!(prods["products"].as_array().unwrap().len(), 1);
    let (_, list) = get(&env.url("/api/orders"));
    assert_eq!(list["orders"][0]["machine"], evil, "注入串应被当作普通字符串存储");
}

// ---------- 对抗：非法商品/数量/支付方式 ----------

#[test]
fn t12_unknown_product_rejected() {
    let env = TestEnv::new("A");
    let (code, resp) = post_json(&env.url("/api/order"),
        json!({"machine":"PC-01","pay_method":"wechat","items":[{"id":9999,"qty":1}]}));
    assert_eq!(code, 400);
    assert_eq!(resp["ok"], false);
    let (_, list) = get(&env.url("/api/orders"));
    assert_eq!(list["orders"].as_array().unwrap().len(), 0, "失败订单不得落库");
}

#[test]
fn t13_offsale_product_rejected() {
    let env = TestEnv::new("A");
    let (code, _) = post_json(&env.url("/api/order"),
        json!({"machine":"PC-01","pay_method":"wechat","items":[{"id":2,"qty":1}]}));
    assert_eq!(code, 400, "下架商品不可下单");
}

#[test]
fn t14_bad_qty_rejected() {
    let env = TestEnv::new("A");
    for bad in [0, -3, 100, i64::MAX] {
        let (code, _) = post_json(&env.url("/api/order"),
            json!({"machine":"PC-01","pay_method":"wechat","items":[{"id":1,"qty":bad}]}));
        assert_eq!(code, 400, "qty={bad} 应被拒绝");
    }
    let (code, _) = post_json(&env.url("/api/order"),
        json!({"machine":"PC-01","pay_method":"wechat","items":[{"id":1,"qty":99}]}));
    assert_eq!(code, 200, "qty=99 是合法上界");
}

#[test]
fn t15_items_count_limits() {
    let env = TestEnv::new("A");
    let (code, _) = post_json(&env.url("/api/order"),
        json!({"machine":"PC-01","pay_method":"wechat","items":[]}));
    assert_eq!(code, 400, "空购物车");
    let items: Vec<Value> = (0..51).map(|_| json!({"id":1,"qty":1})).collect();
    let (code, _) = post_json(&env.url("/api/order"),
        json!({"machine":"PC-01","pay_method":"wechat","items":items}));
    assert_eq!(code, 400, "超过 50 条明细");
}

#[test]
fn t16_pay_method_whitelist() {
    let env = TestEnv::new("A");
    for bad in ["visa", "微信", "", "wechat ']--"] {
        let (code, _) = post_json(&env.url("/api/order"),
            json!({"machine":"PC-01","pay_method":bad,"items":[{"id":1,"qty":1}]}));
        assert_eq!(code, 400, "pay_method={bad:?} 应被拒绝");
    }
    for ok in ["wechat", "alipay", "cash"] {
        let (code, _) = post_json(&env.url("/api/order"),
            json!({"machine":"PC-01","pay_method":ok,"items":[{"id":1,"qty":1}]}));
        assert_eq!(code, 200, "pay_method={ok} 应被接受");
    }
}

// ---------- 对抗：机器名 ----------

#[test]
fn t17_machine_name_rules() {
    let env = TestEnv::new("A");
    // 空名
    let (code, _) = post_json(&env.url("/api/order"),
        json!({"machine":"   ","pay_method":"wechat","items":[{"id":1,"qty":1}]}));
    assert_eq!(code, 400);
    // 超长（65 字符）
    let (code, _) = post_json(&env.url("/api/order"),
        json!({"machine":"A".repeat(65),"pay_method":"wechat","items":[{"id":1,"qty":1}]}));
    assert_eq!(code, 400);
    // 中文用户名允许下单（播报层会过滤/兜底）
    let (code, _) = post_json(&env.url("/api/order"),
        json!({"machine":"网吧一号机","pay_method":"wechat","items":[{"id":1,"qty":1}]}));
    assert_eq!(code, 200);
}

// ---------- 对抗：畸形请求 ----------

#[test]
fn t18_malformed_json_rejected() {
    let env = TestEnv::new("A");
    let (code, _) = post_raw(&env.url("/api/order"), "application/json", "{not json at all");
    assert!((400..500).contains(&code), "畸形 JSON 应 4xx，实际 {code}");
    let (code, _) = post_raw(&env.url("/api/order"), "text/plain", "hello");
    assert!((400..500).contains(&code), "错误 Content-Type 应 4xx，实际 {code}");
    // 服务仍然活着
    assert_eq!(get(&env.url("/api/ping")).0, 200);
}

#[test]
fn t19_oversized_body_rejected() {
    let env = TestEnv::new("A");
    let big = json!({"machine":"A".repeat(200_000),"pay_method":"wechat","items":[{"id":1,"qty":1}]});
    let (code, _) = post_json(&env.url("/api/order"), big);
    assert_eq!(code, 413, "超过 64KB 请求体应 413，实际 {code}");
    assert_eq!(get(&env.url("/api/ping")).0, 200);
}

// ---------- 对抗：路径穿越 ----------

#[test]
fn t20_image_path_traversal_blocked() {
    let env = TestEnv::new("A");
    for p in [
        "/image/..%2Fconfig.ini",
        "/image/..%2F..%2Fetc%2Fpasswd",
        "/image/%2e%2e%2fconfig.ini",
        "/image/.../config.ini",
    ] {
        let (code, body) = get(&env.url(p));
        assert!(code == 400 || code == 404, "{p} 应被拒绝，实际 {code}");
        let s = body.to_string();
        assert!(!s.contains("port"), "{p} 泄露了配置文件内容");
    }
    // 不存在的文件 404，存在的正常取
    assert_eq!(get(&env.url("/image/nope.jpg")).0, 404);
    let (code, _) = get(&env.url("/image/ok.jpg"));
    assert_eq!(code, 200);
}

#[test]
fn t21_qrcode_kind_whitelist() {
    let env = TestEnv::new("A");
    assert_eq!(get(&env.url("/qrcode/wechat")).0, 200);
    assert_eq!(get(&env.url("/qrcode/alipay")).0, 404, "夹具未提供 alipay.png");
    assert_eq!(get(&env.url("/qrcode/cash")).0, 404);
    assert_eq!(get(&env.url("/qrcode/..%2F..%2Fconfig.ini")).0, 404);
}

// ---------- 对抗：并发 ----------

#[test]
fn t22_concurrent_orders() {
    let env = TestEnv::new("A");
    let mut handles = Vec::new();
    for i in 0..10 {
        let url = env.url("/api/order");
        handles.push(std::thread::spawn(move || {
            post_json(&url, json!({
                "machine": format!("PC-{i:02}"),
                "pay_method": "wechat",
                "items": [{"id": 1, "qty": 1}]
            }))
        }));
    }
    let mut ids = Vec::new();
    for h in handles {
        let (code, body) = h.join().unwrap();
        assert_eq!(code, 200, "{body}");
        ids.push(body["order_id"].as_i64().unwrap());
    }
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), 10, "订单号必须唯一");
    // 销量准确累加
    let (_, body) = get(&env.url("/api/products"));
    assert_eq!(body["products"][0]["sold"], 110);
}

// ---------- 对抗：状态设置 ----------

#[test]
fn t23_status_validation() {
    let env = TestEnv::new("A");
    let (_, body) = post_json(&env.url("/api/order"), valid_order());
    let oid = body["order_id"].as_i64().unwrap();
    // 非法状态值
    let (code, _) = post_json(&env.url(&format!("/api/order/{oid}/status")), json!({"status": 5}));
    assert_eq!(code, 400);
    // 不存在的订单
    let (code, _) = post_json(&env.url("/api/order/99999/status"), json!({"status": 1}));
    assert_eq!(code, 404);
    // 重复处理幂等
    assert_eq!(post_json(&env.url(&format!("/api/order/{oid}/status")), json!({"status": 1})).0, 200);
    assert_eq!(post_json(&env.url(&format!("/api/order/{oid}/status")), json!({"status": 1})).0, 200);
}

// ---------- 呼叫网管 ----------

#[test]
fn t24_call_netmanager() {
    let env = TestEnv::new("A");
    let mut rx = env.state.events.subscribe();
    let (code, _) = post_json(&env.url("/api/call"), json!({"machine": "PC-15"}));
    assert_eq!(code, 200);
    let ev = rx.try_recv().expect("呼叫应广播事件");
    assert_eq!(ev["type"], "call");
    assert_eq!(ev["machine"], "PC-15");
    // 空机器名拒绝
    let (code, _) = post_json(&env.url("/api/call"), json!({"machine": ""}));
    assert_eq!(code, 400);
}

// ---------- 环境隔离 ----------

#[test]
fn t25_two_envs_isolated() {
    let a = TestEnv::new("甲");
    let b = TestEnv::new("乙");
    let (_, pa) = get(&a.url("/api/products"));
    let (_, pb) = get(&b.url("/api/products"));
    assert_eq!(pa["products"][0]["name"], "测试可乐甲");
    assert_eq!(pb["products"][0]["name"], "测试可乐乙");
    // A 下单不影响 B
    post_json(&a.url("/api/order"), valid_order());
    let (_, lb) = get(&b.url("/api/orders"));
    assert_eq!(lb["orders"].as_array().unwrap().len(), 0, "环境间订单必须隔离");
    assert!(a.base != b.base);
}

#[test]
fn t26_temp_env_fully_cleaned_after_drop() {
    let path;
    {
        let env = TestEnv::new("临时");
        path = env.base.clone();
        assert!(path.join("data/db/shop_db.db").exists());
    } // Drop：关服务、临时目录自动删除
    assert!(!path.exists(), "测试环境目录应被完全清理，不留残余");
}

// ==================== 管理端 API 对抗性测试 ====================

fn post_bytes(url: &str, bytes: &[u8]) -> (u16, Value) {
    unwrap_resp(ureq::post(url).set("Content-Type", "application/octet-stream").send_bytes(bytes))
}

#[test]
fn t30_admin_products_include_offsale_and_cost() {
    let env = TestEnv::new("A");
    let (code, body) = get(&env.url("/api/admin/products"));
    assert_eq!(code, 200);
    let prods = body["products"].as_array().unwrap();
    assert_eq!(prods.len(), 2, "管理端应看到全部商品含下架");
    let off = prods.iter().find(|p| p["name"] == "下架商品X").unwrap();
    assert_eq!(off["state"], 0);
    assert_eq!(off["jhj"], 1.0, "管理端需要进价");
}

#[test]
fn t31_product_add_and_validation() {
    let env = TestEnv::new("A");
    // 正常新增
    let (code, body) = post_json(&env.url("/api/admin/product"), json!({
        "name":"新上架奶茶500ml","class":"饮料","abbr":"xjnc","jhj":3.0,"price":5.0,"pic":"xjnc.jpg"
    }));
    assert_eq!(code, 200, "{body}");
    let new_id = body["id"].as_i64().unwrap();
    // 顾客端可见
    let (_, prods) = get(&env.url("/api/products"));
    assert!(prods["products"].as_array().unwrap().iter().any(|p| p["id"] == new_id));

    // 分类不存在
    let (code, _) = post_json(&env.url("/api/admin/product"), json!({
        "name":"X","class":"不存在的分类","abbr":"xx","price":1.0}));
    assert_eq!(code, 400);
    // 负价格/超大价格
    for bad in [-1.0, 100000.0] {
        let (code, _) = post_json(&env.url("/api/admin/product"), json!({
            "name":"X","class":"饮料","abbr":"xx","price":bad}));
        assert_eq!(code, 400, "price={bad}");
    }
    // 缩拼含大写/中文
    for bad in ["ABC", "百事", "ab cd"] {
        let (code, _) = post_json(&env.url("/api/admin/product"), json!({
            "name":"X","class":"饮料","abbr":bad,"price":1.0}));
        assert_eq!(code, 400, "abbr={bad}");
    }
    // 空名/超长名
    let (code, _) = post_json(&env.url("/api/admin/product"), json!({
        "name":"  ","class":"饮料","abbr":"xx","price":1.0}));
    assert_eq!(code, 400);
    let (code, _) = post_json(&env.url("/api/admin/product"), json!({
        "name":"超".repeat(61),"class":"饮料","abbr":"xx","price":1.0}));
    assert_eq!(code, 400);

    // 修改：改名改价
    let (code, _) = post_json(&env.url("/api/admin/product"), json!({
        "id":new_id,"name":"改名奶茶500ml","class":"饮料","abbr":"xjnc","jhj":3.5,"price":6.0}));
    assert_eq!(code, 200);
    let (_, prods) = get(&env.url("/api/admin/products"));
    let p = prods["products"].as_array().unwrap().iter().find(|p| p["id"] == new_id).unwrap();
    assert_eq!(p["name"], "改名奶茶500ml");
    assert_eq!(p["price"], 6.0);
    // 改不存在的商品
    let (code, _) = post_json(&env.url("/api/admin/product"), json!({
        "id":99999,"name":"X","class":"饮料","abbr":"xx","price":1.0}));
    assert_eq!(code, 400);
}

#[test]
fn t32_product_state_toggle() {
    let env = TestEnv::new("A");
    // 下架商品1
    let (code, _) = post_json(&env.url("/api/admin/product/1/state"), json!({"state":0}));
    assert_eq!(code, 200);
    let (_, prods) = get(&env.url("/api/products"));
    assert_eq!(prods["products"].as_array().unwrap().len(), 0, "下架后顾客端不可见");
    // 再上架
    assert_eq!(post_json(&env.url("/api/admin/product/1/state"), json!({"state":1})).0, 200);
    let (_, prods) = get(&env.url("/api/products"));
    assert_eq!(prods["products"].as_array().unwrap().len(), 1);
    // 非法状态/不存在商品
    assert_eq!(post_json(&env.url("/api/admin/product/1/state"), json!({"state":7})).0, 400);
    assert_eq!(post_json(&env.url("/api/admin/product/9999/state"), json!({"state":1})).0, 404);
}

#[test]
fn t33_product_delete() {
    let env = TestEnv::new("A");
    let (code, _) = unwrap_resp(ureq::delete(&env.url("/api/admin/product/2")).call());
    assert_eq!(code, 200);
    let (_, prods) = get(&env.url("/api/admin/products"));
    assert_eq!(prods["products"].as_array().unwrap().len(), 1);
    let (code, _) = unwrap_resp(ureq::delete(&env.url("/api/admin/product/2")).call());
    assert_eq!(code, 404, "重复删除");
}

#[test]
fn t34_category_crud_and_guards() {
    let env = TestEnv::new("A");
    // 新增
    assert_eq!(post_json(&env.url("/api/admin/category"), json!({"name":"泡面"})).0, 200);
    // 重复新增
    assert_eq!(post_json(&env.url("/api/admin/category"), json!({"name":"泡面"})).0, 400);
    // 非法名
    assert_eq!(post_json(&env.url("/api/admin/category"), json!({"name":""})).0, 400);
    assert_eq!(post_json(&env.url("/api/admin/category"), json!({"name":"全部商品"})).0, 400);
    // 重命名 + 商品分类同步
    assert_eq!(post_json(&env.url("/api/admin/category"), json!({"name":"泡面","rename_to":"速食"})).0, 200);
    let (_, cats) = get(&env.url("/api/admin/categories"));
    assert!(cats["categories"].as_array().unwrap().iter().any(|c| c["name"] == "速食"));
    // 重命名商品所在分类，商品跟着走
    assert_eq!(post_json(&env.url("/api/admin/category"), json!({"name":"饮料","rename_to":"饮品"})).0, 200);
    let (_, prods) = get(&env.url("/api/admin/products"));
    assert!(prods["products"].as_array().unwrap().iter().all(|p| p["class"] == "饮品"));
    // 有商品的分类不能删
    let (code, _) = unwrap_resp(ureq::delete(&env.url("/api/admin/category/%E9%A5%AE%E5%93%81")).call());
    assert_eq!(code, 400);
    // 空分类可删
    let (code, _) = unwrap_resp(ureq::delete(&env.url("/api/admin/category/%E9%80%9F%E9%A3%9F")).call());
    assert_eq!(code, 200);
    // 系统分类"全部商品"不可删
    let (code, _) = unwrap_resp(ureq::delete(&env.url("/api/admin/category/%E5%85%A8%E9%83%A8%E5%95%86%E5%93%81")).call());
    assert!((400..500).contains(&code));
}

#[test]
fn t35_image_upload_validation() {
    let env = TestEnv::new("A");
    let jpg = [0xFF, 0xD8, 0xFF, 0xE0, 1, 2, 3];
    // 正常上传
    assert_eq!(post_bytes(&env.url("/api/admin/image/newpic.jpg"), &jpg).0, 200);
    assert_eq!(get(&env.url("/image/newpic.jpg")).0, 200, "上传后立即可取");
    // 非图片内容
    assert_eq!(post_bytes(&env.url("/api/admin/image/evil.jpg"), b"MZ not an image").0, 400);
    // 路径穿越文件名
    assert_eq!(post_bytes(&env.url("/api/admin/image/..%2Fevil.jpg"), &jpg).0, 400);
    assert!(!env.base.join("evil.jpg").exists(), "不得写到目录外");
    // 空 body
    assert_eq!(post_bytes(&env.url("/api/admin/image/x.jpg"), &[]).0, 400);
}

#[test]
fn t36_qrcode_upload_and_serve() {
    let env = TestEnv::new("A");
    // 夹具预置了 wechat.png
    assert_eq!(get(&env.url("/qrcode/wechat")).0, 200);
    // 上传 alipay 后立即可取
    let png = [0x89, 0x50, 0x4E, 0x47, 1, 2, 3];
    assert_eq!(post_bytes(&env.url("/api/admin/qrcode/alipay"), &png).0, 200);
    assert_eq!(get(&env.url("/qrcode/alipay")).0, 200);
    // 非法类型/非图片内容
    assert_eq!(post_bytes(&env.url("/api/admin/qrcode/cash"), &png).0, 400);
    assert_eq!(post_bytes(&env.url("/api/admin/qrcode/wechat"), b"not image").0, 400);
}

#[test]
fn t37_records_filter_and_sum() {
    let env = TestEnv::new("A");
    post_json(&env.url("/api/order"), json!({"machine":"PC-01","pay_method":"wechat","items":[{"id":1,"qty":2}]})); // 7.0
    post_json(&env.url("/api/order"), json!({"machine":"PC-02","pay_method":"alipay","items":[{"id":1,"qty":1}]})); // 3.5
    // 全部
    let (code, body) = get(&env.url("/api/admin/records"));
    assert_eq!(code, 200);
    assert_eq!(body["orders"].as_array().unwrap().len(), 2);
    assert_eq!(body["sum"], 10.5);
    // 按支付方式
    let (_, body) = get(&env.url("/api/admin/records?pay=wechat"));
    assert_eq!(body["orders"].as_array().unwrap().len(), 1);
    assert_eq!(body["sum"], 7.0);
    // 按日期（今天应命中，未来日期应空）
    let today = "2099-01-01";
    let (_, body) = get(&env.url(&format!("/api/admin/records?from={today}")));
    assert_eq!(body["orders"].as_array().unwrap().len(), 0);
    // 非法日期/支付方式
    assert_eq!(get(&env.url("/api/admin/records?from=20260101")).0, 400);
    assert_eq!(get(&env.url("/api/admin/records?from=2026-01-01;DROP")).0, 400);
    assert_eq!(get(&env.url("/api/admin/records?pay=cash")).0, 200, "现金是合法支付方式");
    assert_eq!(get(&env.url("/api/admin/records?pay=visa")).0, 400);
}
