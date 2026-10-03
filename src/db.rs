use crate::money::split_proportional;
use sha2::{Digest, Sha256};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use std::str::FromStr;

pub fn password_hash(password: &str) -> String {
    let mut h = Sha256::new();
    h.update(password.as_bytes());
    format!("{:x}", h.finalize())
}

/// 初始化连接池并执行迁移。SQLite 单连接串行化，天然避免写冲突。
pub async fn init_pool(db_url: &str) -> Result<SqlitePool, sqlx::Error> {
    let opts = SqliteConnectOptions::from_str(db_url)?.create_if_missing(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(opts)
        .await?;
    migrate(&pool).await?;
    Ok(pool)
}

pub async fn migrate(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    let sql = include_str!("../migrations/0001_init.sql");
    for stmt in sql.split(';') {
        let stmt = stmt.trim();
        if !stmt.is_empty() {
            sqlx::query(stmt).execute(pool).await?;
        }
    }
    Ok(())
}

/// 首次运行写入演示数据（已有数据则跳过）。
pub async fn seed(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    let (n,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users")
        .fetch_one(pool)
        .await?;
    if n > 0 {
        return Ok(());
    }

    // 员工账号（演示密码均为 123456）
    let users = [
        ("doctor", "张医生", "doctor"),
        ("cashier", "李收费", "cashier"),
        ("reporter", "王报告", "reporter"),
        ("admin", "系统管理员", "admin"),
    ];
    for (username, display, role) in users {
        sqlx::query("INSERT INTO users (username, password_hash, display_name, role) VALUES (?,?,?,?)")
            .bind(username)
            .bind(password_hash("123456"))
            .bind(display)
            .bind(role)
            .execute(pool)
            .await?;
    }

    // 患者
    let patients = [("陈小明", "男", 34), ("刘芳", "女", 28), ("王建国", "男", 57)];
    for (name, gender, age) in patients {
        sqlx::query("INSERT INTO patients (name, gender, age) VALUES (?,?,?)")
            .bind(name)
            .bind(gender)
            .bind(age)
            .execute(pool)
            .await?;
    }

    // 套餐模板
    let packages = [
        (
            "全身体检套餐",
            80000i64,
            r#"[{"code":"BC","name":"血常规","list_price_cents":10000},{"code":"UC","name":"尿常规","list_price_cents":6000},{"code":"LF","name":"肝功能","list_price_cents":20000},{"code":"ECG","name":"心电图","list_price_cents":15000},{"code":"CT","name":"胸部CT","list_price_cents":60000}]"#,
        ),
        (
            "入职体检套餐",
            30000i64,
            r#"[{"code":"BC","name":"血常规","list_price_cents":10000},{"code":"UC","name":"尿常规","list_price_cents":6000},{"code":"ECG","name":"心电图","list_price_cents":15000},{"code":"XR","name":"胸片","list_price_cents":9000}]"#,
        ),
    ];
    for (name, price, items_json) in packages {
        sqlx::query("INSERT INTO packages (name, price_cents, items_json) VALUES (?,?,?)")
            .bind(name)
            .bind(price)
            .bind(items_json)
            .execute(pool)
            .await?;
    }

    // ---------- 账单 1：陈小明 —— 套餐 + 单项，保险 60% / 现金 40%，部分项目已执行 ----------
    create_bill(
        pool,
        1,
        "B20261001-001",
        &[
            // (code, name, list, discount, package_id, package_name, status, cancel_flag, cancel_note)
            ("BC", "血常规", 10000, 2793, Some(1), Some("全身体检套餐"), "reported", None, None),
            ("UC", "尿常规", 6000, 1676, Some(1), Some("全身体检套餐"), "executed", None, None),
            ("LF", "肝功能", 20000, 5585, Some(1), Some("全身体检套餐"), "pending", Some("doctor_cancel"), Some("医生评估后取消该项检查")),
            ("ECG", "心电图", 15000, 4189, Some(1), Some("全身体检套餐"), "pending", Some("equipment_failure"), Some("心电图机故障，今日无法检查")),
            ("CT", "胸部CT", 60000, 16757, Some(1), Some("全身体检套餐"), "pending", None, None),
            ("US", "腹部彩超", 18000, 0, None, None, "pending", None, None),
            ("TM", "肿瘤标志物", 26000, 2000, None, None, "pending", None, None),
        ],
        &[("insurance", 73200), ("cash", 48800)],
    )
    .await?;

    // 血常规已出报告（报告员王报告签发）
    sqlx::query("INSERT INTO reports (bill_item_id, bill_id, summary, issued_by) VALUES (1, 1, '血常规：各项指标大致正常。', 3)")
        .execute(pool)
        .await?;

    // ---------- 账单 2：刘芳 —— 单项 + 单项优惠，微信全额 ----------
    create_bill(
        pool,
        2,
        "B20261002-001",
        &[
            ("MR", "头颅磁共振", 70000, 0, None, None, "executed", None, None),
            ("GS", "胃镜", 35000, 5000, None, None, "pending", None, None),
            ("HP", "幽门螺杆菌", 12000, 0, None, None, "pending", None, None),
        ],
        &[("wechat", 112000)],
    )
    .await?;

    // ---------- 账单 3：王建国 —— 保险 80% / 支付宝 20%，全部待执行 ----------
    create_bill(
        pool,
        3,
        "B20261003-001",
        &[
            ("Holter", "动态心电图", 26000, 0, None, None, "pending", None, None),
            ("ECHO", "心脏彩超", 32000, 0, None, None, "pending", None, None),
        ],
        &[("insurance", 46400), ("alipay", 11600)],
    )
    .await?;

    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn create_bill(
    pool: &SqlitePool,
    patient_id: i64,
    bill_no: &str,
    items: &[(
        &str,
        &str,
        i64,
        i64,
        Option<i64>,
        Option<&str>,
        &str,
        Option<&str>,
        Option<&str>,
    )],
    payments: &[(&str, i64)],
) -> Result<i64, sqlx::Error> {
    let total_list: i64 = items.iter().map(|i| i.2).sum();
    let total_discount: i64 = items.iter().map(|i| i.3).sum();
    let total_payable = total_list - total_discount;
    let total_paid: i64 = payments.iter().map(|p| p.1).sum();
    debug_assert_eq!(total_payable, total_paid, "种子数据实收必须等于应付");

    let bill_id: i64 = sqlx::query(
        "INSERT INTO bills (bill_no, patient_id, total_list_cents, total_discount_cents, total_payable_cents, total_paid_cents)
         VALUES (?,?,?,?,?,?)",
    )
    .bind(bill_no)
    .bind(patient_id)
    .bind(total_list)
    .bind(total_discount)
    .bind(total_payable)
    .bind(total_paid)
    .execute(pool)
    .await?
    .last_insert_rowid();

    for (code, name, list, discount, pkg_id, pkg_name, status, flag, note) in items {
        sqlx::query(
            "INSERT INTO bill_items (bill_id, code, name, list_price_cents, discount_cents, payable_cents, package_id, package_name, status, cancel_flag, cancel_note)
             VALUES (?,?,?,?,?,?,?,?,?,?,?)",
        )
        .bind(bill_id)
        .bind(code)
        .bind(name)
        .bind(list)
        .bind(discount)
        .bind(list - discount)
        .bind(pkg_id)
        .bind(pkg_name)
        .bind(status)
        .bind(flag)
        .bind(note)
        .execute(pool)
        .await?;
    }

    for (method, amount) in payments {
        sqlx::query("INSERT INTO payments (bill_id, method, amount_cents, receipt_no) VALUES (?,?,?,?)")
            .bind(bill_id)
            .bind(method)
            .bind(amount)
            .bind(mock_receipt_no(method, "P"))
            .execute(pool)
            .await?;
    }
    Ok(bill_id)
}

/// 本地模拟支付/退款渠道回执号
pub fn mock_receipt_no(method: &str, kind: &str) -> String {
    let prefix = match method {
        "insurance" => "INS",
        "cash" => "CSH",
        "wechat" => "WXP",
        "alipay" => "ALP",
        _ => "PAY",
    };
    let id = uuid::Uuid::new_v4().simple().to_string();
    format!("{}-{}-{}", prefix, kind, &id[..8].to_uppercase())
}

/// 套餐价按项目原价比例分摊，返回各项目应付（分），合计严格等于套餐价。
pub fn allocate_package(package_price: i64, list_prices: &[i64]) -> Vec<i64> {
    split_proportional(package_price, list_prices).expect("套餐分摊：原价合计必须为正")
}
