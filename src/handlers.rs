use crate::app::AppState;
use crate::auth::{ApiError, AuthUser, Role};
use crate::db::{allocate_package, mock_receipt_no, password_hash};
use crate::models::*;
use crate::refund;
use axum::{
    extract::{Path, State},
    Json,
};
use serde_json::json;
use sqlx::Row;

// ---------- 认证 ----------

pub async fn login(
    State(state): State<AppState>,
    Json(req): Json<LoginReq>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let user: Option<UserRow> = sqlx::query_as("SELECT * FROM users WHERE username = ?")
        .bind(&req.username)
        .fetch_optional(&state.pool)
        .await?;
    let user = match user {
        Some(u) if u.password_hash == password_hash(&req.password) => u,
        _ => return Err(ApiError::Unauthorized("用户名或密码错误".into())),
    };
    let token = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO sessions (token, user_id) VALUES (?,?)")
        .bind(&token)
        .bind(user.id)
        .execute(&state.pool)
        .await?;
    Ok(Json(json!({
        "token": token,
        "user": UserInfo {
            id: user.id,
            username: user.username,
            display_name: user.display_name,
            role: user.role,
        }
    })))
}

pub async fn me(user: AuthUser) -> Json<UserInfo> {
    Json(user.info())
}

pub async fn meta(_user: AuthUser) -> Json<serde_json::Value> {
    Json(json!({
        "refund_reasons": [
            {"code": "doctor_cancel", "label": "医生取消检查"},
            {"code": "equipment_failure", "label": "设备故障无法执行"},
            {"code": "patient_request", "label": "患者要求退项"},
            {"code": "report_voided", "label": "报告作废后退款"},
        ],
        "pay_methods": [
            {"code": "insurance", "label": "医保"},
            {"code": "cash", "label": "现金"},
            {"code": "wechat", "label": "微信"},
            {"code": "alipay", "label": "支付宝"},
        ],
        "item_status": [
            {"code": "pending", "label": "待执行"},
            {"code": "executed", "label": "已执行"},
            {"code": "reported", "label": "已出报告"},
            {"code": "refunded", "label": "已退项"},
        ]
    }))
}

// ---------- 账单 ----------

pub async fn list_bills(
    _user: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<Vec<BillSummary>>, ApiError> {
    let rows = sqlx::query_as::<_, (i64, String, String, String, i64, i64, i64, i64, String)>(
        "SELECT b.id, b.bill_no, b.status, p.name, b.total_payable_cents, b.total_paid_cents,
                b.total_refunded_cents,
                (SELECT COUNT(*) FROM bill_items i WHERE i.bill_id = b.id), b.created_at
         FROM bills b JOIN patients p ON p.id = b.patient_id ORDER BY b.id DESC",
    )
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(
        rows.into_iter()
            .map(
                |(id, bill_no, status, patient_name, payable, paid, refunded, item_count, created_at)| {
                    BillSummary {
                        id,
                        bill_no,
                        status,
                        patient_name,
                        total_payable_cents: payable,
                        total_paid_cents: paid,
                        total_refunded_cents: refunded,
                        item_count,
                        created_at,
                    }
                },
            )
            .collect(),
    ))
}

pub async fn get_bill(
    _user: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<BillDetail>, ApiError> {
    load_bill_detail(&state.pool, id).await.map(Json)
}

async fn load_bill_detail(pool: &sqlx::SqlitePool, id: i64) -> Result<BillDetail, ApiError> {
    let bill: BillRow = sqlx::query_as("SELECT * FROM bills WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("账单 {id} 不存在")))?;
    let patient: PatientRow = sqlx::query_as("SELECT * FROM patients WHERE id = ?")
        .bind(bill.patient_id)
        .fetch_one(pool)
        .await?;
    let items: Vec<BillItemRow> =
        sqlx::query_as("SELECT * FROM bill_items WHERE bill_id = ? ORDER BY id")
            .bind(id)
            .fetch_all(pool)
            .await?;
    let payments: Vec<PaymentRow> = sqlx::query_as("SELECT * FROM payments WHERE bill_id = ?")
        .bind(id)
        .fetch_all(pool)
        .await?;
    let reports: Vec<ReportRow> = sqlx::query_as("SELECT * FROM reports WHERE bill_id = ?")
        .bind(id)
        .fetch_all(pool)
        .await?;
    let refunded_by_method: Vec<(String, i64)> = sqlx::query_as(
        "SELECT rc.method, COALESCE(SUM(rc.amount_cents),0)
         FROM refund_components rc JOIN refunds r ON r.id = rc.refund_id
         WHERE r.bill_id = ? GROUP BY rc.method",
    )
    .bind(id)
    .fetch_all(pool)
    .await?;
    let refunds = sqlx::query_as::<_, (i64, String, String, String, String, i64, String)>(
        "SELECT r.id, r.refund_no, r.reason, r.reason_detail, u.display_name, r.total_cents, r.created_at
         FROM refunds r JOIN users u ON u.id = r.operator_id
         WHERE r.bill_id = ? ORDER BY r.id DESC",
    )
    .bind(id)
    .fetch_all(pool)
    .await?;
    let patient_name = patient.name.clone();

    let item_views = items
        .iter()
        .map(|item| {
            // 未关联退款的作废报告 => 可经 report_voided 通道退款
            let void_report = reports
                .iter()
                .find(|r| r.bill_item_id == item.id && r.status == "voided" && r.refund_id.is_none());
            let (refundable, non_refundable_reason) = match refund::classify_item(item, void_report)
            {
                Ok(_) => (true, None),
                Err(reason) => (false, Some(reason)),
            };
            let report = reports
                .iter()
                .filter(|r| r.bill_item_id == item.id)
                .max_by_key(|r| r.id)
                .map(|r| ReportBrief {
                    id: r.id,
                    status: r.status.clone(),
                    summary: r.summary.clone(),
                    issued_at: r.issued_at.clone(),
                    void_reason: r.void_reason.clone(),
                    refund_id: r.refund_id,
                });
            ItemView {
                id: item.id,
                code: item.code.clone(),
                name: item.name.clone(),
                list_price_cents: item.list_price_cents,
                discount_cents: item.discount_cents,
                payable_cents: item.payable_cents,
                package_name: item.package_name.clone(),
                status: item.status.clone(),
                cancel_flag: item.cancel_flag.clone(),
                cancel_note: item.cancel_note.clone(),
                refunded_cents: item.refunded_cents,
                refundable,
                non_refundable_reason,
                report,
            }
        })
        .collect();

    Ok(BillDetail {
        id: bill.id,
        bill_no: bill.bill_no.clone(),
        status: bill.status.clone(),
        patient,
        total_list_cents: bill.total_list_cents,
        total_discount_cents: bill.total_discount_cents,
        total_payable_cents: bill.total_payable_cents,
        total_paid_cents: bill.total_paid_cents,
        total_refunded_cents: bill.total_refunded_cents,
        created_at: bill.created_at.clone(),
        items: item_views,
        payments,
        refunded_by_method: refunded_by_method
            .into_iter()
            .map(|(method, amount_cents)| MethodAmount {
                method,
                amount_cents,
            })
            .collect(),
        refunds: refunds
            .into_iter()
            .map(|(rid, refund_no, reason, reason_detail, operator_name, total, created_at)| {
                RefundSummary {
                    id: rid,
                    refund_no,
                    bill_id: id,
                    bill_no: bill.bill_no.clone(),
                    patient_name: patient_name.clone(),
                    reason,
                    reason_detail,
                    operator_name,
                    total_cents: total,
                    created_at,
                }
            })
            .collect(),
    })
}

/// 开单（收费台）：支持单项 + 套餐模板，套餐价按原价比例分摊到各项目
pub async fn create_bill(
    user: AuthUser,
    State(state): State<AppState>,
    Json(req): Json<CreateBillReq>,
) -> Result<Json<BillDetail>, ApiError> {
    user.require(&[Role::Cashier, Role::Admin])?;
    if req.items.is_empty() && req.packages.is_empty() {
        return Err(ApiError::BadRequest("账单至少包含一个项目".into()));
    }
    let allowed_methods = ["insurance", "cash", "wechat", "alipay"];
    for p in &req.payments {
        if !allowed_methods.contains(&p.method.as_str()) {
            return Err(ApiError::BadRequest(format!("非法支付方式: {}", p.method)));
        }
        if p.amount_cents <= 0 {
            return Err(ApiError::BadRequest("支付金额必须为正".into()));
        }
    }

    let mut tx = state.pool.begin().await?;
    let patient_exists: Option<(i64,)> =
        sqlx::query_as("SELECT id FROM patients WHERE id = ?")
            .bind(req.patient_id)
            .fetch_optional(&mut *tx)
            .await?;
    if patient_exists.is_none() {
        return Err(ApiError::BadRequest(format!("患者 {} 不存在", req.patient_id)));
    }

    // 展开为待插入项目：(code, name, list, discount, package_id, package_name)
    let mut rows: Vec<(String, String, i64, i64, Option<i64>, Option<String>)> = Vec::new();
    for it in &req.items {
        if it.list_price_cents <= 0 || it.discount_cents < 0 || it.discount_cents > it.list_price_cents {
            return Err(ApiError::BadRequest(format!("项目「{}」价格或优惠不合法", it.name)));
        }
        rows.push((
            it.code.clone(),
            it.name.clone(),
            it.list_price_cents,
            it.discount_cents,
            None,
            None,
        ));
    }
    for pkg_req in &req.packages {
        let pkg: PackageRow = sqlx::query_as("SELECT * FROM packages WHERE id = ?")
            .bind(pkg_req.package_id)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or_else(|| ApiError::BadRequest(format!("套餐 {} 不存在", pkg_req.package_id)))?;
        let tpl: Vec<serde_json::Value> = serde_json::from_str(&pkg.items_json)
            .map_err(|e| ApiError::Internal(format!("套餐模板损坏: {e}")))?;
        let list_prices: Vec<i64> = tpl
            .iter()
            .map(|v| v["list_price_cents"].as_i64().unwrap_or(0))
            .collect();
        let allocated = allocate_package(pkg.price_cents, &list_prices);
        for (t, alloc) in tpl.iter().zip(allocated.iter()) {
            let list = t["list_price_cents"].as_i64().unwrap_or(0);
            rows.push((
                t["code"].as_str().unwrap_or("").to_string(),
                t["name"].as_str().unwrap_or("").to_string(),
                list,
                list - alloc,
                Some(pkg.id),
                Some(pkg.name.clone()),
            ));
        }
    }

    let total_list: i64 = rows.iter().map(|r| r.2).sum();
    let total_discount: i64 = rows.iter().map(|r| r.3).sum();
    let total_payable = total_list - total_discount;
    let total_paid: i64 = req.payments.iter().map(|p| p.amount_cents).sum();
    if total_paid != total_payable {
        return Err(ApiError::BadRequest(format!(
            "支付合计 {} 分与应付 {} 分不一致",
            total_paid, total_payable
        )));
    }

    let (seq,): (i64,) = sqlx::query_as("SELECT COUNT(*) + 1 FROM bills")
        .fetch_one(&mut *tx)
        .await?;
    let (today,): (String,) = sqlx::query_as("SELECT date('now','localtime')")
        .fetch_one(&mut *tx)
        .await?;
    let bill_no = format!("B{}-{:03}", today.replace('-', ""), seq);

    let bill_id: i64 = sqlx::query(
        "INSERT INTO bills (bill_no, patient_id, total_list_cents, total_discount_cents, total_payable_cents, total_paid_cents)
         VALUES (?,?,?,?,?,?)",
    )
    .bind(&bill_no)
    .bind(req.patient_id)
    .bind(total_list)
    .bind(total_discount)
    .bind(total_payable)
    .bind(total_paid)
    .execute(&mut *tx)
    .await?
    .last_insert_rowid();

    for (code, name, list, discount, pkg_id, pkg_name) in &rows {
        sqlx::query(
            "INSERT INTO bill_items (bill_id, code, name, list_price_cents, discount_cents, payable_cents, package_id, package_name)
             VALUES (?,?,?,?,?,?,?,?)",
        )
        .bind(bill_id)
        .bind(code)
        .bind(name)
        .bind(list)
        .bind(discount)
        .bind(list - discount)
        .bind(pkg_id)
        .bind(pkg_name)
        .execute(&mut *tx)
        .await?;
    }
    for p in &req.payments {
        sqlx::query("INSERT INTO payments (bill_id, method, amount_cents, receipt_no) VALUES (?,?,?,?)")
            .bind(bill_id)
            .bind(&p.method)
            .bind(p.amount_cents)
            .bind(mock_receipt_no(&p.method, "P"))
            .execute(&mut *tx)
            .await?;
    }
    audit(
        &mut tx,
        &user,
        "bill.create",
        "bill",
        bill_id,
        &format!("开单 {bill_no}：{} 项，应付 {} 分", rows.len(), total_payable),
    )
    .await?;
    tx.commit().await?;

    load_bill_detail(&state.pool, bill_id).await.map(Json)
}

// ---------- 医生操作 ----------

/// 标记项目已执行（医生）
pub async fn execute_item(
    user: AuthUser,
    State(state): State<AppState>,
    Path(item_id): Path<i64>,
) -> Result<Json<serde_json::Value>, ApiError> {
    user.require(&[Role::Doctor, Role::Admin])?;
    let item = get_item(&state.pool, item_id).await?;
    if item.status != "pending" {
        return Err(ApiError::Conflict(
            format!("项目当前状态为 {}，不能标记为已执行", item.status),
            vec![],
        ));
    }
    sqlx::query("UPDATE bill_items SET status = 'executed' WHERE id = ?")
        .bind(item_id)
        .execute(&state.pool)
        .await?;
    let mut tx = state.pool.begin().await?;
    audit(&mut tx, &user, "item.execute", "bill_item", item_id, &format!("项目「{}」标记已执行", item.name)).await?;
    tx.commit().await?;
    Ok(Json(json!({"ok": true})))
}

/// 医生标记取消原因（医嘱取消 / 设备故障）
pub async fn flag_item(
    user: AuthUser,
    State(state): State<AppState>,
    Path(item_id): Path<i64>,
    Json(req): Json<FlagReq>,
) -> Result<Json<serde_json::Value>, ApiError> {
    user.require(&[Role::Doctor, Role::Admin])?;
    if let Some(f) = &req.flag {
        if !["doctor_cancel", "equipment_failure"].contains(&f.as_str()) {
            return Err(ApiError::BadRequest(format!("非法标记: {f}")));
        }
    }
    let item = get_item(&state.pool, item_id).await?;
    if item.status != "pending" {
        return Err(ApiError::Conflict(
            "仅待执行项目可标记取消原因".into(),
            vec![],
        ));
    }
    sqlx::query("UPDATE bill_items SET cancel_flag = ?, cancel_note = ? WHERE id = ?")
        .bind(&req.flag)
        .bind(&req.note)
        .bind(item_id)
        .execute(&state.pool)
        .await?;
    let mut tx = state.pool.begin().await?;
    audit(
        &mut tx,
        &user,
        "item.flag",
        "bill_item",
        item_id,
        &format!("项目「{}」标记 {:?}：{}", item.name, req.flag, req.note),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(json!({"ok": true})))
}

// ---------- 报告 ----------

/// 签发报告（报告处理员）：项目进入“已出报告”，之后不可改回未执行
pub async fn issue_report(
    user: AuthUser,
    State(state): State<AppState>,
    Path(item_id): Path<i64>,
    Json(req): Json<ReportReq>,
) -> Result<Json<serde_json::Value>, ApiError> {
    user.require(&[Role::Reporter, Role::Admin])?;
    let item = get_item(&state.pool, item_id).await?;
    if item.status != "executed" {
        return Err(ApiError::Conflict(
            format!("项目状态为 {}，仅已执行项目可签发报告", item.status),
            vec![],
        ));
    }
    let mut tx = state.pool.begin().await?;
    let report_id: i64 = sqlx::query(
        "INSERT INTO reports (bill_item_id, bill_id, summary, issued_by) VALUES (?,?,?,?)",
    )
    .bind(item_id)
    .bind(item.bill_id)
    .bind(&req.summary)
    .bind(user.id)
    .execute(&mut *tx)
    .await?
    .last_insert_rowid();
    sqlx::query("UPDATE bill_items SET status = 'reported' WHERE id = ?")
        .bind(item_id)
        .execute(&mut *tx)
        .await?;
    audit(&mut tx, &user, "report.issue", "report", report_id, &format!("项目「{}」签发报告", item.name)).await?;
    tx.commit().await?;
    Ok(Json(json!({"ok": true, "report_id": report_id})))
}

/// 作废报告（报告处理员）：项目回到“已执行”（绝不回到未执行），作废记录可供退款关联
pub async fn void_report(
    user: AuthUser,
    State(state): State<AppState>,
    Path(report_id): Path<i64>,
    Json(req): Json<VoidReq>,
) -> Result<Json<serde_json::Value>, ApiError> {
    user.require(&[Role::Reporter, Role::Admin])?;
    if req.reason.trim().is_empty() {
        return Err(ApiError::BadRequest("作废原因必填".into()));
    }
    let report: ReportRow = sqlx::query_as("SELECT * FROM reports WHERE id = ?")
        .bind(report_id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("报告 {report_id} 不存在")))?;
    if report.status != "issued" {
        return Err(ApiError::Conflict("报告已作废，不可重复作废".into(), vec![]));
    }
    let mut tx = state.pool.begin().await?;
    sqlx::query(
        "UPDATE reports SET status = 'voided', voided_by = ?, voided_at = datetime('now','localtime'), void_reason = ? WHERE id = ?",
    )
    .bind(user.id)
    .bind(&req.reason)
    .bind(report_id)
    .execute(&mut *tx)
    .await?;
    // 已出报告的项目不能改成未执行：作废后仅回到“已执行”
    sqlx::query("UPDATE bill_items SET status = 'executed' WHERE id = ?")
        .bind(report.bill_item_id)
        .execute(&mut *tx)
        .await?;
    audit(
        &mut tx,
        &user,
        "report.void",
        "report",
        report_id,
        &format!("作废报告 #{}：{}", report_id, req.reason),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(json!({"ok": true})))
}

pub async fn list_reports(
    _user: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<Vec<ReportView>>, ApiError> {
    let rows = sqlx::query(
        "SELECT r.id, r.bill_id, b.bill_no, p.name, r.bill_item_id, i.name, r.status, r.summary,
                iu.display_name, r.issued_at, vu.display_name, r.voided_at, r.void_reason,
                r.refund_id, rf.refund_no
         FROM reports r
         JOIN bill_items i ON i.id = r.bill_item_id
         JOIN bills b ON b.id = r.bill_id
         JOIN patients p ON p.id = b.patient_id
         JOIN users iu ON iu.id = r.issued_by
         LEFT JOIN users vu ON vu.id = r.voided_by
         LEFT JOIN refunds rf ON rf.id = r.refund_id
         ORDER BY r.id DESC",
    )
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(
        rows.iter()
            .map(|r| ReportView {
                id: r.get(0),
                bill_id: r.get(1),
                bill_no: r.get(2),
                patient_name: r.get(3),
                bill_item_id: r.get(4),
                item_name: r.get(5),
                status: r.get(6),
                summary: r.get(7),
                issued_by_name: r.get(8),
                issued_at: r.get(9),
                voided_by_name: r.get(10),
                voided_at: r.get(11),
                void_reason: r.get(12),
                refund_id: r.get(13),
                refund_no: r.get(14),
            })
            .collect(),
    ))
}

// ---------- 退款 ----------

pub async fn preview_refund(
    user: AuthUser,
    State(state): State<AppState>,
    Path(bill_id): Path<i64>,
    Json(req): Json<RefundReq>,
) -> Result<Json<RefundPreview>, ApiError> {
    user.require(&[Role::Cashier, Role::Admin])?;
    if req.item_ids.is_empty() {
        return Err(ApiError::BadRequest("未选择任何退项项目".into()));
    }
    refund::preview_refund(&state.pool, bill_id, &req.item_ids)
        .await
        .map(Json)
}

pub async fn create_refund(
    user: AuthUser,
    State(state): State<AppState>,
    Path(bill_id): Path<i64>,
    Json(req): Json<RefundReq>,
) -> Result<Json<RefundDetail>, ApiError> {
    user.require(&[Role::Cashier, Role::Admin])?;
    refund::create_refund(&state.pool, &user, bill_id, &req)
        .await
        .map(Json)
}

pub async fn list_refunds(
    _user: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<Vec<RefundSummary>>, ApiError> {
    let rows = sqlx::query_as::<_, (i64, String, i64, String, String, String, String, String, i64, String)>(
        "SELECT r.id, r.refund_no, r.bill_id, b.bill_no, p.name, r.reason, r.reason_detail,
                u.display_name, r.total_cents, r.created_at
         FROM refunds r
         JOIN bills b ON b.id = r.bill_id
         JOIN patients p ON p.id = b.patient_id
         JOIN users u ON u.id = r.operator_id
         ORDER BY r.id DESC",
    )
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(
        rows.into_iter()
            .map(
                |(id, refund_no, bill_id, bill_no, patient_name, reason, reason_detail, operator_name, total, created_at)| {
                    RefundSummary {
                        id,
                        refund_no,
                        bill_id,
                        bill_no,
                        patient_name,
                        reason,
                        reason_detail,
                        operator_name,
                        total_cents: total,
                        created_at,
                    }
                },
            )
            .collect(),
    ))
}

pub async fn get_refund(
    _user: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<RefundDetail>, ApiError> {
    refund::load_refund_detail(&state.pool, id).await.map(Json)
}

// ---------- 基础数据 ----------

#[derive(Debug, serde::Deserialize)]
pub struct ItemsQuery {
    pub status: Option<String>,
}

/// 按状态列出项目（报告管理页“待签发”等场景）
pub async fn list_items(
    _user: AuthUser,
    State(state): State<AppState>,
    axum::extract::Query(q): axum::extract::Query<ItemsQuery>,
) -> Result<Json<Vec<serde_json::Value>>, ApiError> {
    let rows = sqlx::query(
        "SELECT i.id, i.code, i.name, i.status, i.payable_cents, i.bill_id, b.bill_no, p.name
         FROM bill_items i
         JOIN bills b ON b.id = i.bill_id
         JOIN patients p ON p.id = b.patient_id
         WHERE (?1 IS NULL OR i.status = ?1)
         ORDER BY i.id DESC",
    )
    .bind(&q.status)
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(
        rows.iter()
            .map(|r| {
                json!({
                    "id": r.get::<i64, _>(0),
                    "code": r.get::<String, _>(1),
                    "name": r.get::<String, _>(2),
                    "status": r.get::<String, _>(3),
                    "payable_cents": r.get::<i64, _>(4),
                    "bill_id": r.get::<i64, _>(5),
                    "bill_no": r.get::<String, _>(6),
                    "patient_name": r.get::<String, _>(7),
                })
            })
            .collect(),
    ))
}

pub async fn list_patients(
    _user: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<Vec<PatientRow>>, ApiError> {
    Ok(Json(
        sqlx::query_as("SELECT * FROM patients ORDER BY id")
            .fetch_all(&state.pool)
            .await?,
    ))
}

pub async fn list_packages(
    _user: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<Vec<PackageRow>>, ApiError> {
    Ok(Json(
        sqlx::query_as("SELECT * FROM packages ORDER BY id")
            .fetch_all(&state.pool)
            .await?,
    ))
}

pub async fn bill_audit(
    _user: AuthUser,
    State(state): State<AppState>,
    Path(bill_id): Path<i64>,
) -> Result<Json<Vec<AuditView>>, ApiError> {
    let rows = sqlx::query(
        "SELECT id, actor_name, actor_role, action, entity, entity_id, detail, created_at
         FROM audit_logs
         WHERE (entity = 'bill' AND entity_id = ?)
            OR (entity = 'bill_item' AND entity_id IN (SELECT id FROM bill_items WHERE bill_id = ?))
            OR (entity = 'refund' AND entity_id IN (SELECT id FROM refunds WHERE bill_id = ?))
            OR (entity = 'report' AND entity_id IN (SELECT id FROM reports WHERE bill_id = ?))
         ORDER BY id DESC",
    )
    .bind(bill_id)
    .bind(bill_id)
    .bind(bill_id)
    .bind(bill_id)
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(
        rows.iter()
            .map(|r| AuditView {
                id: r.get(0),
                actor_name: r.get(1),
                actor_role: r.get(2),
                action: r.get(3),
                entity: r.get(4),
                entity_id: r.get(5),
                detail: r.get(6),
                created_at: r.get(7),
            })
            .collect(),
    ))
}

// ---------- 工具 ----------

async fn get_item(pool: &sqlx::SqlitePool, item_id: i64) -> Result<BillItemRow, ApiError> {
    sqlx::query_as("SELECT * FROM bill_items WHERE id = ?")
        .bind(item_id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("项目 {item_id} 不存在")))
}

async fn audit(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    actor: &AuthUser,
    action: &str,
    entity: &str,
    entity_id: i64,
    detail: &str,
) -> Result<(), ApiError> {
    sqlx::query(
        "INSERT INTO audit_logs (actor_id, actor_name, actor_role, action, entity, entity_id, detail)
         VALUES (?,?,?,?,?,?,?)",
    )
    .bind(actor.id)
    .bind(&actor.display_name)
    .bind(actor.role.as_str())
    .bind(action)
    .bind(entity)
    .bind(entity_id)
    .bind(detail)
    .execute(&mut **tx)
    .await?;
    Ok(())
}
