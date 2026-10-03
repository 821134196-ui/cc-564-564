//! 退款计算引擎：可退判定、支付构成拆分、幂等与事务落库。

use crate::auth::{ApiError, AuthUser};
use crate::db::mock_receipt_no;
use crate::models::*;
use crate::money::split_by_method;
use serde_json::json;
use sqlx::{Row, Sqlite, Transaction};
use sqlx::SqlitePool;

pub const REFUND_REASONS: [&str; 4] = [
    "doctor_cancel",
    "equipment_failure",
    "patient_request",
    "report_voided",
];

pub struct PlannedItem {
    pub item: BillItemRow,
    pub report_void_id: Option<i64>,
    pub split: Vec<(String, i64)>,
    pub calc_detail: serde_json::Value,
}

pub struct RefundPlan {
    pub items: Vec<PlannedItem>,
    pub skipped: Vec<SkippedItem>,
    pub total_cents: i64,
    pub components: Vec<(String, i64)>,
}

/// 判定单个项目是否可退；可退时返回关联的报告作废记录 id（如有）。
pub fn classify_item(item: &BillItemRow, unlinked_void: Option<&ReportRow>) -> Result<Option<i64>, String> {
    match item.status.as_str() {
        "refunded" => Err("该项目已退项，不可重复退".into()),
        "pending" => Ok(None),
        "executed" => match unlinked_void {
            Some(r) => Ok(Some(r.id)),
            None => Err("项目已执行，费用已发生，不可退".into()),
        },
        "reported" => Err("项目已出报告，不可退；如需退款请先由报告处理员作废报告".into()),
        other => Err(format!("未知状态 {other}")),
    }
}

/// 计算退款计划（在事务内读取，预览与正式提交共用同一套逻辑）。
pub async fn compute_plan(
    tx: &mut Transaction<'_, Sqlite>,
    bill_id: i64,
    requested_ids: &[i64],
) -> Result<RefundPlan, ApiError> {
    let bill: BillRow = sqlx::query_as("SELECT * FROM bills WHERE id = ?")
        .bind(bill_id)
        .fetch_optional(&mut **tx)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("账单 {bill_id} 不存在")))?;

    let items: Vec<BillItemRow> =
        sqlx::query_as("SELECT * FROM bill_items WHERE bill_id = ? ORDER BY id")
            .bind(bill_id)
            .fetch_all(&mut **tx)
            .await?;

    let payments: Vec<PaymentRow> = sqlx::query_as("SELECT * FROM payments WHERE bill_id = ?")
        .bind(bill_id)
        .fetch_all(&mut **tx)
        .await?;

    // 各渠道已退金额
    let refunded_rows: Vec<(String, i64)> = sqlx::query_as(
        "SELECT rc.method, COALESCE(SUM(rc.amount_cents),0)
         FROM refund_components rc JOIN refunds r ON r.id = rc.refund_id
         WHERE r.bill_id = ? GROUP BY rc.method",
    )
    .bind(bill_id)
    .fetch_all(&mut **tx)
    .await?;

    // 尚未关联退款的报告作废记录（reported -> voided 后的特殊退款通道）
    let voided: Vec<ReportRow> = sqlx::query_as(
        "SELECT * FROM reports WHERE bill_id = ? AND status = 'voided' AND refund_id IS NULL",
    )
    .bind(bill_id)
    .fetch_all(&mut **tx)
    .await?;

    // 原付款构成（按渠道汇总）
    let mut paid_by_method: Vec<(String, i64)> = Vec::new();
    for p in &payments {
        if let Some(e) = paid_by_method.iter_mut().find(|(m, _)| *m == p.method) {
            e.1 += p.amount_cents;
        } else {
            paid_by_method.push((p.method.clone(), p.amount_cents));
        }
    }
    // 剩余可退余额 = 原付款 - 已退（退款拆分以它为权重，天然保证各渠道退款不超原付款）
    let mut remaining: Vec<(String, i64)> = paid_by_method
        .iter()
        .map(|(m, paid)| {
            let refunded = refunded_rows
                .iter()
                .find(|(rm, _)| rm == m)
                .map(|(_, a)| *a)
                .unwrap_or(0);
            (m.clone(), paid - refunded)
        })
        .collect();

    // 逐项判定（请求顺序去重）
    let mut seen = std::collections::HashSet::new();
    let mut planned: Vec<(BillItemRow, Option<i64>)> = Vec::new();
    let mut skipped: Vec<SkippedItem> = Vec::new();
    for id in requested_ids {
        if !seen.insert(*id) {
            continue;
        }
        let item = match items.iter().find(|i| i.id == *id) {
            Some(i) => i.clone(),
            None => {
                skipped.push(SkippedItem {
                    item_id: *id,
                    name: "-".into(),
                    reason: "项目不属于该账单".into(),
                });
                continue;
            }
        };
        let void_report = voided.iter().find(|r| r.bill_item_id == item.id);
        match classify_item(&item, void_report) {
            Ok(void_id) => planned.push((item, void_id)),
            Err(reason) => skipped.push(SkippedItem {
                item_id: item.id,
                name: item.name.clone(),
                reason,
            }),
        }
    }

    if planned.is_empty() {
        return Err(ApiError::Conflict(
            "所选项目均不可退，未产生退款".into(),
            skipped,
        ));
    }

    // 逐项按剩余可退余额拆分支付构成
    let mut planned_items: Vec<PlannedItem> = Vec::new();
    let mut total: i64 = 0;
    for (item, void_id) in planned {
        let amount = item.payable_cents - item.refunded_cents;
        let split = split_by_method(amount, &remaining).ok_or_else(|| {
            ApiError::Unprocessable("账单无可退余额，退款总额不能超过原付款".into())
        })?;
        for (m, a) in &split {
            let e = remaining.iter_mut().find(|(rm, _)| rm == m).unwrap();
            e.1 -= a;
        }
        total += amount;

        let discount_note = match (&item.package_name, item.discount_cents) {
            (Some(pkg), _) => format!(
                "套餐「{}」按原价比例分摊，分摊价 {} 分",
                pkg, item.payable_cents
            ),
            (None, d) if d > 0 => "单项优惠".to_string(),
            _ => "无优惠".to_string(),
        };
        let calc_detail = json!({
            "item_id": item.id,
            "name": item.name,
            "list_price_cents": item.list_price_cents,
            "discount_cents": item.discount_cents,
            "discount_note": discount_note,
            "payable_cents": item.payable_cents,
            "refund_amount_cents": amount,
            "original_payment_mix": paid_by_method.iter().map(|(m, a)| json!({
                "method": m, "amount_cents": a,
            })).collect::<Vec<_>>(),
            "payment_split": split.iter().map(|(m, a)| json!({
                "method": m, "amount_cents": a,
            })).collect::<Vec<_>>(),
            "split_basis": "按原付款构成（剩余可退余额）比例拆分，各渠道退款不超过原付款",
        });
        planned_items.push(PlannedItem {
            item,
            report_void_id: void_id,
            split,
            calc_detail,
        });
    }

    // 汇总各渠道退款
    let mut components: Vec<(String, i64)> = Vec::new();
    for p in &planned_items {
        for (m, a) in &p.split {
            if let Some(e) = components.iter_mut().find(|(cm, _)| cm == m) {
                e.1 += a;
            } else {
                components.push((m.clone(), *a));
            }
        }
    }

    // 硬性上限：总退款不得超过原付款
    if bill.total_refunded_cents + total > bill.total_paid_cents {
        return Err(ApiError::Unprocessable(format!(
            "退款总额超限：已退 {} 分 + 本次 {} 分 > 实收 {} 分",
            bill.total_refunded_cents, total, bill.total_paid_cents
        )));
    }

    Ok(RefundPlan {
        items: planned_items,
        skipped,
        total_cents: total,
        components,
    })
}

/// 退款预览（不落库）
pub async fn preview_refund(
    pool: &SqlitePool,
    bill_id: i64,
    requested_ids: &[i64],
) -> Result<RefundPreview, ApiError> {
    let mut tx = pool.begin().await?;
    let plan = compute_plan(&mut tx, bill_id, requested_ids).await?;
    tx.rollback().await?;
    Ok(RefundPreview {
        items: plan
            .items
            .iter()
            .map(|p| RefundItemPreview {
                item_id: p.item.id,
                name: p.item.name.clone(),
                amount_cents: p.item.payable_cents - p.item.refunded_cents,
                calc_detail: p.calc_detail.clone(),
                report_void_id: p.report_void_id,
            })
            .collect(),
        skipped: plan.skipped,
        total_cents: plan.total_cents,
        components: plan
            .components
            .iter()
            .map(|(m, a)| MethodAmount {
                method: m.clone(),
                amount_cents: *a,
            })
            .collect(),
    })
}

/// 正式创建退款（幂等 + 事务）
pub async fn create_refund(
    pool: &SqlitePool,
    operator: &AuthUser,
    bill_id: i64,
    req: &RefundReq,
) -> Result<RefundDetail, ApiError> {
    if req.item_ids.is_empty() {
        return Err(ApiError::BadRequest("未选择任何退项项目".into()));
    }
    if !REFUND_REASONS.contains(&req.reason.as_str()) {
        return Err(ApiError::BadRequest(format!("非法退款原因: {}", req.reason)));
    }

    // 幂等：同一 idempotency_key 重复提交，直接返回原退款单
    if let Some(key) = &req.idempotency_key {
        let existing: Option<(i64,)> = sqlx::query_as(
            "SELECT id FROM refunds WHERE idempotency_key = ?",
        )
        .bind(key)
        .fetch_optional(pool)
        .await?;
        if let Some((rid,)) = existing {
            let mut detail = load_refund_detail(pool, rid).await?;
            detail.replayed = true;
            return Ok(detail);
        }
    }

    let mut tx = pool.begin().await?;
    let plan = compute_plan(&mut tx, bill_id, &req.item_ids).await?;

    // 经报告作废通道退款时，原因必须为 report_voided
    if plan.items.iter().any(|p| p.report_void_id.is_some()) && req.reason != "report_voided" {
        return Err(ApiError::BadRequest(
            "包含已作废报告的项目，退款原因必须选择 report_voided（报告作废）".into(),
        ));
    }

    let (seq,): (i64,) = sqlx::query_as("SELECT COUNT(*) + 1 FROM refunds")
        .fetch_one(&mut *tx)
        .await?;
    let (today,): (String,) = sqlx::query_as("SELECT date('now','localtime')")
        .fetch_one(&mut *tx)
        .await?;
    let refund_no = format!("R{}-{:03}", today.replace('-', ""), seq);

    let refund_id: i64 = sqlx::query(
        "INSERT INTO refunds (refund_no, bill_id, reason, reason_detail, operator_id, total_cents, idempotency_key)
         VALUES (?,?,?,?,?,?,?)",
    )
    .bind(&refund_no)
    .bind(bill_id)
    .bind(&req.reason)
    .bind(&req.reason_detail)
    .bind(operator.id)
    .bind(plan.total_cents)
    .bind(&req.idempotency_key)
    .execute(&mut *tx)
    .await?
    .last_insert_rowid();

    for p in &plan.items {
        // bill_item_id 唯一约束是“同一项目只退一次”的数据库兜底
        sqlx::query(
            "INSERT INTO refund_items (refund_id, bill_item_id, amount_cents, calc_detail, report_void_id)
             VALUES (?,?,?,?,?)",
        )
        .bind(refund_id)
        .bind(p.item.id)
        .bind(p.item.payable_cents - p.item.refunded_cents)
        .bind(p.calc_detail.to_string())
        .bind(p.report_void_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            if e.to_string().contains("UNIQUE") {
                ApiError::Conflict(
                    format!("项目 {} 已退项，同一项目重复退项只成功一次", p.item.name),
                    vec![],
                )
            } else {
                ApiError::Internal(e.to_string())
            }
        })?;

        sqlx::query(
            "UPDATE bill_items SET status = 'refunded', refunded_cents = payable_cents WHERE id = ?",
        )
        .bind(p.item.id)
        .execute(&mut *tx)
        .await?;

        // 报告作废记录 ↔ 退款记录 双向关联
        if let Some(void_id) = p.report_void_id {
            sqlx::query("UPDATE reports SET refund_id = ? WHERE id = ?")
                .bind(refund_id)
                .bind(void_id)
                .execute(&mut *tx)
                .await?;
        }
    }

    // 各渠道退款回执（本地模拟）
    for (method, amount) in &plan.components {
        sqlx::query(
            "INSERT INTO refund_components (refund_id, method, amount_cents, receipt_no) VALUES (?,?,?,?)",
        )
        .bind(refund_id)
        .bind(method)
        .bind(amount)
        .bind(mock_receipt_no(method, "R"))
        .execute(&mut *tx)
        .await?;
    }

    // 更新账单合计与状态
    let (remaining_items,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM bill_items WHERE bill_id = ? AND status != 'refunded'",
    )
    .bind(bill_id)
    .fetch_one(&mut *tx)
    .await?;
    let new_status = if remaining_items == 0 {
        "fully_refunded"
    } else {
        "partially_refunded"
    };
    sqlx::query(
        "UPDATE bills SET total_refunded_cents = total_refunded_cents + ?, status = ? WHERE id = ?",
    )
    .bind(plan.total_cents)
    .bind(new_status)
    .bind(bill_id)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        "INSERT INTO audit_logs (actor_id, actor_name, actor_role, action, entity, entity_id, detail)
         VALUES (?,?,?,?,?,?,?)",
    )
    .bind(operator.id)
    .bind(&operator.display_name)
    .bind(operator.role.as_str())
    .bind("refund.create")
    .bind("refund")
    .bind(refund_id)
    .bind(format!(
        "退款单 {refund_no}：账单 {bill_id}，{} 项共 {} 分，原因 {}；跳过 {} 项",
        plan.items.len(),
        plan.total_cents,
        req.reason,
        plan.skipped.len()
    ))
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    let mut detail = load_refund_detail(pool, refund_id).await?;
    detail.skipped = plan.skipped;
    Ok(detail)
}

/// 加载退款单完整详情（含计算明细、渠道回执、关联的报告作废记录）
pub async fn load_refund_detail(pool: &SqlitePool, refund_id: i64) -> Result<RefundDetail, ApiError> {
    let refund: RefundRow = sqlx::query_as("SELECT * FROM refunds WHERE id = ?")
        .bind(refund_id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("退款单 {refund_id} 不存在")))?;

    let operator_name: String = sqlx::query("SELECT display_name FROM users WHERE id = ?")
        .bind(refund.operator_id)
        .fetch_one(pool)
        .await?
        .get(0);

    let items: Vec<RefundItemRow> =
        sqlx::query_as("SELECT * FROM refund_items WHERE refund_id = ? ORDER BY id")
            .bind(refund_id)
            .fetch_all(pool)
            .await?;
    let mut item_views = Vec::new();
    for it in &items {
        let name: String = sqlx::query("SELECT name FROM bill_items WHERE id = ?")
            .bind(it.bill_item_id)
            .fetch_one(pool)
            .await?
            .get(0);
        item_views.push(RefundItemView {
            bill_item_id: it.bill_item_id,
            name,
            amount_cents: it.amount_cents,
            calc_detail: serde_json::from_str(&it.calc_detail).unwrap_or(json!({})),
            report_void_id: it.report_void_id,
        });
    }

    let components: Vec<RefundComponentRow> =
        sqlx::query_as("SELECT * FROM refund_components WHERE refund_id = ? ORDER BY id")
            .bind(refund_id)
            .fetch_all(pool)
            .await?;

    // 与本次退款关联的报告作废记录（双向追查）
    let linked: Vec<ReportRow> = sqlx::query_as(
        "SELECT * FROM reports WHERE refund_id = ? OR id IN
         (SELECT report_void_id FROM refund_items WHERE refund_id = ? AND report_void_id IS NOT NULL)",
    )
    .bind(refund_id)
    .bind(refund_id)
    .fetch_all(pool)
    .await?;
    let linked_reports = linked
        .iter()
        .map(|r| ReportBrief {
            id: r.id,
            status: r.status.clone(),
            summary: r.summary.clone(),
            issued_at: r.issued_at.clone(),
            void_reason: r.void_reason.clone(),
            refund_id: r.refund_id,
        })
        .collect();

    Ok(RefundDetail {
        id: refund.id,
        refund_no: refund.refund_no,
        bill_id: refund.bill_id,
        reason: refund.reason,
        reason_detail: refund.reason_detail,
        operator_name,
        total_cents: refund.total_cents,
        created_at: refund.created_at,
        items: item_views,
        components,
        linked_reports,
        skipped: vec![],
        replayed: false,
    })
}
