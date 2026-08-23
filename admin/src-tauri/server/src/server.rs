//! HTTP API 服务（axum）。顾客端与 Tauri 壳共用这一套状态。

use crate::announce::{Announcer, Kind};
use crate::config::AppDirs;
use crate::db::{Db, OrderItemIn, OrderSummary};
use axum::body::Body;
use axum::extract::{DefaultBodyLimit, Path, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::Response;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::sync::{broadcast, oneshot};

/// 订单/呼叫事件广播：Tauri 壳订阅后驱动桌面弹窗；测试也可订阅验证。
pub type EventTx = broadcast::Sender<Value>;

pub struct AppState {
    pub db: Db,
    pub dirs: AppDirs,
    pub announcer: Announcer,
    pub events: EventTx,
    /// 访问门禁模式：生产=TicketOrLocalhost（本机免票/外网卡验票），dev=Off
    pub auth: crate::auth::AuthMode,
}

/// 默认不开门禁（dev / 测试用）。
pub fn build_state(dirs: AppDirs) -> Result<Arc<AppState>, String> {
    build_state_with(dirs, crate::auth::AuthMode::Off)
}

pub fn build_state_with(dirs: AppDirs, auth: crate::auth::AuthMode) -> Result<Arc<AppState>, String> {
    dirs.ensure_dirs().map_err(|e| e.to_string())?;
    let db = Db::open(&dirs.db_path())?;
    let announcer = Announcer::spawn(dirs.sound_dir());
    let (events, _rx) = broadcast::channel(64);
    Ok(Arc::new(AppState { db, dirs, announcer, events, auth }))
}

#[derive(Debug, Deserialize)]
pub struct OrderReq {
    machine: String,
    pay_method: String,
    items: Vec<OrderItemReq>,
    // 注意：客户端传来的任何 price/total 字段一律忽略，金额以服务端数据库为准
}

#[derive(Debug, Deserialize)]
pub struct OrderItemReq {
    id: i64,
    qty: i64,
}

#[derive(Debug, Deserialize)]
pub struct CallReq {
    machine: String,
}

fn err(status: StatusCode, msg: &str) -> (StatusCode, Json<Value>) {
    (status, Json(json!({ "ok": false, "error": msg })))
}

async fn ping() -> Json<Value> {
    // time：服务器当前时间戳，客户端用它对时后再做 HMAC 签名（客户机时钟可能不准）
    Json(json!({ "ok": true, "name": "cybercafeShop-server", "time": crate::auth::now_ts() }))
}

/// 店铺信息（店名 + 欢迎语）：用户端顶部 header 每次打开页面时拉取。
async fn shopinfo(State(st): State<Arc<AppState>>) -> (StatusCode, Json<Value>) {
    match st.db.shop_info() {
        Ok((shop_name, welcome)) => {
            (StatusCode::OK, Json(json!({ "ok": true, "shop_name": shop_name, "welcome": welcome })))
        }
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, &e),
    }
}

async fn products(State(st): State<Arc<AppState>>) -> (StatusCode, Json<Value>) {
    match (st.db.categories(), st.db.products_on_sale()) {
        (Ok(c), Ok(p)) => {
            // pic_t：图片文件 mtime 作 URL 版本号（用户端不用也会带上，保持两端 JSON 结构一致）
            let products: Vec<Value> = p
                .into_iter()
                .map(|x| {
                    json!({
                        "id": x.id, "name": x.name, "class": x.class, "abbr": x.abbr,
                        "price": x.price, "pic": x.pic, "sold": x.sold,
                        "pic_t": pic_mtime(&st.dirs.image_dir(), &x.pic),
                    })
                })
                .collect();
            (StatusCode::OK, Json(json!({ "ok": true, "categories": c, "products": products })))
        }
        _ => err(StatusCode::INTERNAL_SERVER_ERROR, "读取商品失败"),
    }
}

async fn post_order(
    State(st): State<Arc<AppState>>,
    Json(req): Json<OrderReq>,
) -> (StatusCode, Json<Value>) {
    let machine = req.machine.trim().to_string();
    let items: Vec<OrderItemIn> = req.items.iter().map(|i| OrderItemIn { id: i.id, qty: i.qty }).collect();
    match st.db.place_order(&machine, &req.pay_method, &items) {
        Ok((id, total)) => {
            st.announcer.announce(&machine, Kind::Order);
            let _ = st.events.send(json!({
                "type": "order", "id": id, "machine": machine, "total": total,
            }));
            (StatusCode::OK, Json(json!({ "ok": true, "order_id": id, "total": total })))
        }
        Err(msg) => err(StatusCode::BAD_REQUEST, &msg),
    }
}

async fn post_call(
    State(st): State<Arc<AppState>>,
    Json(req): Json<CallReq>,
) -> (StatusCode, Json<Value>) {
    let machine = req.machine.trim().to_string();
    if machine.is_empty() || machine.chars().count() > 64 {
        return err(StatusCode::BAD_REQUEST, "机器名非法");
    }
    st.announcer.announce(&machine, Kind::Call);
    let _ = st.events.send(json!({ "type": "call", "machine": machine }));
    (StatusCode::OK, Json(json!({ "ok": true })))
}

// ---- 管理端本地 API 见 admin.rs（/api/orders、/api/order/{id}/status 均在本机守卫内） ----

/// 文件名白名单校验：只允许 字母/数字/点/下划线/连字符，拒绝路径穿越。
pub fn valid_filename(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && !name.contains("..")
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-')
}

/// 图片文件 mtime（Unix 秒）作为 URL 版本号（pic_t）。
/// 前端 `?t=<pic_t>`：文件不变则 URL 跨启动/跨切页稳定 → WebView2 磁盘缓存可命中；
/// 管理端/手机端重传图片后 mtime 变 → 新 URL → 各端自动拿新图，不依赖客户端时钟。
/// 文件缺失/stat 失败 → 0（URL 退化为 ?t=0，请求仍走正常 200/404 流程）。
pub fn pic_mtime(dir: &std::path::Path, name: &str) -> i64 {
    if name.is_empty() {
        return 0;
    }
    std::fs::metadata(dir.join(name))
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

async fn serve_file(dir: &std::path::Path, name: &str, headers: &HeaderMap) -> Response {
    if !valid_filename(name) {
        return Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .body(Body::from("bad filename"))
            .unwrap();
    }
    let path = dir.join(name);
    // ETag = 文件 mtime（hex）：URL 稳定时浏览器带 If-None-Match，未变 → 304 不读盘不传输
    let etag = format!("\"{:x}\"", pic_mtime(dir, name));
    if headers
        .get(header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
        == Some(etag.as_str())
    {
        return Response::builder()
            .status(StatusCode::NOT_MODIFIED)
            .header(header::ETAG, &etag)
            .header(header::CACHE_CONTROL, "no-cache")
            .body(Body::empty())
            .unwrap();
    }
    match std::fs::read(&path) {
        Ok(bytes) => {
            let mime = mime_guess::from_path(&path).first_or_octet_stream();
            Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, mime.as_ref())
                .header(header::ETAG, &etag)
                // no-cache：每次使用先 revalidate——上传新图（mtime 变）后所有端自动拿到新图，
                // 同时保证磁盘缓存不存死数据
                .header(header::CACHE_CONTROL, "no-cache")
                .body(Body::from(bytes))
                .unwrap()
        }
        Err(_) => Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Body::from("not found"))
            .unwrap(),
    }
}

async fn image(State(st): State<Arc<AppState>>, Path(name): Path<String>, headers: HeaderMap) -> Response {
    serve_file(&st.dirs.image_dir(), &name, &headers).await
}

async fn qrcode(State(st): State<Arc<AppState>>, Path(kind): Path<String>, headers: HeaderMap) -> Response {
    let file = match kind.as_str() {
        "wechat" => "wechat.png",
        "alipay" => "alipay.png",
        _ => {
            return Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(Body::from("not found"))
                .unwrap()
        }
    };
    serve_file(&st.dirs.qrcode_dir(), file, &headers).await
}

pub fn router(st: Arc<AppState>) -> Router {
    let admin_ctx = Arc::new(crate::admin::AdminCtx {
        db: st.db.clone(),
        image_dir: st.dirs.image_dir(),
        qrcode_dir: st.dirs.qrcode_dir(),
        events: st.events.clone(),
    });
    // 手机端：添加商品（无口令，页面隐蔽 + 局域网自用，见 mobile.rs）
    let mobile_ctx = Arc::new(crate::mobile::MobileCtx::new(
        st.db.clone(),
        st.dirs.image_dir(),
        st.events.clone(),
    ));
    // 公开接口：HMAC 时间票门禁，浏览器裸开 IP:端口 = 403。
    // dev/测试 auth=false 时中间件直接放行。
    // 注意：用户端网页已内嵌进用户端 exe，管理端不再托管 /shop/（去掉了 ServeDir）。
    let protected = Router::new()
        .route("/api/products", get(products))
        .route("/api/shopinfo", get(shopinfo))
        .route("/api/order", post(post_order))
        .route("/api/call", post(post_call))
        .route("/image/{name}", get(image))
        .route("/qrcode/{kind}", get(qrcode))
        .layer(axum::middleware::from_fn_with_state(
            st.clone(),
            crate::auth::require_ticket,
        ))
        .layer(DefaultBodyLimit::max(1000 * 1024)) // 请求体上限 1MB（超限会提前拒收并中止未读完的请求体）
        .with_state(st.clone());
    Router::new()
        .route("/api/ping", get(ping)) // 存活探测+对时，不加密（不暴露任何业务数据）
        .merge(protected)
        .with_state(st.clone())
        // 管理端 API：仅本机回环可访问（见 admin::localhost_only）
        .merge(crate::admin::admin_router(admin_ctx))
        // 手机端 API：无口令（/api/m/*，见 mobile.rs），管理端接口守卫不受影响
        .merge(crate::mobile::mobile_router(mobile_ctx))
        // 手机端添加商品页面静态托管（/m/）
        .nest_service(
            "/m",
            tower_http::services::ServeDir::new(st.dirs.mobile_dir()),
        )
        // dev 模式 vite（14201/14202）与 Tauri 自定义协议跨源调 API 需要 CORS
        .layer(tower_http::cors::CorsLayer::permissive())
}

/// 启动服务，收到 shutdown 信号后优雅退出（测试用；生产可传一个永不触发的 channel）。
pub async fn run(st: Arc<AppState>, port: u16, shutdown: oneshot::Receiver<()>) -> Result<(), String> {
    let app = router(st);
    let addr = std::net::SocketAddr::from(([0, 0, 0, 0], port));
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|e| format!("端口 {port} 绑定失败: {e}"))?;
    // with_connect_info：admin 守卫需要取对端地址判断是否本机
    axum::serve(listener, app.into_make_service_with_connect_info::<std::net::SocketAddr>())
        .with_graceful_shutdown(async move {
            let _ = shutdown.await;
        })
        .await
        .map_err(|e| e.to_string())
}

// 供 Tauri 壳直接调用的订单列表类型再导出
pub use crate::db::OrderSummary as OrderDto;
#[allow(dead_code)]
fn _assert_send_sync(_: &OrderSummary) {}
