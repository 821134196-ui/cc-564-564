-- 诊所检查项目退项与费用调整系统 —— 数据库结构
-- 金额一律以“分”(cents) 存储，避免浮点误差。

PRAGMA foreign_keys = ON;

-- 员工账号：医生 / 收费员 / 报告处理员 / 管理员
CREATE TABLE IF NOT EXISTS users (
  id            INTEGER PRIMARY KEY AUTOINCREMENT,
  username      TEXT NOT NULL UNIQUE,
  password_hash TEXT NOT NULL,
  display_name  TEXT NOT NULL,
  role          TEXT NOT NULL CHECK (role IN ('doctor','cashier','reporter','admin'))
);

-- 登录会话（本地模拟 token）
CREATE TABLE IF NOT EXISTS sessions (
  token      TEXT PRIMARY KEY,
  user_id    INTEGER NOT NULL REFERENCES users(id),
  created_at TEXT NOT NULL DEFAULT (datetime('now','localtime'))
);

CREATE TABLE IF NOT EXISTS patients (
  id     INTEGER PRIMARY KEY AUTOINCREMENT,
  name   TEXT NOT NULL,
  gender TEXT NOT NULL DEFAULT '',
  age    INTEGER NOT NULL DEFAULT 0
);

-- 套餐模板：items_json 为 [{code,name,list_price_cents}]，套餐价按原价比例分摊到各项目
CREATE TABLE IF NOT EXISTS packages (
  id          INTEGER PRIMARY KEY AUTOINCREMENT,
  name        TEXT NOT NULL,
  price_cents INTEGER NOT NULL,
  items_json  TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS bills (
  id                   INTEGER PRIMARY KEY AUTOINCREMENT,
  bill_no              TEXT NOT NULL UNIQUE,
  patient_id           INTEGER NOT NULL REFERENCES patients(id),
  status               TEXT NOT NULL DEFAULT 'paid'
                       CHECK (status IN ('paid','partially_refunded','fully_refunded')),
  total_list_cents     INTEGER NOT NULL,   -- 原价合计
  total_discount_cents INTEGER NOT NULL,   -- 优惠合计（含套餐分摊优惠）
  total_payable_cents  INTEGER NOT NULL,   -- 应付合计 = 原价 - 优惠
  total_paid_cents     INTEGER NOT NULL,   -- 实收合计（= 各支付方式之和）
  total_refunded_cents INTEGER NOT NULL DEFAULT 0,
  created_at           TEXT NOT NULL DEFAULT (datetime('now','localtime'))
);

CREATE TABLE IF NOT EXISTS bill_items (
  id               INTEGER PRIMARY KEY AUTOINCREMENT,
  bill_id          INTEGER NOT NULL REFERENCES bills(id),
  code             TEXT NOT NULL,
  name             TEXT NOT NULL,
  list_price_cents INTEGER NOT NULL,            -- 原价
  discount_cents   INTEGER NOT NULL DEFAULT 0,  -- 优惠分摊（套餐价差或单项优惠）
  payable_cents    INTEGER NOT NULL,            -- 应付 = 原价 - 优惠分摊
  package_id       INTEGER REFERENCES packages(id),
  package_name     TEXT,                        -- 冗余快照，便于展示
  status           TEXT NOT NULL DEFAULT 'pending'
                   CHECK (status IN ('pending','executed','reported','refunded')),
  cancel_flag      TEXT,                        -- 医生标记：doctor_cancel / equipment_failure
  cancel_note      TEXT,
  refunded_cents   INTEGER NOT NULL DEFAULT 0,
  created_at       TEXT NOT NULL DEFAULT (datetime('now','localtime'))
);

-- 原付款构成（保险 / 现金 / 微信 / 支付宝，本地模拟回执）
CREATE TABLE IF NOT EXISTS payments (
  id           INTEGER PRIMARY KEY AUTOINCREMENT,
  bill_id      INTEGER NOT NULL REFERENCES bills(id),
  method       TEXT NOT NULL CHECK (method IN ('insurance','cash','wechat','alipay')),
  amount_cents INTEGER NOT NULL,
  receipt_no   TEXT NOT NULL,
  created_at   TEXT NOT NULL DEFAULT (datetime('now','localtime'))
);

-- 检查报告：作废记录与退款记录互相引用，可双向追查
CREATE TABLE IF NOT EXISTS reports (
  id           INTEGER PRIMARY KEY AUTOINCREMENT,
  bill_item_id INTEGER NOT NULL REFERENCES bill_items(id),
  bill_id      INTEGER NOT NULL REFERENCES bills(id),
  status       TEXT NOT NULL DEFAULT 'issued' CHECK (status IN ('issued','voided')),
  summary      TEXT NOT NULL DEFAULT '',
  issued_by    INTEGER NOT NULL REFERENCES users(id),
  issued_at    TEXT NOT NULL DEFAULT (datetime('now','localtime')),
  voided_by    INTEGER REFERENCES users(id),
  voided_at    TEXT,
  void_reason  TEXT,
  refund_id    INTEGER                        -- 作废后若发生退款，回填退款单号
);

CREATE TABLE IF NOT EXISTS refunds (
  id              INTEGER PRIMARY KEY AUTOINCREMENT,
  refund_no       TEXT NOT NULL UNIQUE,
  bill_id         INTEGER NOT NULL REFERENCES bills(id),
  reason          TEXT NOT NULL CHECK (reason IN
                  ('doctor_cancel','equipment_failure','patient_request','report_voided')),
  reason_detail   TEXT NOT NULL DEFAULT '',
  operator_id     INTEGER NOT NULL REFERENCES users(id),
  status          TEXT NOT NULL DEFAULT 'success' CHECK (status IN ('success')),
  total_cents     INTEGER NOT NULL,
  idempotency_key TEXT UNIQUE,                -- 同一键重复提交只成功一次
  created_at      TEXT NOT NULL DEFAULT (datetime('now','localtime'))
);

-- 退款明细：每个检查项目全系统最多成功退一次（唯一约束兜底）
CREATE TABLE IF NOT EXISTS refund_items (
  id             INTEGER PRIMARY KEY AUTOINCREMENT,
  refund_id      INTEGER NOT NULL REFERENCES refunds(id),
  bill_item_id   INTEGER NOT NULL UNIQUE REFERENCES bill_items(id),
  amount_cents   INTEGER NOT NULL,
  calc_detail    TEXT NOT NULL,               -- JSON 计算明细（原价/优惠分摊/支付拆分）
  report_void_id INTEGER REFERENCES reports(id) -- 经报告作废通道退款时关联作废记录
);

-- 退款按原付款构成退回各渠道（含模拟渠道回执号）
CREATE TABLE IF NOT EXISTS refund_components (
  id           INTEGER PRIMARY KEY AUTOINCREMENT,
  refund_id    INTEGER NOT NULL REFERENCES refunds(id),
  method       TEXT NOT NULL,
  amount_cents INTEGER NOT NULL,
  receipt_no   TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS audit_logs (
  id         INTEGER PRIMARY KEY AUTOINCREMENT,
  actor_id   INTEGER,
  actor_name TEXT,
  actor_role TEXT,
  action     TEXT NOT NULL,
  entity     TEXT NOT NULL,
  entity_id  INTEGER,
  detail     TEXT NOT NULL DEFAULT '',
  created_at TEXT NOT NULL DEFAULT (datetime('now','localtime'))
);

CREATE INDEX IF NOT EXISTS idx_bill_items_bill ON bill_items(bill_id);
CREATE INDEX IF NOT EXISTS idx_payments_bill  ON payments(bill_id);
CREATE INDEX IF NOT EXISTS idx_refunds_bill   ON refunds(bill_id);
CREATE INDEX IF NOT EXISTS idx_reports_item   ON reports(bill_item_id);
