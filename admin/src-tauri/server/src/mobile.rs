//! 手机端 API（/api/m/*）：吧台人员用手机浏览器添加商品。
//!
//! 不做口令鉴权：页面地址 /m/ 不对外宣传，网吧顾客一般接触不到（局域网自用，风险可接受）。
//! 注意：`/api/m/product`、`/api/m/image/{name}` 是局域网内裸写的，懂行的人可用脚本直接调；
//! 想加口令时把 login + require_m_token 加回来即可（本文件早期版本有完整实现）。
//!
//! 复用现有 db 逻辑与图片魔数校验，管理端本机接口（/api/admin/*）不受影响。
//! 只暴露分类/添加商品/传图三类，不暴露删除/订单/销售/店铺配置。

use crate::admin::ProductIn;
use crate::db::Db;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::{Json, Router};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::Arc;

/// 手机端共享状态（无鉴权，仅数据目录）
#[derive(Clone)]
pub struct MobileCtx {
    pub db: Db,
    pub image_dir: PathBuf,
}

impl MobileCtx {
    pub fn new(db: Db, image_dir: PathBuf) -> MobileCtx {
        MobileCtx { db, image_dir }
    }
}

async fn categories(State(ctx): State<Arc<MobileCtx>>) -> (StatusCode, Json<Value>) {
    match ctx.db.categories() {
        Ok(c) => (StatusCode::OK, Json(json!({"ok": true, "categories": c}))),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, &e),
    }
}

async fn add_product(
    State(ctx): State<Arc<MobileCtx>>,
    Json(p): Json<ProductIn>,
) -> (StatusCode, Json<Value>) {
    // 复用管理端同款字段校验
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
    // 手机端只做新增，不允许编辑已有商品
    if p.id.is_some() {
        return err(StatusCode::BAD_REQUEST, "手机端仅支持新增商品");
    }
    let abbr = p.abbr.clone().unwrap_or_default();
    if !abbr.is_empty()
        && (abbr.len() > 20
            || !abbr
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'))
    {
        return err(StatusCode::BAD_REQUEST, "缩拼只能是小写字母、数字和下划线");
    }
    match ctx
        .db
        .upsert_product(None, name, p.class.trim(), &abbr, p.jhj.unwrap_or(0.0), p.price, p.pic.as_deref())
    {
        Ok(id) => (StatusCode::OK, Json(json!({"ok": true, "id": id}))),
        Err(e) => err(StatusCode::BAD_REQUEST, &e),
    }
}

async fn upload_image(
    State(ctx): State<Arc<MobileCtx>>,
    Path(name): Path<String>,
    body: Bytes,
) -> (StatusCode, Json<Value>) {
    if !crate::server::valid_filename(&name) {
        return err(StatusCode::BAD_REQUEST, "文件名非法");
    }
    if body.is_empty() || body.len() > 3 * 1024 * 1024 {
        return err(StatusCode::BAD_REQUEST, "图片大小非法");
    }
    let is_jpg = body.starts_with(&[0xFF, 0xD8, 0xFF]);
    let is_png = body.starts_with(&[0x89, 0x50, 0x4E, 0x47]);
    if !is_jpg && !is_png {
        return err(StatusCode::BAD_REQUEST, "只接受 JPG/PNG 图片");
    }
    match std::fs::write(ctx.image_dir.join(&name), &body) {
        Ok(()) => (StatusCode::OK, Json(json!({"ok": true}))),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

fn err(status: StatusCode, msg: &str) -> (StatusCode, Json<Value>) {
    (status, Json(json!({ "ok": false, "error": msg })))
}

pub fn mobile_router(ctx: Arc<MobileCtx>) -> Router {
    Router::new()
        .route("/api/m/categories", axum::routing::get(categories))
        .route("/api/m/product", axum::routing::post(add_product))
        .route("/api/m/image/{name}", axum::routing::post(upload_image))
        .layer(axum::extract::DefaultBodyLimit::max(3 * 1024 * 1024))
        .with_state(ctx)
}
