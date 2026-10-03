-- 诊所检查退项与费用调整系统 · 初始化 schema
-- 所有金额单位均为人民币「分」(INTEGER)，避免浮点误差。

CREATE TABLE users (
    id         TEXT PRIMARY KEY,
    name       TEXT NOT NULL,
    role       TEXT NOT NULL CHECK (role IN ('DOCTOR','CASHIER','REPORT_STAFF','PATIENT')),
    token      TEXT NOT NULL UNIQUE,           -- 本地演示用登录令牌
    patient_id TEXT REFERENCES patients(id)
);

CREATE TABLE patients (
    id         TEXT PRIMARY KEY,
    name       TEXT NOT NULL,
    identifier TEXT NOT NULL
);

CREATE TABLE bills (
    id         TEXT PRIMARY KEY,
    patient_id TEXT NOT NULL REFERENCES patients(id),
    bill_no    TEXT NOT NULL UNIQUE,
    status     TEXT NOT NULL DEFAULT 'OPEN',   -- OPEN / CLOSED
    created_at TEXT NOT NULL
);

-- 账单检查项目行：支持套餐(PACKAGE)与套餐子项(PACKAGE_COMPONENT)父子结构
CREATE TABLE bill_items (
    id              TEXT PRIMARY KEY,
    bill_id         TEXT NOT NULL REFERENCES bills(id),
    parent_item_id  TEXT REFERENCES bill_items(id),
    code            TEXT NOT NULL,
    name            TEXT NOT NULL,
    category        TEXT NOT NULL CHECK (category IN ('STANDALONE','PACKAGE','PACKAGE_COMPONENT')),
    unit_price      INTEGER NOT NULL,          -- 实际成交单价(分摊优惠后), 分
    list_unit_price INTEGER NOT NULL,          -- 原价单价(优惠前), 分
    quantity        INTEGER NOT NULL,
    executed_qty    INTEGER NOT NULL DEFAULT 0,
    refunded_qty    INTEGER NOT NULL DEFAULT 0,
    discount_total  INTEGER NOT NULL DEFAULT 0,-- 本行整单优惠分摊, 分
    status          TEXT NOT NULL DEFAULT 'SCHEDULED',
    -- SCHEDULED / EXECUTED / PARTIALLY_EXECUTED / CANCELLED / REFUNDED / PARTIALLY_REFUNDED
    cancel_reason   TEXT,                      -- DOCTOR_CANCEL / EQUIPMENT_FAULT
    cancel_remark   TEXT,
    cancelled_by    TEXT REFERENCES users(id),
    cancelled_at    TEXT,
    report_status   TEXT NOT NULL DEFAULT 'NONE', -- NONE / ISSUED / VOIDED
    sort_no         INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE reports (
    id           TEXT PRIMARY KEY,
    bill_item_id TEXT NOT NULL REFERENCES bill_items(id),
    report_no    TEXT NOT NULL UNIQUE,
    status       TEXT NOT NULL DEFAULT 'ISSUED', -- ISSUED / VOIDED
    issued_by    TEXT NOT NULL REFERENCES users(id),
    issued_at    TEXT NOT NULL,
    voided_at    TEXT
);

-- 报告作废记录，与退款单双向追查
CREATE TABLE report_voids (
    id           TEXT PRIMARY KEY,
    report_id    TEXT NOT NULL REFERENCES reports(id),
    bill_item_id TEXT NOT NULL REFERENCES bill_items(id),
    reason       TEXT NOT NULL,
    voided_by    TEXT NOT NULL REFERENCES users(id),
    refund_id    TEXT REFERENCES refunds(id),
    created_at   TEXT NOT NULL
);

-- 原始付款构成：保险 / 自费
CREATE TABLE bill_payments (
    id         TEXT PRIMARY KEY,
    bill_id    TEXT NOT NULL REFERENCES bills(id),
    method     TEXT NOT NULL CHECK (method IN ('INSURANCE','SELF_PAY')),
    amount     INTEGER NOT NULL,               -- 分
    receipt_no TEXT NOT NULL,
    paid_at    TEXT NOT NULL
);

-- 退款单（退项申请）。calc_detail 保存完整计算明细 JSON。
CREATE TABLE refunds (
    id                 TEXT PRIMARY KEY,
    bill_id            TEXT NOT NULL REFERENCES bills(id),
    bill_item_id       TEXT NOT NULL REFERENCES bill_items(id),
    status             TEXT NOT NULL CHECK (status IN ('AWAITING_VOID','COMPLETED','REJECTED')),
    cancel_reason      TEXT,
    cancel_remark      TEXT,
    refund_qty         INTEGER NOT NULL,
    unit_list_price    INTEGER NOT NULL,
    gross_refund       INTEGER NOT NULL,       -- 原价退款额(优惠前)
    discount_share     INTEGER NOT NULL,       -- 该退项分摊的优惠
    net_refund         INTEGER NOT NULL,       -- 实际退款(剔除优惠后)
    insurance_amount   INTEGER NOT NULL DEFAULT 0, -- 退回保险部分
    self_pay_amount    INTEGER NOT NULL DEFAULT 0, -- 退回自费部分
    payment_ratio      TEXT NOT NULL,          -- {insurance,total} 原始付款构成比例
    calc_detail        TEXT NOT NULL,
    insurance_receipt_no TEXT,                 -- 模拟保险冲账回执
    payment_receipt_no   TEXT,                 -- 模拟自费退款回执
    requested_by       TEXT NOT NULL REFERENCES users(id),
    confirmed_by       TEXT REFERENCES users(id),
    report_void_id     TEXT REFERENCES report_voids(id),
    reject_reason      TEXT,
    created_at         TEXT NOT NULL,
    completed_at       TEXT
);

CREATE INDEX idx_bill_items_bill ON bill_items(bill_id);
CREATE INDEX idx_refunds_bill ON refunds(bill_id);
CREATE INDEX idx_refunds_item ON refunds(bill_item_id);
CREATE INDEX idx_payments_bill ON bill_payments(bill_id);
