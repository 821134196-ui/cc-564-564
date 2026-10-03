//! 集成测试：部分执行退项、套餐拆分、混合支付核对、岗位越权、
//! 重复退项幂等、报告作废联动、退款总额上限。
//!
//! 种子数据（每次测试独立内存库，id 确定）：
//! - 账单1 陈小明：套餐5项(应付 7207/4324/14415/10811/43243) + 彩超18000 + 肿瘤标志物24000
//!   项目1 血常规=已出报告(报告#1)，项目2 尿常规=已执行，项目3~7=待执行；医保73200 + 现金48800
//! - 账单2 刘芳：项目8 磁共振=已执行70000，项目9 胃镜30000，项目10 幽门螺杆菌12000；微信112000
//! - 账单3 王建国：项目11 动态心电图26000，项目12 心脏彩超32000；医保46400(80%) + 支付宝11600(20%)

use axum::body::Body;
use axum::http::{Request, StatusCode};
use clinic_refund::{app, db};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use tower::ServiceExt;

async fn setup() -> axum::Router {
    let pool = db::init_pool("sqlite::memory:").await.unwrap();
    db::seed(&pool).await.unwrap();
    app::router(pool)
}

async fn call(
    app: &axum::Router,
    method: &str,
    uri: &str,
    token: Option<&str>,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json");
    if let Some(t) = token {
        builder = builder.header("authorization", format!("Bearer {t}"));
    }
    let req = builder
        .body(Body::from(
            body.map(|b| b.to_string()).unwrap_or_else(|| "{}".into()),
        ))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let v: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, v)
}

async fn login(app: &axum::Router, username: &str) -> String {
    let (s, v) = call(
        app,
        "POST",
        "/api/auth/login",
        None,
        Some(json!({"username": username, "password": "123456"})),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "登录失败: {v}");
    v["token"].as_str().unwrap().to_string()
}

fn component(components: &Value, method: &str) -> i64 {
    components
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["method"] == method)
        .map(|c| c["amount_cents"].as_i64().unwrap())
        .unwrap_or(0)
}

/// 1. 部分执行：已执行/已出报告的项目自动跳过，只退可退部分
#[tokio::test]
async fn partial_execution_refund_only_refundable() {
    let app = setup().await;
    let cashier = login(&app, "cashier").await;

    // 对账单1的全部 7 个项目发起退项
    let (s, v) = call(
        &app,
        "POST",
        "/api/bills/1/refunds",
        Some(&cashier),
        Some(json!({
            "item_ids": [1,2,3,4,5,6,7],
            "reason": "doctor_cancel",
            "reason_detail": "医生取消 + 设备故障"
        })),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{v}");

    // 只退待执行的 5 项：14415+10811+43243+18000+24000
    assert_eq!(v["total_cents"].as_i64().unwrap(), 110469);
    assert_eq!(v["items"].as_array().unwrap().len(), 5);

    // 已执行与已出报告的项目被跳过并给出原因
    let skipped = v["skipped"].as_array().unwrap();
    assert_eq!(skipped.len(), 2);
    let reasons: Vec<String> = skipped
        .iter()
        .map(|s| s["reason"].as_str().unwrap().to_string())
        .collect();
    assert!(reasons.iter().any(|r| r.contains("已出报告")), "{reasons:?}");
    assert!(reasons.iter().any(|r| r.contains("已执行")), "{reasons:?}");

    // 每个退款项目的支付拆分之和 = 项目应付
    for item in v["items"].as_array().unwrap() {
        let split_sum: i64 = item["calc_detail"]["payment_split"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p["amount_cents"].as_i64().unwrap())
            .sum();
        assert_eq!(split_sum, item["amount_cents"].as_i64().unwrap());
    }

    // 渠道构成合计 = 退款总额，且各渠道不超过原付款
    let comps = &v["components"];
    let ins = component(comps, "insurance");
    let cash = component(comps, "cash");
    assert_eq!(ins + cash, 110469);
    assert!(ins > 0 && ins <= 73200);
    assert!(cash > 0 && cash <= 48800);

    // 账单状态与项目状态
    let (_, bill) = call(&app, "GET", "/api/bills/1", Some(&cashier), None).await;
    assert_eq!(bill["total_refunded_cents"].as_i64().unwrap(), 110469);
    assert_eq!(bill["status"], "partially_refunded");
    let items = bill["items"].as_array().unwrap();
    assert_eq!(items[0]["status"], "reported"); // 血常规不受影响
    assert_eq!(items[1]["status"], "executed"); // 尿常规不受影响
    for i in 2..7 {
        assert_eq!(items[i]["status"], "refunded");
        assert_eq!(items[i]["refundable"], false);
    }
}

/// 2. 套餐拆分：套餐价按原价比例分摊，各项目应付合计严格等于套餐价
#[tokio::test]
async fn package_split_allocation() {
    let app = setup().await;
    let cashier = login(&app, "cashier").await;

    // 用套餐模板开单：全身体检套餐 800.00（原价合计 1110.00）
    let (s, v) = call(
        &app,
        "POST",
        "/api/bills",
        Some(&cashier),
        Some(json!({
            "patient_id": 2,
            "packages": [{"package_id": 1}],
            "payments": [{"method": "cash", "amount_cents": 80000}]
        })),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    let bill_id = v["id"].as_i64().unwrap();

    let items = v["items"].as_array().unwrap();
    assert_eq!(items.len(), 5);
    // 分摊结果：7207 / 4324 / 14415 / 10811 / 43243，合计 80000
    let payables: Vec<i64> = items.iter().map(|i| i["payable_cents"].as_i64().unwrap()).collect();
    assert_eq!(payables, vec![7207, 4324, 14415, 10811, 43243]);
    assert_eq!(payables.iter().sum::<i64>(), 80000);
    // 每项：原价 - 优惠分摊 = 应付
    for it in items {
        assert_eq!(
            it["list_price_cents"].as_i64().unwrap() - it["discount_cents"].as_i64().unwrap(),
            it["payable_cents"].as_i64().unwrap()
        );
        assert_eq!(it["package_name"], "全身体检套餐");
    }

    // 退胸部CT（应付 43243），计算明细保留套餐分摊说明
    let ct_id = items[4]["id"].as_i64().unwrap();
    let (s, v) = call(
        &app,
        "POST",
        &format!("/api/bills/{bill_id}/refunds"),
        Some(&cashier),
        Some(json!({"item_ids": [ct_id], "reason": "patient_request"})),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["total_cents"].as_i64().unwrap(), 43243);
    let detail = &v["items"][0]["calc_detail"];
    assert!(detail["discount_note"].as_str().unwrap().contains("套餐"));
    assert_eq!(detail["list_price_cents"].as_i64().unwrap(), 60000);
    assert_eq!(detail["discount_cents"].as_i64().unwrap(), 16757);
    assert_eq!(component(&v["components"], "cash"), 43243);
}

/// 3. 混合支付：退款严格按原付款构成拆分，各渠道累计不超过原付款
#[tokio::test]
async fn mixed_payment_refund_split() {
    let app = setup().await;
    let cashier = login(&app, "cashier").await;

    // 账单3：医保 46400 (80%) + 支付宝 11600 (20%)
    // 退项目11（26000）→ 医保 20800 / 支付宝 5200
    let (s, v) = call(
        &app,
        "POST",
        "/api/bills/3/refunds",
        Some(&cashier),
        Some(json!({"item_ids": [11], "reason": "patient_request"})),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(component(&v["components"], "insurance"), 20800);
    assert_eq!(component(&v["components"], "alipay"), 5200);

    // 退项目12（32000）→ 医保 25600 / 支付宝 6400
    let (s, v) = call(
        &app,
        "POST",
        "/api/bills/3/refunds",
        Some(&cashier),
        Some(json!({"item_ids": [12], "reason": "equipment_failure"})),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(component(&v["components"], "insurance"), 25600);
    assert_eq!(component(&v["components"], "alipay"), 6400);

    // 核对：各渠道累计退款恰好等于原付款，总额 = 实收，账单全退
    let (_, bill) = call(&app, "GET", "/api/bills/3", Some(&cashier), None).await;
    assert_eq!(bill["total_refunded_cents"].as_i64().unwrap(), 58000);
    assert_eq!(bill["status"], "fully_refunded");
    let by_method = bill["refunded_by_method"].as_array().unwrap();
    assert_eq!(component(&Value::from(by_method.clone()), "insurance"), 46400);
    assert_eq!(component(&Value::from(by_method.clone()), "alipay"), 11600);
}

/// 4. 岗位权限：越权操作一律 403，未登录 401
#[tokio::test]
async fn rbac_enforcement() {
    let app = setup().await;
    let doctor = login(&app, "doctor").await;
    let cashier = login(&app, "cashier").await;
    let reporter = login(&app, "reporter").await;

    // 未登录
    let (s, _) = call(&app, "GET", "/api/bills", None, None).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);

    // 医生退款 → 403
    let (s, v) = call(
        &app,
        "POST",
        "/api/bills/1/refunds",
        Some(&doctor),
        Some(json!({"item_ids": [3], "reason": "doctor_cancel"})),
    )
    .await;
    assert_eq!(s, StatusCode::FORBIDDEN, "{v}");

    // 报告员退款 → 403
    let (s, _) = call(
        &app,
        "POST",
        "/api/bills/1/refunds",
        Some(&reporter),
        Some(json!({"item_ids": [3], "reason": "doctor_cancel"})),
    )
    .await;
    assert_eq!(s, StatusCode::FORBIDDEN);

    // 收费员签发报告 → 403；作废报告 → 403；标记执行 → 403
    let (s, _) = call(&app, "POST", "/api/items/2/report", Some(&cashier), Some(json!({"summary": "x"}))).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, _) = call(&app, "POST", "/api/reports/1/void", Some(&cashier), Some(json!({"reason": "x"}))).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, _) = call(&app, "POST", "/api/items/3/execute", Some(&cashier), None).await;
    assert_eq!(s, StatusCode::FORBIDDEN);

    // 报告员标记执行 → 403
    let (s, _) = call(&app, "POST", "/api/items/3/execute", Some(&reporter), None).await;
    assert_eq!(s, StatusCode::FORBIDDEN);

    // 各岗位本职操作 → 成功
    let (s, _) = call(&app, "POST", "/api/items/3/execute", Some(&doctor), None).await;
    assert_eq!(s, StatusCode::OK);
    let (s, _) = call(
        &app,
        "POST",
        "/api/items/5/flag",
        Some(&doctor),
        Some(json!({"flag": "equipment_failure", "note": "CT 机故障"})),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let (s, _) = call(&app, "POST", "/api/items/2/report", Some(&reporter), Some(json!({"summary": "尿常规正常"}))).await;
    assert_eq!(s, StatusCode::OK);
    let (s, _) = call(
        &app,
        "POST",
        "/api/bills/1/refunds",
        Some(&cashier),
        Some(json!({"item_ids": [4], "reason": "equipment_failure"})),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
}

/// 5. 重复退项：同一项目只成功一次；同一幂等键重放返回原单
#[tokio::test]
async fn duplicate_refund_only_once() {
    let app = setup().await;
    let cashier = login(&app, "cashier").await;

    let body = json!({"item_ids": [11], "reason": "patient_request", "idempotency_key": "key-001"});
    let (s, v1) = call(&app, "POST", "/api/bills/3/refunds", Some(&cashier), Some(body.clone())).await;
    assert_eq!(s, StatusCode::OK, "{v1}");
    assert_eq!(v1["total_cents"].as_i64().unwrap(), 26000);
    let refund_id = v1["id"].as_i64().unwrap();

    // 相同幂等键重放：返回原退款单，不产生新记录
    let (s, v2) = call(&app, "POST", "/api/bills/3/refunds", Some(&cashier), Some(body)).await;
    assert_eq!(s, StatusCode::OK, "{v2}");
    assert_eq!(v2["id"].as_i64().unwrap(), refund_id);
    assert_eq!(v2["replayed"], true);

    // 不带幂等键重复退同一项目 → 409，金额不变
    let (s, v3) = call(
        &app,
        "POST",
        "/api/bills/3/refunds",
        Some(&cashier),
        Some(json!({"item_ids": [11], "reason": "patient_request"})),
    )
    .await;
    assert_eq!(s, StatusCode::CONFLICT, "{v3}");
    assert!(v3["skipped"][0]["reason"].as_str().unwrap().contains("已退项"));

    let (_, bill) = call(&app, "GET", "/api/bills/3", Some(&cashier), None).await;
    assert_eq!(bill["total_refunded_cents"].as_i64().unwrap(), 26000);
    assert_eq!(bill["refunds"].as_array().unwrap().len(), 1);
}

/// 6. 报告联动：已出报告不可退、不可改回未执行；作废后可退且记录互相追查
#[tokio::test]
async fn reported_item_flow() {
    let app = setup().await;
    let cashier = login(&app, "cashier").await;
    let reporter = login(&app, "reporter").await;

    // 已出报告的项目直接退 → 409
    let (s, v) = call(
        &app,
        "POST",
        "/api/bills/1/refunds",
        Some(&cashier),
        Some(json!({"item_ids": [1], "reason": "patient_request"})),
    )
    .await;
    assert_eq!(s, StatusCode::CONFLICT, "{v}");
    assert!(v["skipped"][0]["reason"].as_str().unwrap().contains("已出报告"));

    // 报告员作废报告
    let (s, v) = call(
        &app,
        "POST",
        "/api/reports/1/void",
        Some(&reporter),
        Some(json!({"reason": "报告内容有误，需重新出具"})),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{v}");

    // 作废后项目状态为“已执行”，绝不能回到“未执行”
    let (_, bill) = call(&app, "GET", "/api/bills/1", Some(&cashier), None).await;
    let item1 = &bill["items"][0];
    assert_eq!(item1["status"], "executed");
    assert_ne!(item1["status"], "pending");
    assert_eq!(item1["refundable"], true); // 可经报告作废通道退款

    // 收费员以 report_voided 原因退款，关联作废记录
    let (s, refund) = call(
        &app,
        "POST",
        "/api/bills/1/refunds",
        Some(&cashier),
        Some(json!({"item_ids": [1], "reason": "report_voided", "reason_detail": "报告作废，费用退回"})),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{refund}");
    assert_eq!(refund["total_cents"].as_i64().unwrap(), 7207);
    assert_eq!(refund["items"][0]["report_void_id"].as_i64().unwrap(), 1);
    let refund_id = refund["id"].as_i64().unwrap();

    // 退款单 → 报告作废记录（正向追查）
    let (s, detail) = call(&app, "GET", &format!("/api/refunds/{refund_id}"), Some(&cashier), None).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(detail["linked_reports"][0]["id"].as_i64().unwrap(), 1);
    assert_eq!(detail["linked_reports"][0]["status"], "voided");

    // 报告作废记录 → 退款单（反向追查）
    let (_, reports) = call(&app, "GET", "/api/reports", Some(&reporter), None).await;
    let r1 = reports
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == 1)
        .unwrap()
        .clone();
    assert_eq!(r1["status"], "voided");
    assert_eq!(r1["refund_id"].as_i64().unwrap(), refund_id);
    assert_eq!(r1["refund_no"], refund["refund_no"]);

    // 已退项目不能再退
    let (s, _) = call(
        &app,
        "POST",
        "/api/bills/1/refunds",
        Some(&cashier),
        Some(json!({"item_ids": [1], "reason": "report_voided"})),
    )
    .await;
    assert_eq!(s, StatusCode::CONFLICT);
}

/// 7. 总额上限：任何调整都不能让总退款超过原付款
#[tokio::test]
async fn refund_never_exceeds_paid() {
    let app = setup().await;
    let cashier = login(&app, "cashier").await;

    // 账单1：退掉全部可退项
    let (s, _) = call(
        &app,
        "POST",
        "/api/bills/1/refunds",
        Some(&cashier),
        Some(json!({"item_ids": [3,4,5,6,7], "reason": "doctor_cancel"})),
    )
    .await;
    assert_eq!(s, StatusCode::OK);

    // 再无可退项目 → 409，不产生任何新退款
    let (s, _) = call(
        &app,
        "POST",
        "/api/bills/1/refunds",
        Some(&cashier),
        Some(json!({"item_ids": [2], "reason": "patient_request"})),
    )
    .await;
    assert_eq!(s, StatusCode::CONFLICT);

    let (_, bill) = call(&app, "GET", "/api/bills/1", Some(&cashier), None).await;
    let refunded = bill["total_refunded_cents"].as_i64().unwrap();
    let paid = bill["total_paid_cents"].as_i64().unwrap();
    assert!(refunded <= paid, "总退款 {refunded} 不得超过实收 {paid}");
    assert_eq!(refunded, 110469);
    for m in bill["refunded_by_method"].as_array().unwrap() {
        let method = m["method"].as_str().unwrap();
        let amount = m["amount_cents"].as_i64().unwrap();
        let paid_m = bill["payments"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|p| p["method"] == method)
            .map(|p| p["amount_cents"].as_i64().unwrap())
            .sum::<i64>();
        assert!(amount <= paid_m, "渠道 {method} 退款 {amount} 超过原付款 {paid_m}");
    }

    // 账单2：微信单渠道，退完可退项后累计 == 可退部分，不超实收
    let (s, _) = call(
        &app,
        "POST",
        "/api/bills/2/refunds",
        Some(&cashier),
        Some(json!({"item_ids": [8,9,10], "reason": "patient_request"})),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let (_, bill2) = call(&app, "GET", "/api/bills/2", Some(&cashier), None).await;
    assert_eq!(bill2["total_refunded_cents"].as_i64().unwrap(), 42000);
    assert_eq!(component(&bill2["refunded_by_method"], "wechat"), 42000);
    assert!(bill2["total_refunded_cents"].as_i64().unwrap() <= bill2["total_paid_cents"].as_i64().unwrap());
}
