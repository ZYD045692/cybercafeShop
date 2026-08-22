//! 数据库层。商品/分类表结构（shop_list / shop_fl），
//! 订单表（orders / order_items）不存在时自动创建。

use rusqlite::{params, Connection};
use serde::Serialize;
use std::path::Path;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Serialize)]
pub struct Category {
    pub name: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Product {
    pub id: i64,
    pub name: String,
    pub class: String,
    pub abbr: String,
    pub price: f64,
    pub pic: String,
    pub sold: i64,
}

#[derive(Debug)]
pub struct OrderItemIn {
    pub id: i64,
    pub qty: i64,
}

/// 管理端商品视图（含进价/上下架状态）
#[derive(Debug, Clone, Serialize)]
pub struct AdminProduct {
    pub id: i64,
    pub name: String,
    pub class: String,
    pub abbr: String,
    pub jhj: f64,
    pub price: f64,
    pub pic: String,
    pub sold: i64,
    pub state: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct OrderSummary {
    pub id: i64,
    pub machine: String,
    pub pay_method: String,
    pub total: f64,
    pub status: i64,
    pub created_at: String,
    pub items: Vec<OrderItemOut>,
}

#[derive(Debug, Clone, Serialize)]
pub struct OrderItemOut {
    pub name: String,
    pub price: f64,
    pub qty: i64,
}

pub const PAY_METHODS: [&str; 3] = ["wechat", "alipay", "cash"];

#[derive(Clone)]
pub struct Db {
    conn: Arc<Mutex<Connection>>,
    /// 店铺配置独立库存放（data\db\config.db）：换商品库/数据包不碰店名和欢迎语
    cfg: Arc<Mutex<Connection>>,
}

impl Db {
    /// 打开（必要时创建）数据库并初始化订单表。
    pub fn open(path: &Path) -> Result<Db, String> {
        if let Some(p) = path.parent() {
            std::fs::create_dir_all(p).map_err(|e| e.to_string())?;
        }
        let conn = Connection::open(path).map_err(|e| format!("打开数据库失败: {e}"))?;
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             CREATE TABLE IF NOT EXISTS orders (
               id INTEGER PRIMARY KEY AUTOINCREMENT,
               machine TEXT NOT NULL,
               pay_method TEXT NOT NULL,
               total REAL NOT NULL,
               status INTEGER NOT NULL DEFAULT 0,
               created_at TEXT NOT NULL DEFAULT (datetime('now','localtime'))
             );
             CREATE TABLE IF NOT EXISTS order_items (
               id INTEGER PRIMARY KEY AUTOINCREMENT,
               order_id INTEGER NOT NULL,
               gds_name TEXT NOT NULL,
               price REAL NOT NULL,
               qty INTEGER NOT NULL
             );
             CREATE INDEX IF NOT EXISTS idx_orders_time ON orders(created_at);
             CREATE INDEX IF NOT EXISTS idx_items_order ON order_items(order_id);
             -- 商品/分类表结构与种子库一致（IF NOT EXISTS：有种子库时是空操作；
             -- 没有种子库的全新环境也不至于缺表 500，只是分类/商品为空）
             CREATE TABLE IF NOT EXISTS shop_fl (
               id INTEGER PRIMARY KEY AUTOINCREMENT,
               class_name TEXT, class_px INTEGER, class_ext_1 TEXT
             );
             CREATE TABLE IF NOT EXISTS shop_list (
               id INTEGER PRIMARY KEY AUTOINCREMENT,
               gds_number TEXT, gds_class TEXT, gds_name TEXT,
               gds_bt_count INTEGER, gds_ck_count INTEGER,
               gds_jhj INTEGER, gds_xsj INTEGER, gds_gys TEXT,
               gds_pic TEXT, gds_px INTEGER, gds_state INTEGER,
               gds_out INTEGER, gds_js TEXT,
               gds_ext_1 TEXT, gds_ext_2 TEXT, gds_ext_3 TEXT
             );",
        )
        .map_err(|e| format!("初始化订单表失败: {e}"))?;

        // 店铺配置独立库（与商品库分开存：换商品数据不影响店名/欢迎语）
        let cfg_path = path.with_file_name("config.db");
        let cfg = Connection::open(&cfg_path).map_err(|e| format!("打开配置库失败: {e}"))?;
        cfg.execute_batch(
            "PRAGMA journal_mode=WAL;
             CREATE TABLE IF NOT EXISTS shop_config (
               key TEXT PRIMARY KEY,
               value TEXT NOT NULL
             );
             INSERT OR IGNORE INTO shop_config(key, value) VALUES
               ('shop_name', '莱尚网电竞馆'),
               ('welcome', '欢迎光临，祝您游戏愉快');",
        )
        .map_err(|e| format!("初始化配置库失败: {e}"))?;
        Ok(Db { conn: Arc::new(Mutex::new(conn)), cfg: Arc::new(Mutex::new(cfg)) })
    }

    pub fn categories(&self) -> Result<Vec<Category>, String> {
        let conn = self.conn.lock().unwrap();
        let mut st = conn
            .prepare("SELECT class_name FROM shop_fl WHERE class_name<>'全部商品' ORDER BY class_px DESC")
            .map_err(|e| e.to_string())?;
        let rows = st
            .query_map([], |r| Ok(Category { name: r.get(0)? }))
            .map_err(|e| e.to_string())?;
        Ok(rows.flatten().collect())
    }

    /// 在售商品列表（顾客端用），带销量。
    pub fn products_on_sale(&self) -> Result<Vec<Product>, String> {
        let conn = self.conn.lock().unwrap();
        let mut st = conn
            .prepare(
                "SELECT id, gds_name, gds_class, gds_number, gds_xsj, gds_pic, gds_out
                 FROM shop_list WHERE gds_state=1
                 ORDER BY gds_px DESC, gds_out DESC, id",
            )
            .map_err(|e| e.to_string())?;
        let rows = st
            .query_map([], |r| {
                Ok(Product {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    class: r.get(2)?,
                    abbr: r.get(3)?,
                    price: r.get(4)?,
                    pic: r.get(5)?,
                    sold: r.get(6)?,
                })
            })
            .map_err(|e| e.to_string())?;
        Ok(rows.flatten().collect())
    }

    /// 下单。金额一律以数据库售价为准（忽略客户端报的任何价格），事务写入，并累加销量。
    /// 返回 (订单id, 合计金额)。
    pub fn place_order(
        &self,
        machine: &str,
        pay_method: &str,
        items: &[OrderItemIn],
    ) -> Result<(i64, f64), String> {
        if machine.is_empty() || machine.chars().count() > 64 {
            return Err("机器名非法".into());
        }
        if !PAY_METHODS.contains(&pay_method) {
            return Err("支付方式非法".into());
        }
        if items.is_empty() || items.len() > 50 {
            return Err("商品条目数量非法".into());
        }
        for it in items {
            if it.qty < 1 || it.qty > 99 {
                return Err(format!("商品 {} 数量非法", it.id));
            }
        }

        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction().map_err(|e| e.to_string())?;
        let mut total = 0.0f64;
        let mut lines: Vec<(String, f64, i64)> = Vec::new();
        for it in items {
            // 必须存在且在售；价格从库里取
            let row: Result<(String, f64), _> = tx.query_row(
                "SELECT gds_name, gds_xsj FROM shop_list WHERE id=?1 AND gds_state=1",
                params![it.id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            );
            let (name, price) = match row {
                Ok(v) => v,
                Err(_) => return Err(format!("商品 {} 不存在或已下架", it.id)),
            };
            total += price * it.qty as f64;
            lines.push((name, price, it.qty));
        }
        tx.execute(
            "INSERT INTO orders(machine, pay_method, total) VALUES (?1,?2,?3)",
            params![machine, pay_method, total],
        )
        .map_err(|e| e.to_string())?;
        let order_id = tx.last_insert_rowid();
        for (name, price, qty) in &lines {
            tx.execute(
                "INSERT INTO order_items(order_id, gds_name, price, qty) VALUES (?1,?2,?3,?4)",
                params![order_id, name, price, qty],
            )
            .map_err(|e| e.to_string())?;
        }
        for it in items {
            tx.execute(
                "UPDATE shop_list SET gds_out = gds_out + ?2 WHERE id=?1",
                params![it.id, it.qty],
            )
            .map_err(|e| e.to_string())?;
        }
        tx.commit().map_err(|e| e.to_string())?;
        Ok((order_id, total))
    }

    /// 订单列表（管理端）。status: None=全部, Some(0)=未处理, Some(1)=已处理。
    pub fn orders(&self, status: Option<i64>, limit: i64) -> Result<Vec<OrderSummary>, String> {
        let conn = self.conn.lock().unwrap();
        let sql = match status {
            Some(_) => {
                "SELECT id, machine, pay_method, total, status, created_at FROM orders
                 WHERE status=?1 ORDER BY status ASC, id DESC LIMIT ?2"
            }
            None => {
                "SELECT id, machine, pay_method, total, status, created_at FROM orders
                 ORDER BY status ASC, id DESC LIMIT ?1"
            }
        };
        let mut st = conn.prepare(sql).map_err(|e| e.to_string())?;
        let map_row = |r: &rusqlite::Row| -> rusqlite::Result<(i64, String, String, f64, i64, String)> {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?))
        };
        let raw: Vec<_> = match status {
            Some(s) => st
                .query_map(params![s, limit], map_row)
                .map_err(|e| e.to_string())?,
            None => st
                .query_map(params![limit], map_row)
                .map_err(|e| e.to_string())?,
        }
        .flatten()
        .collect();

        let mut out = Vec::new();
        let mut ist = conn
            .prepare("SELECT gds_name, price, qty FROM order_items WHERE order_id=?1")
            .map_err(|e| e.to_string())?;
        for (id, machine, pay_method, total, st_status, created_at) in raw {
            let items: Vec<OrderItemOut> = ist
                .query_map(params![id], |r| {
                    Ok(OrderItemOut { name: r.get(0)?, price: r.get(1)?, qty: r.get(2)? })
                })
                .map_err(|e| e.to_string())?
                .flatten()
                .collect();
            out.push(OrderSummary { id, machine, pay_method, total, status: st_status, created_at, items });
        }
        Ok(out)
    }

    pub fn set_order_status(&self, order_id: i64, status: i64) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        let n = conn
            .execute("UPDATE orders SET status=?2 WHERE id=?1", params![order_id, status])
            .map_err(|e| e.to_string())?;
        if n == 0 {
            return Err("订单不存在".into());
        }
        Ok(())
    }

    pub fn pending_count(&self) -> Result<i64, String> {
        let conn = self.conn.lock().unwrap();
        conn.query_row("SELECT COUNT(*) FROM orders WHERE status=0", [], |r| r.get(0))
            .map_err(|e| e.to_string())
    }

    /// 店铺信息（店名 + 欢迎语），用户端顶部 header 展示用。独立存在 config.db。
    pub fn shop_info(&self) -> Result<(String, String), String> {
        let cfg = self.cfg.lock().unwrap();
        let get = |k: &str| -> String {
            cfg.query_row("SELECT value FROM shop_config WHERE key=?1", params![k], |r| r.get(0))
                .unwrap_or_default()
        };
        Ok((get("shop_name"), get("welcome")))
    }

    pub fn set_shop_info(&self, name: &str, welcome: &str) -> Result<(), String> {
        let cfg = self.cfg.lock().unwrap();
        cfg.execute("INSERT INTO shop_config(key,value) VALUES('shop_name',?1)
                      ON CONFLICT(key) DO UPDATE SET value=excluded.value", params![name])
            .map_err(|e| e.to_string())?;
        cfg.execute("INSERT INTO shop_config(key,value) VALUES('welcome',?1)
                      ON CONFLICT(key) DO UPDATE SET value=excluded.value", params![welcome])
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    // ---------------- 管理端 ----------------

    pub fn admin_products(&self) -> Result<Vec<AdminProduct>, String> {
        let conn = self.conn.lock().unwrap();
        let mut st = conn
            .prepare(
                "SELECT id, gds_name, gds_class, gds_number, gds_jhj, gds_xsj, gds_pic, gds_out, gds_state
                 FROM shop_list ORDER BY gds_px DESC, gds_out DESC, id",
            )
            .map_err(|e| e.to_string())?;
        let rows = st
            .query_map([], |r| {
                Ok(AdminProduct {
                    id: r.get(0)?, name: r.get(1)?, class: r.get(2)?, abbr: r.get(3)?,
                    jhj: r.get(4)?, price: r.get(5)?, pic: r.get(6)?, sold: r.get(7)?, state: r.get(8)?,
                })
            })
            .map_err(|e| e.to_string())?;
        Ok(rows.flatten().collect())
    }

    /// 新增（id=None）或修改商品。图片文件名沿用缩拼。
    #[allow(clippy::too_many_arguments)]
    pub fn upsert_product(
        &self,
        id: Option<i64>,
        name: &str,
        class: &str,
        abbr: &str,
        jhj: f64,
        price: f64,
        pic: Option<&str>,
    ) -> Result<i64, String> {
        let conn = self.conn.lock().unwrap();
        // 缩拼由前端生成后上传，后端原样存储（不再自动生成）
        // 分类必须存在
        let exists: i64 = conn
            .query_row("SELECT COUNT(*) FROM shop_fl WHERE class_name=?1", params![class], |r| r.get(0))
            .map_err(|e| e.to_string())?;
        if exists == 0 {
            return Err(format!("分类 {class} 不存在"));
        }
        match id {
            Some(id) => {
                let n = match pic {
                    Some(pic) => conn.execute(
                        "UPDATE shop_list SET gds_name=?2,gds_class=?3,gds_number=?4,gds_jhj=?5,gds_xsj=?6,gds_pic=?7 WHERE id=?1",
                        params![id, name, class, abbr, jhj, price, pic],
                    ),
                    None => conn.execute(
                        "UPDATE shop_list SET gds_name=?2,gds_class=?3,gds_number=?4,gds_jhj=?5,gds_xsj=?6 WHERE id=?1",
                        params![id, name, class, abbr, jhj, price],
                    ),
                }
                .map_err(|e| e.to_string())?;
                if n == 0 {
                    return Err("商品不存在".into());
                }
                Ok(id)
            }
            None => {
                let pic = pic.unwrap_or("");
                // 缩拼冲突唯一化：abbr 已存在则追加 _1/_2/_3...（图片以 abbr 命名，需唯一避免互覆）
                // 例：whh 被占 → whh_1；whh_1 被占 → whh_2
                let mut final_abbr = abbr.to_string();
                if !final_abbr.is_empty() {
                    let mut n = 1u64;
                    loop {
                        let cnt: i64 = conn
                            .query_row(
                                "SELECT COUNT(*) FROM shop_list WHERE gds_number=?1",
                                params![final_abbr],
                                |r| r.get(0),
                            )
                            .map_err(|e| e.to_string())?;
                        if cnt == 0 {
                            break;
                        }
                        final_abbr = format!("{abbr}_{n}");
                        n += 1;
                    }
                }
                conn.execute(
                    "INSERT INTO shop_list (gds_number,gds_class,gds_name,gds_bt_count,gds_ck_count,gds_jhj,gds_xsj,gds_gys,gds_pic,gds_px,gds_state,gds_out,gds_js)
                     VALUES (?1,?2,?3,99999999,99999999,?4,?5,'默认',?6,6,1,0,'')",
                    params![final_abbr, class, name, jhj, price, pic],
                )
                .map_err(|e| e.to_string())?;
                Ok(conn.last_insert_rowid())
            }
        }
    }

    /// 按 id 取商品最终缩拼（手机端：建商品后用它命名图片，保证图随缩拼唯一）
    pub fn product_abbr(&self, id: i64) -> Result<String, String> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT gds_number FROM shop_list WHERE id=?1",
            params![id],
            |r| r.get(0),
        )
        .map_err(|_| "商品不存在".to_string())
    }

    /// 回填商品图片文件名（手机端：图片在商品建好后才上传）
    pub fn set_product_pic(&self, id: i64, pic: &str) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        let n = conn
            .execute(
                "UPDATE shop_list SET gds_pic=?2 WHERE id=?1",
                params![id, pic],
            )
            .map_err(|e| e.to_string())?;
        if n == 0 {
            return Err("商品不存在".into());
        }
        Ok(())
    }

    pub fn set_product_state(&self, id: i64, state: i64) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        let n = conn
            .execute("UPDATE shop_list SET gds_state=?2 WHERE id=?1", params![id, state])
            .map_err(|e| e.to_string())?;
        if n == 0 {
            return Err("商品不存在".into());
        }
        Ok(())
    }

    pub fn delete_product(&self, id: i64) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        let n = conn
            .execute("DELETE FROM shop_list WHERE id=?1", params![id])
            .map_err(|e| e.to_string())?;
        if n == 0 {
            return Err("商品不存在".into());
        }
        Ok(())
    }

    pub fn add_category(&self, name: &str) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        let exists: i64 = conn
            .query_row("SELECT COUNT(*) FROM shop_fl WHERE class_name=?1", params![name], |r| r.get(0))
            .map_err(|e| e.to_string())?;
        if exists > 0 {
            return Err("分类已存在".into());
        }
        // 排到最后：比当前最小排序值再小 1（'全部商品' 固定 100 不参与）
        let min_px: i64 = conn
            .query_row(
                "SELECT COALESCE(MIN(class_px),1) FROM shop_fl WHERE class_name<>'全部商品'",
                [],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT INTO shop_fl (class_name, class_px) VALUES (?1, ?2)",
            params![name, min_px - 1],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn rename_category(&self, old: &str, new: &str) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        let n = conn
            .execute("UPDATE shop_fl SET class_name=?2 WHERE class_name=?1", params![old, new])
            .map_err(|e| e.to_string())?;
        if n == 0 {
            return Err("分类不存在".into());
        }
        // 同步商品表里的分类名
        conn.execute("UPDATE shop_list SET gds_class=?2 WHERE gds_class=?1", params![old, new])
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn delete_category(&self, name: &str) -> Result<(), String> {
        if name == "全部商品" {
            return Err("系统分类不可删除".into());
        }
        let conn = self.conn.lock().unwrap();
        let used: i64 = conn
            .query_row("SELECT COUNT(*) FROM shop_list WHERE gds_class=?1", params![name], |r| r.get(0))
            .map_err(|e| e.to_string())?;
        if used > 0 {
            return Err(format!("分类下还有 {used} 个商品，不能删除"));
        }
        conn.execute("DELETE FROM shop_fl WHERE class_name=?1 AND class_name<>'全部商品'", params![name])
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// 销售记录：按日期范围（YYYY-MM-DD，闭区间）和支付方式筛选，返回列表+合计。
    pub fn records(
        &self,
        from: Option<&str>,
        to: Option<&str>,
        pay: Option<&str>,
    ) -> Result<(Vec<OrderSummary>, f64), String> {
        let conn = self.conn.lock().unwrap();
        let mut sql = String::from(
            "SELECT id, machine, pay_method, total, status, created_at FROM orders WHERE 1=1",
        );
        let mut bind: Vec<String> = Vec::new();
        if let Some(f) = from {
            sql.push_str(" AND date(created_at) >= ?");
            bind.push(f.to_string());
        }
        if let Some(t) = to {
            sql.push_str(" AND date(created_at) <= ?");
            bind.push(t.to_string());
        }
        if let Some(p) = pay {
            sql.push_str(" AND pay_method = ?");
            bind.push(p.to_string());
        }
        sql.push_str(" ORDER BY id DESC LIMIT 1000");
        let mut st = conn.prepare(&sql).map_err(|e| e.to_string())?;
        let params: Vec<&dyn rusqlite::ToSql> =
            bind.iter().map(|s| s as &dyn rusqlite::ToSql).collect();
        let raw: Vec<(i64, String, String, f64, i64, String)> = st
            .query_map(params.as_slice(), |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?))
            })
            .map_err(|e| e.to_string())?
            .flatten()
            .collect();

        let mut out = Vec::new();
        let mut sum = 0.0;
        let mut ist = conn
            .prepare("SELECT gds_name, price, qty FROM order_items WHERE order_id=?1")
            .map_err(|e| e.to_string())?;
        for (id, machine, pay_method, total, status, created_at) in raw {
            sum += total;
            let items: Vec<OrderItemOut> = ist
                .query_map(params![id], |r| {
                    Ok(OrderItemOut { name: r.get(0)?, price: r.get(1)?, qty: r.get(2)? })
                })
                .map_err(|e| e.to_string())?
                .flatten()
                .collect();
            out.push(OrderSummary { id, machine, pay_method, total, status, created_at, items });
        }
        Ok((out, (sum * 100.0).round() / 100.0))
    }
}
