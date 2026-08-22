//! 配置与目录约定。
//!
//! 生产目录结构（与 exe 同级）：
//!   cybercafeShop-server.exe / 管理端.exe
//!   config.ini        [server] port=21974
//!   data/db/shop_db.db   商品库 + 订单表
//!   data/db/config.db    店铺配置（店名/欢迎语，独立库）
//!   data/image/          商品图片 300x300 jpg
//!   data/sound/          播报 wav
//!   data/qrcode/         wechat.png / alipay.png
//!   web/m/               手机端添加商品页面（HTTP 托管，仅手机用）
//!
//! 生产/测试隔离：数据根目录可用环境变量 LSWSHOP_DATA_DIR 覆盖，
//! 测试把根目录指到临时目录即可，互不干扰（参考 Landisk 的 LANDISK_DATA_DIR）。

use std::path::{Path, PathBuf};

/// 生产环境固定端口（在 config.ini 中，不通过界面修改）
pub const DEFAULT_PORT: u16 = 21974;

#[derive(Debug, Clone)]
pub struct Config {
    pub port: u16,
}

impl Config {
    /// 从数据根目录下的 config.ini 读取；文件缺失或字段缺失时用默认值。
    pub fn load(base: &Path) -> Config {
        let mut cfg = Config { port: DEFAULT_PORT };
        let path = base.join("config.ini");
        if let Ok(text) = std::fs::read_to_string(&path) {
            if let Some(v) = ini_get(&text, "server", "port") {
                if let Ok(p) = v.parse::<u16>() {
                    if p > 0 {
                        cfg.port = p;
                    }
                }
            }
        }
        cfg
    }
}

/// 极简 ini 解析：支持 [section] 与 key=value，忽略 ; 和 # 注释。
pub fn ini_get<'a>(text: &'a str, section: &str, key: &str) -> Option<&'a str> {
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
                    return Some(v.trim());
                }
            }
        }
    }
    None
}

/// 数据根目录：优先环境变量 LSWSHOP_DATA_DIR（测试用），否则取 exe 所在目录。
#[derive(Debug, Clone)]
pub struct AppDirs {
    pub base: PathBuf,
}

impl AppDirs {
    pub fn from_env() -> AppDirs {
        let base = std::env::var("LSWSHOP_DATA_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                std::env::current_exe()
                    .ok()
                    .and_then(|p| p.parent().map(|p| p.to_path_buf()))
                    .unwrap_or_else(|| PathBuf::from("."))
            });
        AppDirs::new(base)
    }

    pub fn new(base: PathBuf) -> AppDirs {
        AppDirs { base }
    }

    pub fn db_path(&self) -> PathBuf {
        self.base.join("data").join("db").join("shop_db.db")
    }
    pub fn image_dir(&self) -> PathBuf {
        self.base.join("data").join("image")
    }
    pub fn sound_dir(&self) -> PathBuf {
        self.base.join("data").join("sound")
    }
    pub fn qrcode_dir(&self) -> PathBuf {
        self.base.join("data").join("qrcode")
    }
    /// 网页目录：管理端页面内嵌 exe 不走这里；web/m 为手机端添加商品页（HTTP 托管）
    pub fn web_dir(&self) -> PathBuf {
        self.base.join("web")
    }
    /// 手机端添加商品页面目录（/m/ 托管）
    pub fn mobile_dir(&self) -> PathBuf {
        self.web_dir().join("m")
    }

    /// 确保子目录存在（测试环境也会调用）。
    pub fn ensure_dirs(&self) -> std::io::Result<()> {
        for d in ["data/db", "data/image", "data/qrcode", "data/sound", "web/m"] {
            std::fs::create_dir_all(self.base.join(d))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ini_parse_basic() {
        let text = "[server]\nport = 23917\n; comment\n[other]\nx=1\n";
        assert_eq!(ini_get(text, "server", "port"), Some("23917"));
        assert_eq!(ini_get(text, "other", "x"), Some("1"));
        assert_eq!(ini_get(text, "server", "missing"), None);
        assert_eq!(ini_get(text, "nope", "port"), None);
    }

    #[test]
    fn config_default_port_when_missing() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = Config::load(dir.path());
        assert_eq!(cfg.port, DEFAULT_PORT);
    }
}
