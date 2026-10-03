use crate::auth::*;
use crate::error::{AppError, AppResult};
use crate::models::*;
use crate::service;
use crate::state::AppState;
use axum::extract::{Path, Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/me", get(me))
        .route("/api/users", get(list_users))
        .route("/api/bills", get(list_bills))
        .route("/api/bills/:bill_id", get(get_bill))
        .route("/api/bills/:bill_id/items/:item_id/execute", post(mark_executed))
        .route("/api/bills/:bill_id/items/:item_id/cancel", post(cancel_item))
        .route("/api/bills/:bill_id/items/:item_id/issue-report", post(issue_report))
        .route("/api/bills/:bill_id/refunds", post(create_refund))
        .route("/api/bills/:bill_id/refunds", get(list_refunds))
        .route("/api/reports/:report_id/void", post(void_report))
        .route("/api/refunds/:refund_id/trace", get(trace_refund))
        .route("/api/receipts", get(list_receipts))
}

async fn me(AuthUser(user): AuthUser) -> Json<Value> {
    Json(json!({
        "id": user.id, "name": user.name, "role": user.role, "patientId": user.patient_id
    }))
}

async fn list_users(State(state): State<AppState>) -> AppResult<Json<Vec<Value>>> {
    let users = sqlx::query_as::<_, User>("SELECT id,name,role,token,patient_id FROM users ORDER BY role,id")
        .fetch_all(&state.pool)
        .await?;
    Ok(Json(
        users
            .iter()
            .map(|u| json!({"id": u.id, "name": u.name, "role": u.role, "token": u.token, "patientId": u.patient_id}))
            .collect(),
    ))
}

async fn list_bills(
    AuthUser(user): AuthUser,
    State(state): State<AppState>,
) -> AppResult<Json<Vec<Bill>>> {
    let bills = if user.role() == Role::Patient {
        let pid = user.patient_id.as_deref().unwrap_or("");
        sqlx::query_as::<_, Bill>("SELECT * FROM bills WHERE patient_id = ?1 ORDER BY created_at")
            .bind(pid)
            .fetch_all(&state.pool)
            .await?
    } else {
        sqlx::query_as::<_, Bill>("SELECT * FROM bills ORDER BY created_at")
            .fetch_all(&state.pool)
            .await?
    };
    Ok(Json(bills))
}

/// 患者只能看自己的账单；其他岗位可读全部
async fn authorize_bill_read(state: &AppState, user: &User, bill_id: &str) -> AppResult<()> {
    if user.role() == Role::Patient {
        let row: Option<(String,)> = sqlx::query_as("SELECT patient_id FROM bills WHERE id = ?1")
            .bind(bill_id)
            .fetch_optional(&state.pool)
            .await?;
        match row {
            None => return Err(AppError::NotFound(format!("账单 {bill_id} 不存在"))),
            Some((pid,)) if Some(&pid) != user.patient_id.as_ref() => {
                return Err(AppError::Forbidden("只能查看本人账单".into()))
            }
            _ => {}
        }
    }
    Ok(())
}

async fn get_bill(
    AuthUser(user): AuthUser,
    State(state): State<AppState>,
    Path(bill_id): Path<String>,
) -> AppResult<Json<Value>> {
    authorize_bill_read(&state, &user, &bill_id).await?;
    let view = service::get_bill_view(&state, &bill_id).await?;
    Ok(Json(serde_json::to_value(&view).unwrap()))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct QtyBody {
    qty: i64,
}

async fn mark_executed(
    RequireDoctor(user): RequireDoctor,
    State(state): State<AppState>,
    Path((bill_id, item_id)): Path<(String, String)>,
    Json(body): Json<QtyBody>,
) -> AppResult<Json<BillItem>> {
    let item = service::mark_executed(&state, &item_id, body.qty, &user.id).await?;
    if item.bill_id != bill_id {
        return Err(AppError::NotFound("该项目不属于此账单".into()));
    }
    Ok(Json(item))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CancelBody {
    reason: String,
    #[serde(default)]
    remark: Option<String>,
}

async fn cancel_item(
    RequireDoctor(user): RequireDoctor,
    State(state): State<AppState>,
    Path((bill_id, item_id)): Path<(String, String)>,
    Json(body): Json<CancelBody>,
) -> AppResult<Json<BillItem>> {
    let item =
        service::cancel_item(&state, &item_id, &body.reason, body.remark.as_deref(), &user.id)
            .await?;
    if item.bill_id != bill_id {
        return Err(AppError::NotFound("该项目不属于此账单".into()));
    }
    Ok(Json(item))
}

async fn issue_report(
    RequireReportStaff(user): RequireReportStaff,
    State(state): State<AppState>,
    Path((bill_id, item_id)): Path<(String, String)>,
) -> AppResult<Json<Report>> {
    let item = sqlx::query_as::<_, BillItem>("SELECT * FROM bill_items WHERE id = ?1")
        .bind(&item_id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::NotFound("检查项目不存在".into()))?;
    if item.bill_id != bill_id {
        return Err(AppError::NotFound("该项目不属于此账单".into()));
    }
    service::issue_report(&state, &item_id, &user.id).await.map(Json)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RefundBody {
    item_id: String,
    qty: i64,
    cancel_reason: String,
    #[serde(default)]
    cancel_remark: Option<String>,
}

async fn create_refund(
    RequireCashier(user): RequireCashier,
    State(state): State<AppState>,
    Path(bill_id): Path<String>,
    Json(body): Json<RefundBody>,
) -> AppResult<Json<Refund>> {
    // 确认项目属于该账单
    let item_bill: String = sqlx::query_scalar("SELECT bill_id FROM bill_items WHERE id = ?1")
        .bind(&body.item_id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::NotFound("检查项目不存在".into()))?;
    if item_bill != bill_id {
        return Err(AppError::NotFound("该项目不属于此账单".into()));
    }
    service::create_refund(
        &state,
        service::CreateRefundInput {
            item_id: body.item_id,
            qty: body.qty,
            cancel_reason: body.cancel_reason,
            cancel_remark: body.cancel_remark,
            cashier_id: user.id,
        },
    )
    .await
    .map(Json)
}

async fn list_refunds(
    AuthUser(user): AuthUser,
    State(state): State<AppState>,
    Path(bill_id): Path<String>,
    Query(_q): Query<HashMap<String, String>>,
) -> AppResult<Json<Vec<Refund>>> {
    authorize_bill_read(&state, &user, &bill_id).await?;
    let refunds = sqlx::query_as::<_, Refund>(
        "SELECT * FROM refunds WHERE bill_id = ?1 ORDER BY created_at",
    )
    .bind(&bill_id)
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(refunds))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct VoidBody {
    reason: String,
}

async fn void_report(
    RequireReportStaff(user): RequireReportStaff,
    State(state): State<AppState>,
    Path(report_id): Path<String>,
    Json(body): Json<VoidBody>,
) -> AppResult<Json<Value>> {
    let (void_record, refund) =
        service::void_report(&state, &report_id, &body.reason, &user.id).await?;
    Ok(Json(json!({ "reportVoid": void_record, "refund": refund })))
}

/// 退款与报告作废记录相互追查
async fn trace_refund(
    AuthUser(user): AuthUser,
    State(state): State<AppState>,
    Path(refund_id): Path<String>,
) -> AppResult<Json<Value>> {
    let refund = sqlx::query_as::<_, Refund>("SELECT * FROM refunds WHERE id = ?1")
        .bind(&refund_id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::NotFound("退款单不存在".into()))?;

    if user.role() == Role::Patient {
        let row: (String,) = sqlx::query_as("SELECT patient_id FROM bills WHERE id = ?1")
            .bind(&refund.bill_id)
            .fetch_one(&state.pool)
            .await?;
        if Some(&row.0) != user.patient_id.as_ref() {
            return Err(AppError::Forbidden("只能追查本人账单的退款".into()));
        }
    }

    let void_record = match &refund.report_void_id {
        Some(void_id) => {
            sqlx::query_as::<_, ReportVoid>("SELECT * FROM report_voids WHERE id = ?1")
                .bind(void_id)
                .fetch_optional(&state.pool)
                .await?
        }
        None => None,
    };
    let report = match &void_record {
        Some(v) => {
            sqlx::query_as::<_, Report>("SELECT * FROM reports WHERE id = ?1")
                .bind(&v.report_id)
                .fetch_optional(&state.pool)
                .await?
        }
        None => None,
    };
    Ok(Json(json!({
        "refund": refund,
        "reportVoid": void_record,
        "report": report
    })))
}

async fn list_receipts(
    RequireStaff(_user): RequireStaff,
    State(state): State<AppState>,
) -> Json<Value> {
    Json(json!({ "receipts": state.gateway.all_receipts() }))
}
