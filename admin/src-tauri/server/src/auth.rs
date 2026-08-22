//! 访问门禁：HMAC-SHA256 时间票签名。
//!
//! 设计要点：
//! - 密钥只编译在管理端/用户端 exe 的 Rust 里，绝不下发到网页文件；
//!   局域网里用浏览器裸开 IP:端口 → 403，什么都看不到。
//! - 票据 = HMAC(secret, 时间戳)，±300 秒有效；网页 JS 的密钥由壳通过
//!   initialization_script 注入内存（不进任何静态文件）。
//! - 网吧客户机时钟可能不准：/api/ping 返回服务器时间，客户端先对时（算偏移），
//!   再用偏移后的时间签名，时钟错乱也能用。
//! - dev 模式（debug 构建）不启用门禁，方便 vite 热重载与浏览器调试。

use axum::body::Body;
use axum::extract::{ConnectInfo, State};
use axum::http::{header, Request, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::Json;
use hmac::{Hmac, Mac};
use sha2::Sha256;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

type HmacSha256 = Hmac<Sha256>;

/// 访问密钥（编译进 exe；用户端壳里也有一份相同的）。
/// 换密钥 = 改这里和 client/src-tauri/src/lib.rs 里的 ACCESS_KEY，重新编译两端。
pub const ACCESS_KEY: &[u8] = b"cybercafeShop-6f2a9c4e8b1d4a7f-k53p9q2w7e0r6t5y8";

/// 票据有效窗口（秒）。客户机已对时，300 秒足够宽松。
pub const TICKET_WINDOW_SECS: i64 = 300;

pub fn now_ts() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// 生成时间戳签名（Rust 侧：壳签名 /api/ping 与 shop 页面 URL 用）。
pub fn sign(ts: i64) -> String {
    let mut mac = <HmacSha256 as Mac>::new_from_slice(ACCESS_KEY).expect("HMAC 初始化失败");
    mac.update(ts.to_string().as_bytes());
    hex::encode(mac.finalize().into_bytes())
}

/// 校验 (ts, sig)：时间窗内且签名一致。
pub fn verify(ts: i64, sig: &str) -> bool {
    if (now_ts() - ts).abs() > TICKET_WINDOW_SECS {
        return false;
    }
    let mut mac = <HmacSha256 as Mac>::new_from_slice(ACCESS_KEY).expect("HMAC 初始化失败");
    mac.update(ts.to_string().as_bytes());
    let expect = hex::encode(mac.finalize().into_bytes());
    // 定长 hex 字符串，直接比较即可
    expect == sig
}

/// 门禁模式
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthMode {
    /// dev/测试：放行一切
    Off,
    /// 严格：一律要求时间票（测试 403 行为用）
    Ticket,
    /// 生产：本机回环放行（管理端页面天然免票），外网卡必须带票
    TicketOrLocalhost,
}

use crate::server::AppState;

/// 请求中提取 (ts, sig)：优先 header（x-lsw-ts / x-lsw-sig），退回 query（图片等 <img> 场景）。
fn extract_ticket(req: &Request<Body>) -> Option<(i64, String)> {
    let h = req.headers();
    let from_header = || {
        let ts = h.get("x-lsw-ts")?.to_str().ok()?.parse::<i64>().ok()?;
        let sig = h.get("x-lsw-sig")?.to_str().ok()?.to_string();
        Some((ts, sig))
    };
    if let Some(v) = from_header() {
        return Some(v);
    }
    let q = req.uri().query()?;
    let mut ts = None;
    let mut sig = None;
    for kv in q.split('&') {
        if let Some((k, v)) = kv.split_once('=') {
            match k {
                "ts" => ts = v.parse::<i64>().ok(),
                "sig" => sig = Some(v.to_string()),
                _ => {}
            }
        }
    }
    Some((ts?, sig?))
}

/// 门禁中间件：无票/假票一律 403；生产模式下本机回环免票（管理端页面直接用）。
pub async fn require_ticket(
    State(st): State<Arc<AppState>>,
    req: Request<Body>,
    next: Next,
) -> Response {
    match st.auth {
        AuthMode::Off => return next.run(req).await,
        AuthMode::TicketOrLocalhost => {
            // ConnectInfo 由 into_make_service_with_connect_info 注入
            let is_local = req
                .extensions()
                .get::<ConnectInfo<SocketAddr>>()
                .map(|c| c.0.ip().is_loopback())
                .unwrap_or(false);
            if is_local {
                return next.run(req).await; // 本机请求（管理端页面）免票
            }
        }
        AuthMode::Ticket => {}
    }
    let ok = extract_ticket(&req).map(|(ts, sig)| verify(ts, &sig)).unwrap_or(false);
    if ok {
        next.run(req).await
    } else {
        (
            StatusCode::FORBIDDEN,
            [(header::CONTENT_TYPE, "application/json")],
            Json(serde_json::json!({"ok":false,"error":"访问被拒绝"})),
        )
            .into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ticket_roundtrip() {
        let ts = now_ts();
        let sig = sign(ts);
        assert!(verify(ts, &sig));
        assert!(!verify(ts, "deadbeef"));
        assert!(!verify(ts + 10000, &sign(ts + 10000))); // 超窗
    }

    #[test]
    fn sign_matches_reference_vector() {
        // 与 Python hmac.new(key, b"1755780000", sha256).hexdigest() 及前端 hmac.js 对拍
        // 注：密钥前缀 cybercafeShop- 时向量 = 7524e645...；换密钥要重算这里
        assert_eq!(
            sign(1755780000),
            "7524e645af266bbb472241b5d5e8865b3d97a8a5ae54573aa0506658fc6da199"
        );
    }
}
