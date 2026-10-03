//! 演示用种子数据：患者、三个岗位用户、一张含套餐/单项/部分执行场景的账单，
//! 保险与自费混合支付。

use crate::error::AppResult;
use sqlx::SqlitePool;
use uuid::Uuid;

fn id(prefix: &str) -> String {
    format!("{prefix}_{}", Uuid::new_v4().simple())
}

pub async fn seed(pool: &SqlitePool) -> AppResult<()> {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
        .fetch_one(pool)
        .await?;
    if count > 0 {
        return Ok(());
    }

    let ts = "2026-10-01T08:00:00Z";

    // 用户
    let patient_id = "pat_001".to_string();
    let doctor = "u_doctor".to_string();
    let cashier = "u_cashier".to_string();
    let staff = "u_report".to_string();

    sqlx::query("INSERT INTO patients (id,name,identifier) VALUES (?1,'张三','P-1001')")
        .bind(&patient_id)
        .execute(pool)
        .await?;

    let users = [
        (&doctor, "李医生", "DOCTOR", "tok-doctor", None::<&str>),
        (&cashier, "王收费", "CASHIER", "tok-cashier", None),
        (&staff, "赵报告", "REPORT_STAFF", "tok-report", None),
        (
            &"u_patient".to_string(),
            "张三(患者)",
            "PATIENT",
            "tok-patient",
            Some(patient_id.as_str()),
        ),
    ];
    for (uid, name, role, token, pid) in users {
        sqlx::query("INSERT INTO users (id,name,role,token,patient_id) VALUES (?1,?2,?3,?4,?5)")
            .bind(uid)
            .bind(name)
            .bind(role)
            .bind(token)
            .bind(pid)
            .execute(pool)
            .await?;
    }

    // 账单
    let bill_id = "bill_001".to_string();
    sqlx::query(
        "INSERT INTO bills (id,patient_id,bill_no,status,created_at) VALUES (?1,?2,'B-20261001-001','OPEN',?3)",
    )
    .bind(&bill_id)
    .bind(&patient_id)
    .bind(ts)
    .execute(pool)
    .await?;

    // 混合支付：保险 56,000 分(¥560)，自费 34,000 分(¥340)，合计 90,000 分(¥900)
    let payments = [
        ("pay_ins", "INSURANCE", 56_000i64, "INS-PAY-8001"),
        ("pay_self", "SELF_PAY", 34_000, "PAY-8002"),
    ];
    for (pid, method, amount, receipt) in payments {
        sqlx::query(
            "INSERT INTO bill_payments (id,bill_id,method,amount,receipt_no,paid_at)
             VALUES (?1,?2,?3,?4,?5,?6)",
        )
        .bind(pid)
        .bind(&bill_id)
        .bind(method)
        .bind(amount)
        .bind(receipt)
        .bind(ts)
        .execute(pool)
        .await?;
    }

    // 检查项目（金额单位：分）
    // 计价规则：STANDALONE 行直接计价；PACKAGE 父行仅展示套餐原价/优惠，
    // 套餐价按子项原价比例分摊到 PACKAGE_COMPONENT 子项承载，退项按子项成交价计算。
    // 1) 头部CT平扫 ×1：原价 30000，成交 27000（优惠分摊 3000），未执行可退
    // 2) 血常规 ×2：原价 5000/项，已执行 1（部分执行，仅可退 1 项）
    // 3) 心血管筛查套餐：套餐原价 35000，套餐价 30000（套餐优惠 5000）
    //    心电图 原价15000→成交12000；肌钙蛋白 原价20000→成交18000
    // 4) 腹部超声 ×1：原价 16000，成交 13000，已执行且已出报告（演示已执行报告拦截）
    // 5) 心脏彩超 ×1：原价/成交 10000，未执行但报告已异常出具
    //    （演示：医生无法直接改未执行、收费员退项挂起、报告员作废后退款到账并双向追查）
    let items: Vec<(String, Option<String>, &str, &str, &str, i64, i64, i64, i64, i64, &str, i64)> = vec![
        ("it_ct".into(), None, "CT-01", "头部CT平扫", "STANDALONE", 30_000, 27_000, 1, 0, 0, "SCHEDULED", 1),
        ("it_cbc".into(), None, "LAB-01", "血常规", "STANDALONE", 5_000, 5_000, 2, 1, 0, "PARTIALLY_EXECUTED", 2),
        ("it_pkg".into(), None, "PKG-09", "心血管筛查套餐", "PACKAGE", 35_000, 0, 1, 0, 0, "SCHEDULED", 3),
        ("it_ecg".into(), Some("it_pkg".to_string()), "ECG-01", "心电图(套餐子项)", "PACKAGE_COMPONENT", 15_000, 12_000, 1, 0, 0, "SCHEDULED", 1),
        ("it_trop".into(), Some("it_pkg".to_string()), "LAB-22", "肌钙蛋白(套餐子项)", "PACKAGE_COMPONENT", 20_000, 18_000, 1, 0, 0, "SCHEDULED", 2),
        ("it_us".into(), None, "US-03", "腹部超声", "STANDALONE", 16_000, 13_000, 1, 1, 0, "EXECUTED", 4),
        ("it_echo".into(), None, "ECHO-1", "心脏彩超", "STANDALONE", 10_000, 10_000, 1, 0, 0, "SCHEDULED", 5),
    ];
    // 字段: id, parent, code, name, category, list, unit, qty, executed, refunded, status, sort
    for (iid, parent, code, name, cat, list, unit, qty, executed, refunded, status, sort) in items {
        // 套餐父行优惠 = 套餐原价 - 套餐价（子项成交价之和）
        let discount = if iid == "it_pkg" {
            5_000i64
        } else {
            (list - unit) * qty
        };
        sqlx::query(
            "INSERT INTO bill_items
             (id,bill_id,parent_item_id,code,name,category,unit_price,list_unit_price,
              quantity,executed_qty,refunded_qty,discount_total,status,report_status,sort_no)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,
               CASE ?14 WHEN 'it_us' THEN 'ISSUED' WHEN 'it_echo' THEN 'ISSUED' ELSE 'NONE' END, ?15)",
        )
        .bind(&iid)
        .bind(&bill_id)
        .bind(&parent)
        .bind(code)
        .bind(name)
        .bind(cat)
        .bind(unit)
        .bind(list)
        .bind(qty)
        .bind(executed)
        .bind(refunded)
        .bind(discount)
        .bind(status)
        .bind(&iid)
        .bind(sort)
        .execute(pool)
        .await?;
    }

    // 腹部超声已出报告；心脏彩超报告异常出具（项目实际未执行）
    let report_id = id("rpt");
    sqlx::query(
        "INSERT INTO reports (id,bill_item_id,report_no,status,issued_by,issued_at)
         VALUES (?1,'it_us','RPT-US-0001','ISSUED',?2,?3)",
    )
    .bind(&report_id)
    .bind(&staff)
    .bind(ts)
    .execute(pool)
    .await?;

    sqlx::query(
        "INSERT INTO reports (id,bill_item_id,report_no,status,issued_by,issued_at)
         VALUES (?1,'it_echo','RPT-ECHO-0002','ISSUED',?2,?3)",
    )
    .bind(id("rpt"))
    .bind(&staff)
    .bind(ts)
    .execute(pool)
    .await?;

    Ok(())
}
