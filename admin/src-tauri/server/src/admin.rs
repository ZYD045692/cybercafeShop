//! 管理端本地 API：仅允许本机（127.0.0.1）访问，客户机无法调用。
//! 商品/分类/收款码/销售记录的管理操作都走这里。

use crate::db::{Db, Product};
use axum::body::Bytes;
use axum::extract::{ConnectInfo, Query, Request, State};
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

/// 管理端共享状态需要的额外目录
#[derive(Clone)]
pub struct AdminCtx {
    pub db: Db,
    pub image_dir: PathBuf,
    pub qrcode_dir: PathBuf,
    /// 事件广播：订单状态变化时通知两个前端窗口（订单页 + 通知卡片页）
    pub events: crate::server::EventTx,
}

fn err(status: StatusCode, msg: &str) -> (StatusCode, Json<Value>) {
    (status, Json(json!({ "ok": false, "error": msg })))
}

/// 本机回环守卫：非 127.0.0.1 的请求一律 403
pub async fn localhost_only(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    req: Request,
    next: Next,
) -> Response {
    if addr.ip().is_loopback() {
        next.run(req).await
    } else {
        (StatusCode::FORBIDDEN, Json(json!({"ok":false,"error":"仅本机可访问"}))).into_response()
    }
}

// ---------------- 订单管理 ----------------

async fn list_orders(State(ctx): State<Arc<AdminCtx>>) -> (StatusCode, Json<Value>) {
    match ctx.db.orders(None, 500) {
        Ok(v) => (StatusCode::OK, Json(json!({ "ok": true, "orders": v }))),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, &e),
    }
}

#[derive(Debug, Deserialize)]
pub struct StatusReq {
    pub status: i64,
}

async fn set_status(
    State(ctx): State<Arc<AdminCtx>>,
    axum::extract::Path(id): axum::extract::Path<i64>,
    Json(req): Json<StatusReq>,
) -> (StatusCode, Json<Value>) {
    if req.status != 0 && req.status != 1 {
        return err(StatusCode::BAD_REQUEST, "状态非法");
    }
    match ctx.db.set_order_status(id, req.status) {
        Ok(()) => {
            // 广播订单状态变化：订单管理页与通知卡片页都监听 tf-event 并自动刷新，
            // 让两边同步（一边点了「已确认/已出货」，另一边对应订单/卡片消失）。
            let _ = ctx.events.send(json!({
                "type": "order-status", "id": id, "status": req.status,
            }));
            (StatusCode::OK, Json(json!({ "ok": true })))
        }
        Err(msg) => err(StatusCode::NOT_FOUND, &msg),
    }
}

// ---------------- 店铺信息（店名 + 客户端欢迎语，两个独立项） ----------------

async fn get_shopinfo(State(ctx): State<Arc<AdminCtx>>) -> (StatusCode, Json<Value>) {
    match ctx.db.shop_info() {
        Ok((shop_name, welcome)) => {
            (StatusCode::OK, Json(json!({"ok":true,"shop_name":shop_name,"welcome":welcome})))
        }
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, &e),
    }
}

#[derive(Debug, Deserialize)]
pub struct ShopInfoIn {
    pub shop_name: String,
    pub welcome: String,
}

async fn set_shopinfo(
    State(ctx): State<Arc<AdminCtx>>,
    Json(s): Json<ShopInfoIn>,
) -> (StatusCode, Json<Value>) {
    let name = s.shop_name.trim();
    let welcome = s.welcome.trim();
    if name.is_empty() || name.chars().count() > 30 {
        return err(StatusCode::BAD_REQUEST, "店名非法（1~30 字）");
    }
    if welcome.chars().count() > 60 {
        return err(StatusCode::BAD_REQUEST, "欢迎语最长 60 字");
    }
    match ctx.db.set_shop_info(name, welcome) {
        Ok(()) => (StatusCode::OK, Json(json!({"ok":true}))),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, &e),
    }
}

// ---------------- 商品管理 ----------------

#[derive(Debug, Deserialize)]
pub struct ProductIn {
    pub id: Option<i64>,
    pub name: String,
    pub class: String,
    pub abbr: Option<String>,
    pub jhj: Option<f64>,
    pub price: f64,
    pub pic: Option<String>,
}

async fn admin_products(State(ctx): State<Arc<AdminCtx>>) -> (StatusCode, Json<Value>) {
    match ctx.db.admin_products() {
        Ok(v) => {
            // pic_t：图片文件 mtime 作 URL 版本号（前端 ?t= 用它，不随启动/切页变，磁盘缓存可命中）
            let arr: Vec<Value> = v
                .into_iter()
                .map(|p| {
                    json!({
                        "id": p.id, "name": p.name, "class": p.class, "abbr": p.abbr,
                        "jhj": p.jhj, "price": p.price, "pic": p.pic, "sold": p.sold,
                        "state": p.state, "pic_t": crate::server::pic_mtime(&ctx.image_dir, &p.pic),
                    })
                })
                .collect();
            (StatusCode::OK, Json(json!({"ok": true, "products": arr})))
        }
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, &e),
    }
}

async fn upsert_product(
    State(ctx): State<Arc<AdminCtx>>,
    Json(p): Json<ProductIn>,
) -> (StatusCode, Json<Value>) {
    // 校验
    let name = p.name.trim();
    if name.is_empty() || name.chars().count() > 60 {
        return err(StatusCode::BAD_REQUEST, "商品名非法");
    }
    if p.class.trim().is_empty() || p.class.chars().count() > 20 {
        return err(StatusCode::BAD_REQUEST, "分类非法");
    }
    if !(0.0..=99999.0).contains(&p.price) {
        return err(StatusCode::BAD_REQUEST, "售价非法");
    }
    if let Some(j) = p.jhj {
        if !(0.0..=99999.0).contains(&j) {
            return err(StatusCode::BAD_REQUEST, "进价非法");
        }
    }
    let abbr = p.abbr.clone().unwrap_or_default();
    // 允许下划线：后端缩拼冲突唯一化会生成 whh_1/whh_2 这类带 _ 的缩拼
    if !abbr.is_empty()
        && (abbr.len() > 20
            || !abbr
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'))
    {
        return err(StatusCode::BAD_REQUEST, "缩拼只能是小写字母、数字和下划线");
    }
    match ctx.db.upsert_product(p.id, name, p.class.trim(), &abbr, p.jhj.unwrap_or(0.0), p.price, p.pic.as_deref()) {
        Ok(id) => (StatusCode::OK, Json(json!({"ok":true,"id":id}))),
        Err(e) => err(StatusCode::BAD_REQUEST, &e),
    }
}

#[derive(Debug, Deserialize)]
pub struct StateIn {
    pub state: i64,
}

async fn set_product_state(
    State(ctx): State<Arc<AdminCtx>>,
    axum::extract::Path(id): axum::extract::Path<i64>,
    Json(s): Json<StateIn>,
) -> (StatusCode, Json<Value>) {
    if s.state != 0 && s.state != 1 {
        return err(StatusCode::BAD_REQUEST, "状态非法");
    }
    match ctx.db.set_product_state(id, s.state) {
        Ok(()) => (StatusCode::OK, Json(json!({"ok":true}))),
        Err(e) => err(StatusCode::NOT_FOUND, &e),
    }
}

async fn delete_product(
    State(ctx): State<Arc<AdminCtx>>,
    axum::extract::Path(id): axum::extract::Path<i64>,
) -> (StatusCode, Json<Value>) {
    match ctx.db.delete_product(id) {
        Ok(pic) => {
            // 连带删除图片文件，避免磁盘孤儿文件堆积（只取文件名部分，防路径穿越）
            if let Some(pic) = pic {
                if let Some(name) = std::path::Path::new(&pic).file_name() {
                    let _ = std::fs::remove_file(ctx.image_dir.join(name));
                }
            }
            (StatusCode::OK, Json(json!({"ok":true})))
        }
        Err(e) => err(StatusCode::NOT_FOUND, &e),
    }
}

// ---------------- 分类管理 ----------------

#[derive(Debug, Deserialize)]
pub struct CategoryIn {
    pub name: String,
    pub rename_to: Option<String>,
}

async fn admin_categories(State(ctx): State<Arc<AdminCtx>>) -> (StatusCode, Json<Value>) {
    match ctx.db.categories() {
        Ok(v) => (StatusCode::OK, Json(json!({"ok":true,"categories":v}))),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, &e),
    }
}

async fn add_or_rename_category(
    State(ctx): State<Arc<AdminCtx>>,
    Json(c): Json<CategoryIn>,
) -> (StatusCode, Json<Value>) {
    let name = c.name.trim();
    if name.is_empty() || name.chars().count() > 20 || name == "全部商品" {
        return err(StatusCode::BAD_REQUEST, "分类名非法");
    }
    let r = match &c.rename_to {
        Some(new) => {
            let new = new.trim();
            if new.is_empty() || new.chars().count() > 20 || new == "全部商品" {
                return err(StatusCode::BAD_REQUEST, "新分类名非法");
            }
            ctx.db.rename_category(name, new)
        }
        None => ctx.db.add_category(name),
    };
    match r {
        Ok(()) => (StatusCode::OK, Json(json!({"ok":true}))),
        Err(e) => err(StatusCode::BAD_REQUEST, &e),
    }
}

async fn delete_category(
    State(ctx): State<Arc<AdminCtx>>,
    axum::extract::Path(name): axum::extract::Path<String>,
) -> (StatusCode, Json<Value>) {
    match ctx.db.delete_category(&name) {
        Ok(()) => (StatusCode::OK, Json(json!({"ok":true}))),
        Err(e) => err(StatusCode::BAD_REQUEST, &e),
    }
}

// ---------------- 图片上传 ----------------

/// 图片字节校验：非空、≤2MB、JPG/PNG 魔数。upload_image 与 upload_product_image 共用。
fn check_image(body: &Bytes) -> Result<(), &'static str> {
    if body.is_empty() || body.len() > 2 * 1024 * 1024 {
        return Err("图片大小非法");
    }
    let is_jpg = body.starts_with(&[0xFF, 0xD8, 0xFF]);
    let is_png = body.starts_with(&[0x89, 0x50, 0x4E, 0x47]);
    if !is_jpg && !is_png {
        return Err("只接受 JPG/PNG 图片");
    }
    Ok(())
}

/// 前端已用 canvas 裁剪压缩成 300x300 JPEG/PNG，这里只校验头、写盘。
async fn upload_image(
    State(ctx): State<Arc<AdminCtx>>,
    axum::extract::Path(name): axum::extract::Path<String>,
    body: Bytes,
) -> (StatusCode, Json<Value>) {
    if !crate::server::valid_filename(&name) {
        return err(StatusCode::BAD_REQUEST, "文件名非法");
    }
    if let Err(e) = check_image(&body) {
        return err(StatusCode::BAD_REQUEST, e);
    }
    match std::fs::write(ctx.image_dir.join(&name), &body) {
        Ok(()) => (StatusCode::OK, Json(json!({"ok":true}))),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

/// 管理端「先建商品、再按 id 传图」：图片文件名由服务端按该商品最终缩拼生成回填。
/// 缩拼在建商品时已唯一化（whh → whh_1），图随缩拼走，避免同缩拼商品互覆、删除时误删共用图。
/// 与手机端 mobile.rs::upload_product_image 同一套逻辑（这里挂在 localhost_only 守卫下）。
async fn upload_product_image(
    State(ctx): State<Arc<AdminCtx>>,
    axum::extract::Path(id): axum::extract::Path<i64>,
    body: Bytes,
) -> (StatusCode, Json<Value>) {
    if let Err(e) = check_image(&body) {
        return err(StatusCode::BAD_REQUEST, e);
    }
    let abbr = match ctx.db.product_abbr(id) {
        Ok(a) if !a.is_empty() => a,
        Ok(_) => return err(StatusCode::BAD_REQUEST, "商品缩拼为空，无法命名图片"),
        Err(e) => return err(StatusCode::BAD_REQUEST, &e),
    };
    let name = format!("{abbr}.jpg");
    if !crate::server::valid_filename(&name) {
        return err(StatusCode::INTERNAL_SERVER_ERROR, "缩拼生成文件名非法");
    }
    if let Err(e) = std::fs::write(ctx.image_dir.join(&name), &body) {
        return err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string());
    }
    match ctx.db.set_product_pic(id, &name) {
        Ok(()) => (StatusCode::OK, Json(json!({"ok": true, "pic": name}))),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, &e),
    }
}

async fn upload_qrcode(
    State(ctx): State<Arc<AdminCtx>>,
    axum::extract::Path(kind): axum::extract::Path<String>,
    body: Bytes,
) -> (StatusCode, Json<Value>) {
    let file = match kind.as_str() {
        "wechat" => "wechat.png",
        "alipay" => "alipay.png",
        _ => return err(StatusCode::BAD_REQUEST, "收款码类型非法"),
    };
    if body.is_empty() || body.len() > 2 * 1024 * 1024 {
        return err(StatusCode::BAD_REQUEST, "图片大小非法");
    }
    let is_jpg = body.starts_with(&[0xFF, 0xD8, 0xFF]);
    let is_png = body.starts_with(&[0x89, 0x50, 0x4E, 0x47]);
    if !is_jpg && !is_png {
        return err(StatusCode::BAD_REQUEST, "只接受 JPG/PNG 图片");
    }
    match std::fs::write(ctx.qrcode_dir.join(file), &body) {
        Ok(()) => (StatusCode::OK, Json(json!({"ok":true}))),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

// ---------------- 销售记录 ----------------

#[derive(Debug, Deserialize)]
pub struct RecordsQuery {
    pub from: Option<String>, // YYYY-MM-DD
    pub to: Option<String>,
    pub pay: Option<String>,  // wechat / alipay
}

async fn records(
    State(ctx): State<Arc<AdminCtx>>,
    Query(q): Query<RecordsQuery>,
) -> (StatusCode, Json<Value>) {
    // 日期格式粗校验
    for d in [&q.from, &q.to].into_iter().flatten() {
        if !valid_date(d) {
            return err(StatusCode::BAD_REQUEST, "日期格式非法");
        }
    }
    if let Some(p) = &q.pay {
        if !crate::db::PAY_METHODS.contains(&p.as_str()) {
            return err(StatusCode::BAD_REQUEST, "支付方式非法");
        }
    }
    match ctx.db.records(q.from.as_deref(), q.to.as_deref(), q.pay.as_deref()) {
        Ok((list, sum)) => (StatusCode::OK, Json(json!({"ok":true,"orders":list,"sum":sum}))),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, &e),
    }
}

fn valid_date(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 10
        && b[4] == b'-' && b[7] == b'-'
        && b.iter().enumerate().all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit())
}

/// 本机局域网 IPv4（枚举本机网卡取一个，不再依赖连外网/UDP 路由法）。
/// 之前用 `UdpSocket.connect("223.5.5.5:53")` 取「默认路由出口」的 IP，但当默认路由走到代理/虚拟
/// 网卡（如 198.18/19 fake-ip）时，拿到的正是要排除的段，退化到 127.0.0.1——网吧常开代理/虚拟网卡，
/// 二维码就会写成 127。改为枚举网卡：跳过回环、未指定、链路本地（169.254）、代理 fake-ip（198.18/19），
/// 优先取私有段（192.168/10/172.16-31，吧台局域网），否则取第一个非回环的 IPv4。
/// 注意：不要用 `ipconfig` 子进程——release 的 GUI 程序没有控制台，启动控制台子进程会瞬间
/// 弹一个空白控制台窗口（表现为「切到商品页闪一个透明窗」），这里已踩坑。
fn lan_ip() -> String {
    let mut first_other: Option<String> = None;
    if let Ok(ifaces) = if_addrs::get_if_addrs() {
        for iface in ifaces {
            if let std::net::IpAddr::V4(v4) = iface.ip() {
                let s = v4.to_string();
                // 排除回环、未指定、链路本地（网卡没拿到地址，手机扫了也连不上）、代理 fake-ip（198.18/19）
                if v4.is_loopback() || v4.is_unspecified() || v4.is_link_local()
                    || s.starts_with("198.18.") || s.starts_with("198.19.") {
                    continue;
                }
                if v4.is_private() {
                    return s; // 192.168/10/172.16-31：吧台局域网，优先
                }
                if first_other.is_none() {
                    first_other = Some(s);
                }
            }
        }
    }
    first_other.unwrap_or_else(|| "127.0.0.1".to_string())
}

/// 管理端取本机局域网 IPv4，用于在添加商品弹窗里生成手机端二维码
async fn host_info() -> Json<Value> {
    Json(json!({ "ok": true, "lan_ip": lan_ip() }))
}

pub fn admin_router(ctx: Arc<AdminCtx>) -> Router {
    Router::new()
        .route("/api/orders", axum::routing::get(list_orders))
        .route("/api/order/{id}/status", axum::routing::post(set_status))
        .route("/api/admin/shopinfo", axum::routing::get(get_shopinfo).post(set_shopinfo))
        .route("/api/admin/hostinfo", axum::routing::get(host_info))
        .route("/api/admin/products", axum::routing::get(admin_products))
        .route("/api/admin/product", axum::routing::post(upsert_product))
        .route("/api/admin/product/{id}/state", axum::routing::post(set_product_state))
        .route("/api/admin/product/{id}/image", axum::routing::post(upload_product_image))
        .route("/api/admin/product/{id}", axum::routing::delete(delete_product))
        .route("/api/admin/categories", axum::routing::get(admin_categories))
        .route("/api/admin/category", axum::routing::post(add_or_rename_category))
        .route("/api/admin/category/{name}", axum::routing::delete(delete_category))
        .route("/api/admin/image/{name}", axum::routing::post(upload_image))
        .route("/api/admin/qrcode/{kind}", axum::routing::post(upload_qrcode))
        .route("/api/admin/records", axum::routing::get(records))
        .layer(axum::middleware::from_fn(localhost_only))
        .layer(axum::extract::DefaultBodyLimit::max(3 * 1024 * 1024))
        .with_state(ctx)
}

// 让 Product 可复用到管理端列表
#[allow(dead_code)]
fn _t(_: Product) {}
