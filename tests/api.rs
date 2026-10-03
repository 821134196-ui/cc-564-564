//! 端到端验证：部分执行、套餐拆分、混合支付退款金额核对、
//! 重复退项拦截、报告作废联动、岗位越权拦截、退款不超原付款。
//!
//! 每个测试用独立临时 SQLite 文件，服务在随机端口以真实 HTTP 启动。

use clinic_refund::{app, bootstrap};
use serde_json::{json, Value};

struct Harness {
    base: String,
    _temp: String,
    _server: tokio::task::JoinHandle<()>,
}

impl Harness {
    async fn new() -> Self {
        let temp = format!(
            "/tmp/clinic-test-{}-{}.db",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let state = bootstrap(&temp, true).await.expect("bootstrap");
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = tokio::spawn(async move {
            axum::serve(listener, app(state)).await.unwrap();
        });
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        Harness {
            base: format!("http://127.0.0.1:{port}"),
            _temp: temp,
            _server: server,
        }
    }
}

struct Resp {
    status: u16,
    body: Value,
}

async fn call(method: &str, url: &str, token: Option<&str>, body: Option<Value>) -> Resp {
    let client = reqwest::Client::new();
    let mut req = client.request(method.parse().unwrap(), url);
    if let Some(t) = token {
        req = req.header("Authorization", format!("Bearer {t}"));
    }
    if let Some(b) = body {
        req = req.json(&b);
    }
    let res = req.send().await.unwrap();
    let status = res.status().as_u16();
    let text = res.text().await.unwrap();
    let body = if text.is_empty() {
        Value::Null
    } else {
        serde_json::from_str(&text).unwrap_or(json!({ "raw": text }))
    };
    Resp { status, body }
}

const BILL: &str = "bill_001";
const DOCTOR: &str = "tok-doctor";
const CASHIER: &str = "tok-cashier";
const REPORT: &str = "tok-report";
const PATIENT: &str = "tok-patient";

async fn post(base: &str, path: &str, token: Option<&str>, body: Value) -> Resp {
    call("POST", &format!("{base}{path}"), token, Some(body)).await
}
async fn get(base: &str, path: &str, token: Option<&str>) -> Resp {
    call("GET", &format!("{base}{path}"), token, None).await
}
async fn refund(base: &str, token: &str, item: &str, qty: i64, reason: &str) -> Resp {
    post(
        base,
        &format!("/api/bills/{BILL}/refunds"),
        Some(token),
        json!({ "itemId": item, "qty": qty, "cancelReason": reason }),
    )
    .await
}

#[tokio::test]
async fn test_full_refund_lifecycle() {
    let h = Harness::new().await;
    let b = &h.base;

    // ---------- 1. 认证 / 岗位越权拦截 ----------
    let r = get(b, &format!("/api/bills/{BILL}"), None).await;
    assert_eq!(r.status, 401, "无令牌必须拒绝");

    // 患者可读本人账单
    let r = get(b, &format!("/api/bills/{BILL}"), Some(PATIENT)).await;
    assert_eq!(r.status, 200);
    let bills = get(b, "/api/bills", Some(PATIENT)).await;
    assert_eq!(bills.body.as_array().unwrap().len(), 1, "患者只见本人账单");

    // 医生不能办理退项
    let r = post(
        b,
        &format!("/api/bills/{BILL}/refunds"),
        Some(DOCTOR),
        json!({ "itemId": "it_ct", "qty": 1, "cancelReason": "DOCTOR_CANCEL" }),
    )
    .await;
    assert_eq!(r.status, 403, "医生越权退项必须 403: {:?}", r.body);

    // 收费员不能登记取消
    let r = post(
        b,
        &format!("/api/bills/{BILL}/items/it_ct/cancel"),
        Some(CASHIER),
        json!({ "reason": "DOCTOR_CANCEL" }),
    )
    .await;
    assert_eq!(r.status, 403, "收费员越权取消必须 403");

    // 收费员不能作废报告 / 出报告
    let r = post(
        b,
        "/api/reports/nope/void",
        Some(CASHIER),
        json!({ "reason": "x" }),
    )
    .await;
    assert_eq!(r.status, 403, "收费员越权作废必须 403");
    let r = post(
        b,
        &format!("/api/bills/{BILL}/items/it_us/issue-report"),
        Some(CASHIER),
        json!({}),
    )
    .await;
    assert_eq!(r.status, 403, "收费员越权出报告必须 403");

    // 患者不能退项
    let r = post(
        b,
        &format!("/api/bills/{BILL}/refunds"),
        Some(PATIENT),
        json!({ "itemId": "it_ct", "qty": 1, "cancelReason": "DOCTOR_CANCEL" }),
    )
    .await;
    assert_eq!(r.status, 403);

    // ---------- 2. 医生登记设备故障取消 CT ----------
    let r = post(
        b,
        &format!("/api/bills/{BILL}/items/it_ct/cancel"),
        Some(DOCTOR),
        json!({ "reason": "EQUIPMENT_FAULT", "remark": "CT球管故障" }),
    )
    .await;
    assert_eq!(r.status, 200, "医生登记取消: {:?}", r.body);
    assert_eq!(r.body["status"], "CANCELLED");

    // ---------- 3. 混合支付退款核对（CT：成交 27000 = 保险16800 + 自费10200）----------
    let r = refund(b, CASHIER, "it_ct", 1, "EQUIPMENT_FAULT").await;
    assert_eq!(r.status, 200, "CT 退款应成功: {:?}", r.body);
    assert_eq!(r.body["status"], "COMPLETED");
    assert_eq!(r.body["grossRefund"], 30_000, "原价退款额 30000");
    assert_eq!(r.body["discountShare"], 3_000, "分摊优惠不退");
    assert_eq!(r.body["netRefund"], 27_000);
    assert_eq!(r.body["insuranceAmount"], 16_800, "56000/90000 × 27000");
    assert_eq!(r.body["selfPayAmount"], 10_200);
    assert!(
        r.body["insuranceReceiptNo"].as_str().unwrap().starts_with("INS-REF"),
        "保险冲账回执"
    );
    assert!(
        r.body["paymentReceiptNo"].as_str().unwrap().starts_with("PAY-REF"),
        "自费退款回执"
    );
    // 计算明细落库
    let detail: Value = serde_json::from_str(r.body["calcDetail"].as_str().unwrap()).unwrap();
    assert_eq!(detail["paymentComposition"]["insuranceRatioBp"], 6222);

    // ---------- 4. 重复退项只成功一次 ----------
    let r = refund(b, CASHIER, "it_ct", 1, "EQUIPMENT_FAULT").await;
    assert_eq!(r.status, 422, "重复退项必须被业务规则拦截: {:?}", r.body);
    assert!(r.body["message"].as_str().unwrap().contains("重复退项"));

    // ---------- 5. 部分执行：血常规 2 项已执行 1，只能退 1 项 ----------
    let r = refund(b, CASHIER, "it_cbc", 2, "DOCTOR_CANCEL").await;
    assert_eq!(r.status, 422, "超出可退数量必须拦截");
    let r = refund(b, CASHIER, "it_cbc", 1, "DOCTOR_CANCEL").await;
    assert_eq!(r.status, 200, "部分执行只退可退 1 项: {:?}", r.body);
    assert_eq!(r.body["netRefund"], 5_000);
    // 5000 × 56/90 = 3111 保险，1889 自费
    assert_eq!(r.body["insuranceAmount"], 3_111);
    assert_eq!(r.body["selfPayAmount"], 1_889);
    // 剩余 1 项已执行不可再退
    let r = refund(b, CASHIER, "it_cbc", 1, "DOCTOR_CANCEL").await;
    assert_eq!(r.status, 422);

    // ---------- 6. 套餐拆分：只能退子项，按分摊价 + 原付款构成退回 ----------
    let r = refund(b, CASHIER, "it_pkg", 1, "EQUIPMENT_FAULT").await;
    assert_eq!(r.status, 422, "套餐父行退项必须拒绝");

    // 心电图子项：成交 12000 → 保险 7466 / 自费 4534
    let r = refund(b, CASHIER, "it_ecg", 1, "EQUIPMENT_FAULT").await;
    assert_eq!(r.status, 200, "套餐子项退项: {:?}", r.body);
    assert_eq!(r.body["grossRefund"], 15_000, "子项按原价展示原价");
    assert_eq!(r.body["discountShare"], 3_000, "套餐优惠分摊扣减");
    assert_eq!(r.body["netRefund"], 12_000);
    assert_eq!(r.body["insuranceAmount"], 7_466);
    assert_eq!(r.body["selfPayAmount"], 4_534);

    // 肌钙蛋白子项：成交 18000 → 保险 11200 / 自费 6800
    let r = refund(b, CASHIER, "it_trop", 1, "DOCTOR_CANCEL").await;
    assert_eq!(r.status, 200, "{:?}", r.body);
    assert_eq!(r.body["netRefund"], 18_000);
    assert_eq!(r.body["insuranceAmount"], 11_200);
    assert_eq!(r.body["selfPayAmount"], 6_800);

    // ---------- 7. 已出报告且已执行：超声不能退、医生不能改未执行 ----------
    let r = post(
        b,
        &format!("/api/bills/{BILL}/items/it_us/cancel"),
        Some(DOCTOR),
        json!({ "reason": "DOCTOR_CANCEL" }),
    )
    .await;
    assert_eq!(r.status, 422, "已出报告不能改成未执行: {:?}", r.body);
    let r = refund(b, CASHIER, "it_us", 1, "DOCTOR_CANCEL").await;
    assert_eq!(r.status, 422, "已出报告直接退项必须拦截");

    // ---------- 8. 未执行但报告已出（心脏彩超）：挂起 → 作废 → 完成，双向追查 ----------
    let r = refund(b, CASHIER, "it_echo", 1, "EQUIPMENT_FAULT").await;
    assert_eq!(r.status, 200, "{:?}", r.body);
    assert_eq!(r.body["status"], "AWAITING_VOID");
    assert!(r.body["insuranceReceiptNo"].is_null(), "挂起单不出回执");
    let echo_refund_id = r.body["id"].as_str().unwrap().to_string();

    // 挂起期间重复申请被拦截
    let r = refund(b, CASHIER, "it_echo", 1, "EQUIPMENT_FAULT").await;
    assert_eq!(r.status, 422);
    // 医生也不能对其取消（报告 ISSUED）
    let r = post(
        b,
        &format!("/api/bills/{BILL}/items/it_echo/cancel"),
        Some(DOCTOR),
        json!({ "reason": "EQUIPMENT_FAULT" }),
    )
    .await;
    assert_eq!(r.status, 422);

    // 找到 echo 的报告 id
    let view = get(b, &format!("/api/bills/{BILL}"), Some(CASHIER)).await;
    assert_eq!(view.status, 200);
    let echo_report = view
        .body["reports"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["billItemId"] == "it_echo")
        .unwrap();
    assert_eq!(echo_report["status"], "ISSUED");
    let report_id = echo_report["id"].as_str().unwrap();

    // 医生不能作废报告
    let r = post(
        b,
        &format!("/api/reports/{report_id}/void"),
        Some(DOCTOR),
        json!({ "reason": "x" }),
    )
    .await;
    assert_eq!(r.status, 403);

    // 报告员作废 → 退款自动完成
    let r = post(
        b,
        &format!("/api/reports/{report_id}/void"),
        Some(REPORT),
        json!({ "reason": "设备故障项目取消，报告作废" }),
    )
    .await;
    assert_eq!(r.status, 200, "作废并完成退款: {:?}", r.body);
    assert_eq!(r.body["refund"]["status"], "COMPLETED");
    assert_eq!(r.body["refund"]["netRefund"], 10_000);
    assert_eq!(r.body["refund"]["insuranceAmount"], 6_222);
    assert_eq!(r.body["refund"]["selfPayAmount"], 3_778);
    let void_id = r.body["reportVoid"]["id"].as_str().unwrap().to_string();
    assert_eq!(r.body["reportVoid"]["refundId"], echo_refund_id);

    // 双向追查：退款单 → 作废记录 → 报告
    let trace = get(b, &format!("/api/refunds/{echo_refund_id}/trace"), Some(PATIENT)).await;
    assert_eq!(trace.status, 200, "患者可追查本人退款");
    assert_eq!(trace.body["reportVoid"]["id"], void_id);
    assert_eq!(trace.body["reportVoid"]["refundId"], echo_refund_id);
    assert_eq!(trace.body["report"]["status"], "VOIDED");
    assert_eq!(trace.body["refund"]["reportVoidId"], void_id);

    // 重复作废拦截
    let r = post(
        b,
        &format!("/api/reports/{report_id}/void"),
        Some(REPORT),
        json!({ "reason": "again" }),
    )
    .await;
    assert_eq!(r.status, 422);

    // ---------- 9. 账单调整前后金额 + 退款总额不超过原付款 ----------
    let view = get(b, &format!("/api/bills/{BILL}"), Some(CASHIER)).await;
    let v = &view.body;
    assert_eq!(v["paidTotal"], 90_000, "调整前实付 90000");
    // 已退: CT 27000 + CBC 5000 + ECG 12000 + TROP 18000 + ECHO 10000 = 72000
    assert_eq!(v["refundedTotal"], 72_000, "累计退款 72000");
    assert_eq!(v["adjustedTotal"], 18_000, "调整后应付 18000（超声已执行）");
    assert!(
        v["refundedTotal"].as_i64().unwrap() <= v["paidTotal"].as_i64().unwrap(),
        "总退款永远不超过原付款"
    );
    assert_eq!(v["insuranceRefunded"], 44_799, "保险累计: 16800+3111+7466+11200+6222");
    assert_eq!(v["selfPayRefunded"], 27_201, "自费累计: 10200+1889+4534+6800+3778");
    assert!(
        v["insuranceRefunded"].as_i64().unwrap() <= 56_000
            && v["selfPayRefunded"].as_i64().unwrap() <= 34_000,
        "分渠道退款均不超过各渠道原付款"
    );

    // 项目状态核对
    let by_id = |id: &str| {
        v["items"]
            .as_array()
            .unwrap()
            .iter()
            .chain(
                v["items"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .flat_map(|i| i["children"].as_array().unwrap().iter()),
            )
            .find(|i| i["id"] == id)
            .unwrap()
            .clone()
    };
    assert_eq!(by_id("it_ct")["status"], "REFUNDED");
    assert_eq!(by_id("it_ct")["refundedQty"], 1);
    assert_eq!(by_id("it_cbc")["status"], "PARTIALLY_REFUNDED");
    assert_eq!(by_id("it_echo")["reportStatus"], "VOIDED");
    assert_eq!(by_id("it_us")["nonRefundableReason"].is_null(), false);
}
