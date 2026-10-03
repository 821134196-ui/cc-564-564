//! 核心业务服务：医生取消/执行、收费员退项、报告作废，
//! 以及「按原付款构成比例拆分退款」的计算与全账单退款上限校验。
//!
//! 退款状态机：
//!   无报告          : 收费员申请 -> COMPLETED（即时出回执）
//!   已出报告        : 收费员申请 -> AWAITING_VOID
//!                     报告员作废报告 -> report_voids 关联 refund_id -> COMPLETED（出回执）

use crate::bill_view::build_view;
use crate::error::{AppError, AppResult};
use crate::models::*;
use crate::state::AppState;
use serde_json::json;
use sqlx::SqliteConnection;
use uuid::Uuid;

fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}

fn new_id(prefix: &str) -> String {
    format!("{prefix}_{}", Uuid::new_v4().simple())
}

async fn load_item(conn: &mut SqliteConnection, item_id: &str) -> AppResult<BillItem> {
    sqlx::query_as::<_, BillItem>("SELECT * FROM bill_items WHERE id = ?1")
        .bind(item_id)
        .fetch_optional(conn)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("检查项目 {item_id} 不存在")))
}

async fn load_bill_payments(conn: &mut SqliteConnection, bill_id: &str) -> AppResult<Vec<Payment>> {
    sqlx::query_as::<_, Payment>("SELECT * FROM bill_payments WHERE bill_id = ?1 ORDER BY paid_at")
        .bind(bill_id)
        .fetch_all(conn)
        .await
        .map_err(Into::into)
}

/// 账单上已完成退款按付款渠道汇总
async fn completed_refunds_by_method(
    conn: &mut SqliteConnection,
    bill_id: &str,
) -> AppResult<(i64, i64, i64)> {
    let row: (i64, i64, i64) = sqlx::query_as(
        "SELECT COALESCE(SUM(net_refund),0), COALESCE(SUM(insurance_amount),0),
                COALESCE(SUM(self_pay_amount),0)
         FROM refunds WHERE bill_id = ?1 AND status = 'COMPLETED'",
    )
    .bind(bill_id)
    .fetch_one(conn)
    .await?;
    Ok(row)
}

/// 退款金额计算结果（金额单位：分）
#[derive(Debug)]
pub struct RefundCalc {
    pub gross: i64,
    pub discount_share: i64,
    pub net: i64,
    pub insurance: i64,
    pub self_pay: i64,
    pub detail: serde_json::Value,
}

/// 按原付款构成（保险/自费比例）拆分退款，基点整数运算避免浮点误差。
/// 同时执行两条硬性上限：
///   1) 本次后账单累计退款总额 <= 原始付款总额
///   2) 每个渠道累计退款 <= 该渠道原始付款额
pub fn calc_refund(
    item: &BillItem,
    qty: i64,
    payments: &[Payment],
    already_refunded_total: i64,
    already_insurance: i64,
    already_self: i64,
) -> AppResult<RefundCalc> {
    if qty <= 0 {
        return Err(AppError::business("退项数量必须大于 0"));
    }
    let list_unit = item.list_unit_price;
    let paid_unit = item.unit_price;
    let discount_per_unit = list_unit - paid_unit;

    let gross = list_unit * qty;
    let discount_share = discount_per_unit * qty;
    let net = paid_unit * qty; // 实际可退 = 成交价 × 数量（优惠已不分摊退还）

    let insurance_original: i64 = payments.iter().filter(|p| p.method == "INSURANCE").map(|p| p.amount).sum();
    let self_original: i64 = payments.iter().filter(|p| p.method == "SELF_PAY").map(|p| p.amount).sum();
    let paid_total = insurance_original + self_original;
    if paid_total <= 0 {
        return Err(AppError::business("账单无付款记录，无法退款"));
    }

    // 上限 1：总退款不得超过原付款
    let remaining_total = paid_total - already_refunded_total;
    if net > remaining_total {
        return Err(AppError::business(format!(
            "本次退款 {net} 分将使累计退款超过原付款总额（剩余可退 {remaining_total} 分），已拦截"
        )));
    }

    // 按原始保险/自费构成比例拆分（保险向下取整，尾差归自费）
    let mut insurance = net.checked_mul(insurance_original).unwrap_or(0) / paid_total;
    let mut self_pay = net - insurance;

    // 上限 2：各渠道不超过其原始付款额；保险超限时把超出部分转由自费承担
    let insurance_cap = insurance_original - already_insurance;
    let self_cap = self_original - already_self;
    if insurance > insurance_cap {
        let overflow = insurance - insurance_cap;
        insurance = insurance_cap;
        self_pay += overflow;
    }
    if self_pay > self_cap {
        return Err(AppError::business(format!(
            "自费渠道剩余可退额度 {self_cap} 分不足以承担本次退款 {self_pay} 分，已拦截"
        )));
    }

    let detail = json!({
        "formula": "gross = list_unit_price × qty; discount_share = (list_unit_price - unit_price) × qty; net = unit_price × qty; 渠道拆分按原保险/自费付款比例",
        "inputs": {
            "billItemId": item.id,
            "billItemName": item.name,
            "quantity": qty,
            "listUnitPrice": list_unit,
            "unitPrice": paid_unit,
            "discountPerUnit": discount_per_unit,
        },
        "paymentComposition": {
            "insuranceOriginal": insurance_original,
            "selfPayOriginal": self_original,
            "insuranceRatioBp": insurance_original * 10_000 / paid_total,
            "selfPayRatioBp": self_original * 10_000 / paid_total,
            "alreadyRefundedTotal": already_refunded_total,
            "alreadyInsuranceRefunded": already_insurance,
            "alreadySelfPayRefunded": already_self,
        },
        "steps": [
            {"step": "gross_refund", "amount": gross, "note": "原价退款额(优惠前)"},
            {"step": "discount_share", "amount": -discount_share, "note": "扣减:本退项分摊的优惠"},
            {"step": "net_refund", "amount": net, "note": "实际退款额"},
            {"step": "insurance_amount", "amount": insurance, "note": "退回保险(冲账)"},
            {"step": "self_pay_amount", "amount": self_pay, "note": "退回自费(原路退回)"}
        ],
        "caps": {
            "remainingTotalCap": remaining_total,
            "remainingInsuranceCap": insurance_cap,
            "remainingSelfPayCap": self_cap
        }
    });

    Ok(RefundCalc {
        gross,
        discount_share,
        net,
        insurance,
        self_pay,
        detail,
    })
}

pub struct CreateRefundInput {
    pub item_id: String,
    pub qty: i64,
    pub cancel_reason: String, // DOCTOR_CANCEL / EQUIPMENT_FAULT
    pub cancel_remark: Option<String>,
    pub cashier_id: String,
}

/// 收费员办理退项
pub async fn create_refund(state: &AppState, input: CreateRefundInput) -> AppResult<Refund> {
    if !matches!(input.cancel_reason.as_str(), "DOCTOR_CANCEL" | "EQUIPMENT_FAULT") {
        return Err(AppError::business("取消原因必须是 DOCTOR_CANCEL 或 EQUIPMENT_FAULT"));
    }

    let mut tx = state.pool.begin().await?;

    let item = load_item(&mut tx, &input.item_id).await?;

    if item.category == "PACKAGE" {
        return Err(AppError::business("套餐项目须按具体子项办理退项"));
    }

    // 重复退项 / 待处理申请拦截
    let existing = sqlx::query_as::<_, Refund>(
        "SELECT * FROM refunds WHERE bill_item_id = ?1 ORDER BY created_at",
    )
    .bind(&item.id)
    .fetch_all(&mut *tx)
    .await?;
    if existing.iter().any(|r| r.status == "AWAITING_VOID") {
        return Err(AppError::business("该项目已有待报告作废的退项申请，请勿重复提交"));
    }
    let refunded_qty: i64 = existing
        .iter()
        .filter(|r| r.status == "COMPLETED")
        .map(|r| r.refund_qty)
        .sum();
    let refundable_qty = item.quantity - item.executed_qty - refunded_qty;
    if refundable_qty <= 0 {
        return Err(AppError::business("该项目可退数量为 0：已全部执行或已完成退款，重复退项被拦截"));
    }
    if input.qty > refundable_qty {
        return Err(AppError::business(format!(
            "本次申请退 {0} 项，但未执行且未退数量仅 {refundable_qty} 项（部分执行只退可退部分）",
            input.qty
        )));
    }

    // 已出报告：不能直接退，先挂起等待作废（报告与退款双向追查）
    let needs_void = item.report_status == "ISSUED";

    let payments = load_bill_payments(&mut tx, &item.bill_id).await?;
    let (already_total, already_ins, already_self) =
        completed_refunds_by_method(&mut tx, &item.bill_id).await?;
    let calc = calc_refund(
        &item,
        input.qty,
        &payments,
        already_total,
        already_ins,
        already_self,
    )?;

    let ratio = json!({
        "insuranceRatioBp": calc.detail["paymentComposition"]["insuranceRatioBp"],
        "selfPayRatioBp": calc.detail["paymentComposition"]["selfPayRatioBp"]
    })
    .to_string();

    let refund_id = new_id("rf");
    let ts = now();

    if needs_void {
        sqlx::query(
            "INSERT INTO refunds
             (id,bill_id,bill_item_id,status,cancel_reason,cancel_remark,refund_qty,
              unit_list_price,gross_refund,discount_share,net_refund,
              insurance_amount,self_pay_amount,payment_ratio,calc_detail,
              requested_by,created_at)
             VALUES (?1,?2,?3,'AWAITING_VOID',?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16)",
        )
        .bind(&refund_id)
        .bind(&item.bill_id)
        .bind(&item.id)
        .bind(&input.cancel_reason)
        .bind(&input.cancel_remark)
        .bind(input.qty)
        .bind(item.list_unit_price)
        .bind(calc.gross)
        .bind(calc.discount_share)
        .bind(calc.net)
        .bind(calc.insurance)
        .bind(calc.self_pay)
        .bind(&ratio)
        .bind(calc.detail.to_string())
        .bind(&input.cashier_id)
        .bind(&ts)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        return sqlx::query_as::<_, Refund>("SELECT * FROM refunds WHERE id = ?1")
            .bind(&refund_id)
            .fetch_one(&state.pool)
            .await
            .map_err(Into::into);
    }

    // 无报告：直接完成退款，调用模拟网关出具回执
    let ins_receipt = if calc.insurance > 0 {
        let ins_orig = payments.iter().find(|p| p.method == "INSURANCE").map(|p| p.receipt_no.as_str()).unwrap_or("");
        Some(state.gateway.insurance_chargeback(calc.insurance, ins_orig).receipt_no)
    } else {
        None
    };
    let pay_receipt = if calc.self_pay > 0 {
        let self_orig = payments.iter().find(|p| p.method == "SELF_PAY").map(|p| p.receipt_no.as_str()).unwrap_or("");
        Some(state.gateway.self_pay_refund(calc.self_pay, self_orig).receipt_no)
    } else {
        None
    };

    insert_completed_refund(
        &mut tx,
        &refund_id,
        &item,
        &input,
        &calc,
        &ratio,
        &ins_receipt,
        &pay_receipt,
        &ts,
    )
    .await?;

    tx.commit().await?;

    sqlx::query_as::<_, Refund>("SELECT * FROM refunds WHERE id = ?1")
        .bind(&refund_id)
        .fetch_one(&state.pool)
        .await
        .map_err(Into::into)
}

#[allow(clippy::too_many_arguments)]
async fn insert_completed_refund(
    conn: &mut SqliteConnection,
    refund_id: &str,
    item: &BillItem,
    input: &CreateRefundInput,
    calc: &RefundCalc,
    ratio: &str,
    ins_receipt: &Option<String>,
    pay_receipt: &Option<String>,
    ts: &str,
) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO refunds
         (id,bill_id,bill_item_id,status,cancel_reason,cancel_remark,refund_qty,
          unit_list_price,gross_refund,discount_share,net_refund,
          insurance_amount,self_pay_amount,payment_ratio,calc_detail,
          insurance_receipt_no,payment_receipt_no,requested_by,confirmed_by,
          created_at,completed_at)
         VALUES (?1,?2,?3,'COMPLETED',?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?17,?18,?18)",
    )
    .bind(refund_id)
    .bind(&item.bill_id)
    .bind(&item.id)
    .bind(&input.cancel_reason)
    .bind(&input.cancel_remark)
    .bind(input.qty)
    .bind(item.list_unit_price)
    .bind(calc.gross)
    .bind(calc.discount_share)
    .bind(calc.net)
    .bind(calc.insurance)
    .bind(calc.self_pay)
    .bind(ratio)
    .bind(calc.detail.to_string())
    .bind(ins_receipt)
    .bind(pay_receipt)
    .bind(&input.cashier_id)
    .bind(ts)
    .execute(&mut *conn)
    .await?;

    apply_refund_to_item(conn, item, input.qty, ts).await
}

/// 退款完成后更新项目的已退数量与状态
async fn apply_refund_to_item(
    conn: &mut SqliteConnection,
    item: &BillItem,
    qty: i64,
    _ts: &str,
) -> AppResult<()> {
    let new_refunded = item.refunded_qty + qty;
    let new_status = if new_refunded + item.executed_qty >= item.quantity {
        if item.executed_qty > 0 {
            "PARTIALLY_REFUNDED"
        } else {
            "REFUNDED"
        }
    } else if item.executed_qty > 0 {
        "PARTIALLY_EXECUTED"
    } else {
        "CANCELLED"
    };
    sqlx::query(
        "UPDATE bill_items SET refunded_qty = ?1,
            status = CASE WHEN status = 'EXECUTED' THEN status ELSE ?2 END
         WHERE id = ?3",
    )
    .bind(new_refunded)
    .bind(new_status)
    .bind(&item.id)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// 报告员作废报告：登记作废记录（关联退款单），并把挂起的退款单推进为 COMPLETED
pub async fn void_report(
    state: &AppState,
    report_id: &str,
    reason: &str,
    staff_id: &str,
) -> AppResult<(ReportVoid, Refund)> {
    if reason.trim().is_empty() {
        return Err(AppError::business("作废原因不能为空"));
    }
    let mut tx = state.pool.begin().await?;

    let report = sqlx::query_as::<_, Report>("SELECT * FROM reports WHERE id = ?1")
        .bind(report_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("报告 {report_id} 不存在")))?;
    if report.status == "VOIDED" {
        return Err(AppError::business("报告已作废，请勿重复操作"));
    }

    let item = load_item(&mut tx, &report.bill_item_id).await?;

    // 找到等待作废的退款单
    let pending = sqlx::query_as::<_, Refund>(
        "SELECT * FROM refunds WHERE bill_item_id = ?1 AND status = 'AWAITING_VOID'
         ORDER BY created_at LIMIT 1",
    )
    .bind(&item.id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| {
        AppError::business("该报告没有待处理的退项申请；无退款关联时不能从退项流程作废报告")
    })?;

    // 退款数量必须覆盖整张报告（已出报告意味着全部执行？此处允许报告对应整项，
    // 若项目部分执行则报告按整份作废、仅退未执行部分——报告已出说明未执行数量应为 0，
    // 出现挂起单说明医生先取消后出报告的异常序，这里以退款单数量为准完成退款）
    let void_id = new_id("void");
    let ts = now();

    sqlx::query("INSERT INTO report_voids (id,report_id,bill_item_id,reason,voided_by,refund_id,created_at)
                 VALUES (?1,?2,?3,?4,?5,?6,?7)")
        .bind(&void_id)
        .bind(&report.id)
        .bind(&item.id)
        .bind(reason)
        .bind(staff_id)
        .bind(&pending.id)
        .bind(&ts)
        .execute(&mut *tx)
        .await?;

    sqlx::query("UPDATE reports SET status = 'VOIDED', voided_at = ?1 WHERE id = ?2")
        .bind(&ts)
        .bind(&report.id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("UPDATE bill_items SET report_status = 'VOIDED' WHERE id = ?1")
        .bind(&item.id)
        .execute(&mut *tx)
        .await?;

    // 再次校验上限（作废确认时可能已有其他退款落账）
    let payments = load_bill_payments(&mut tx, &item.bill_id).await?;
    let (already_total, already_ins, already_self) =
        completed_refunds_by_method(&mut tx, &item.bill_id).await?;
    if pending.net_refund > payments.iter().map(|p| p.amount).sum::<i64>() - already_total {
        return Err(AppError::business("确认退款将使累计退款超过原付款，已拦截"));
    }
    let insurance_original: i64 = payments.iter().filter(|p| p.method == "INSURANCE").map(|p| p.amount).sum();
    let self_original: i64 = payments.iter().filter(|p| p.method == "SELF_PAY").map(|p| p.amount).sum();
    if pending.insurance_amount > insurance_original - already_ins
        || pending.self_pay_amount > self_original - already_self
    {
        return Err(AppError::business("渠道退款额度不足，已拦截"));
    }

    // 调用模拟网关
    let ins_receipt = if pending.insurance_amount > 0 {
        let orig = payments.iter().find(|p| p.method == "INSURANCE").map(|p| p.receipt_no.as_str()).unwrap_or("");
        Some(state.gateway.insurance_chargeback(pending.insurance_amount, orig).receipt_no)
    } else {
        None
    };
    let pay_receipt = if pending.self_pay_amount > 0 {
        let orig = payments.iter().find(|p| p.method == "SELF_PAY").map(|p| p.receipt_no.as_str()).unwrap_or("");
        Some(state.gateway.self_pay_refund(pending.self_pay_amount, orig).receipt_no)
    } else {
        None
    };

    sqlx::query(
        "UPDATE refunds SET status='COMPLETED', report_void_id=?1,
            insurance_receipt_no=?2, payment_receipt_no=?3,
            confirmed_by=?4, completed_at=?5 WHERE id=?6",
    )
    .bind(&void_id)
    .bind(&ins_receipt)
    .bind(&pay_receipt)
    .bind(staff_id)
    .bind(&ts)
    .bind(&pending.id)
    .execute(&mut *tx)
    .await?;

    apply_refund_to_item(&mut tx, &item, pending.refund_qty, &ts).await?;

    tx.commit().await?;

    let void_record = sqlx::query_as::<_, ReportVoid>("SELECT * FROM report_voids WHERE id = ?1")
        .bind(&void_id)
        .fetch_one(&state.pool)
        .await?;
    let completed = sqlx::query_as::<_, Refund>("SELECT * FROM refunds WHERE id = ?1")
        .bind(&pending.id)
        .fetch_one(&state.pool)
        .await?;
    Ok((void_record, completed))
}

/// 医生登记检查执行（含部分执行）
pub async fn mark_executed(
    state: &AppState,
    item_id: &str,
    qty: i64,
    _doctor_id: &str,
) -> AppResult<BillItem> {
    let mut tx = state.pool.begin().await?;
    let item = load_item(&mut tx, item_id).await?;
    if item.category == "PACKAGE" {
        return Err(AppError::business("套餐请登记具体子项的执行情况"));
    }
    if qty <= 0 || qty > item.quantity - item.executed_qty - item.refunded_qty {
        return Err(AppError::business(format!(
            "执行数量无效：剩余可执行 {0} 项",
            item.quantity - item.executed_qty - item.refunded_qty
        )));
    }
    if item.report_status == "VOIDED" {
        return Err(AppError::business("报告已作废的项目不能登记为已执行"));
    }
    let new_executed = item.executed_qty + qty;
    let new_status = if new_executed >= item.quantity { "EXECUTED" } else { "PARTIALLY_EXECUTED" };
    sqlx::query("UPDATE bill_items SET executed_qty = ?1, status = ?2 WHERE id = ?3")
        .bind(new_executed)
        .bind(new_status)
        .bind(item_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    let mut conn = state.pool.acquire().await?;
    load_item(&mut *conn, item_id).await
}

/// 医生登记取消（医生取消检查 / 设备故障）。已出报告的项目不能改成未执行。
pub async fn cancel_item(
    state: &AppState,
    item_id: &str,
    reason: &str,
    remark: Option<&str>,
    doctor_id: &str,
) -> AppResult<BillItem> {
    if !matches!(reason, "DOCTOR_CANCEL" | "EQUIPMENT_FAULT") {
        return Err(AppError::business("取消原因必须是 DOCTOR_CANCEL 或 EQUIPMENT_FAULT"));
    }
    let mut tx = state.pool.begin().await?;
    let item = load_item(&mut tx, item_id).await?;
    if item.category == "PACKAGE" {
        return Err(AppError::business("套餐请对具体子项登记取消"));
    }
    if item.executed_qty >= item.quantity {
        return Err(AppError::business("项目已全部执行，不能取消"));
    }
    if item.report_status == "ISSUED" {
        return Err(AppError::business(
            "该项目已经出报告，不能改成未执行；须先由报告处理人员作废报告",
        ));
    }
    if item.refunded_qty > 0 {
        return Err(AppError::business("项目已发生退款，不能重复登记取消"));
    }
    let ts = now();
    let new_status = if item.executed_qty > 0 { "PARTIALLY_EXECUTED" } else { "CANCELLED" };
    sqlx::query(
        "UPDATE bill_items SET status=?1, cancel_reason=?2, cancel_remark=?3,
            cancelled_by=?4, cancelled_at=?5 WHERE id=?6",
    )
    .bind(new_status)
    .bind(reason)
    .bind(remark)
    .bind(doctor_id)
    .bind(&ts)
    .bind(item_id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    let mut conn = state.pool.acquire().await?;
    load_item(&mut *conn, item_id).await
}

/// 报告员为已执行项目出具报告
pub async fn issue_report(state: &AppState, item_id: &str, staff_id: &str) -> AppResult<Report> {
    let mut tx = state.pool.begin().await?;
    let item = load_item(&mut tx, item_id).await?;
    if item.category == "PACKAGE" {
        return Err(AppError::business("套餐请对具体子项出具报告"));
    }
    if item.executed_qty <= 0 {
        return Err(AppError::business("项目尚未执行，不能出报告"));
    }
    if item.report_status != "NONE" {
        return Err(AppError::business("该项目已有报告（或已作废），不能重复出具"));
    }
    let id = new_id("rpt");
    let ts = now();
    let report_no = format!("RPT-{}", &id[4..12].to_uppercase());
    sqlx::query(
        "INSERT INTO reports (id,bill_item_id,report_no,status,issued_by,issued_at)
         VALUES (?1,?2,?3,'ISSUED',?4,?5)",
    )
    .bind(&id)
    .bind(item_id)
    .bind(&report_no)
    .bind(staff_id)
    .bind(&ts)
    .execute(&mut *tx)
    .await?;
    sqlx::query("UPDATE bill_items SET report_status='ISSUED' WHERE id=?1")
        .bind(item_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    sqlx::query_as::<_, Report>("SELECT * FROM reports WHERE id = ?1")
        .bind(&id)
        .fetch_one(&state.pool)
        .await
        .map_err(Into::into)
}

/// 组装账单完整视图
pub async fn get_bill_view(state: &AppState, bill_id: &str) -> AppResult<crate::bill_view::BillView> {
    let mut conn = state.pool.acquire().await?;
    let bill = sqlx::query_as::<_, Bill>("SELECT * FROM bills WHERE id = ?1")
        .bind(bill_id)
        .fetch_optional(&mut *conn)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("账单 {bill_id} 不存在")))?;
    let patient = sqlx::query_as::<_, Patient>("SELECT * FROM patients WHERE id = ?1")
        .bind(&bill.patient_id)
        .fetch_one(&mut *conn)
        .await?;
    let items = sqlx::query_as::<_, BillItem>("SELECT * FROM bill_items WHERE bill_id = ?1")
        .bind(bill_id)
        .fetch_all(&mut *conn)
        .await?;
    let payments = sqlx::query_as::<_, Payment>("SELECT * FROM bill_payments WHERE bill_id = ?1")
        .bind(bill_id)
        .fetch_all(&mut *conn)
        .await?;
    let refunds = sqlx::query_as::<_, Refund>("SELECT * FROM refunds WHERE bill_id = ?1")
        .bind(bill_id)
        .fetch_all(&mut *conn)
        .await?;
    let reports = sqlx::query_as::<_, Report>(
        "SELECT r.* FROM reports r JOIN bill_items i ON r.bill_item_id = i.id WHERE i.bill_id = ?1",
    )
    .bind(bill_id)
    .fetch_all(&mut *conn)
    .await?;
    Ok(build_view(bill, patient, items, payments, refunds, reports))
}
