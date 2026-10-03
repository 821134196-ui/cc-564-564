use serde::Serialize;

#[derive(Debug, Clone, sqlx::FromRow, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Patient {
    pub id: String,
    pub name: String,
    pub identifier: String,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Bill {
    pub id: String,
    pub patient_id: String,
    pub bill_no: String,
    pub status: String,
    pub created_at: String,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BillItem {
    pub id: String,
    pub bill_id: String,
    pub parent_item_id: Option<String>,
    pub code: String,
    pub name: String,
    pub category: String,
    pub unit_price: i64,
    pub list_unit_price: i64,
    pub quantity: i64,
    pub executed_qty: i64,
    pub refunded_qty: i64,
    pub discount_total: i64,
    pub status: String,
    pub cancel_reason: Option<String>,
    pub cancel_remark: Option<String>,
    pub cancelled_by: Option<String>,
    pub cancelled_at: Option<String>,
    pub report_status: String,
    pub sort_no: i64,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Payment {
    pub id: String,
    pub bill_id: String,
    pub method: String,
    pub amount: i64,
    pub receipt_no: String,
    pub paid_at: String,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Refund {
    pub id: String,
    pub bill_id: String,
    pub bill_item_id: String,
    pub status: String,
    pub cancel_reason: Option<String>,
    pub cancel_remark: Option<String>,
    pub refund_qty: i64,
    pub unit_list_price: i64,
    pub gross_refund: i64,
    pub discount_share: i64,
    pub net_refund: i64,
    pub insurance_amount: i64,
    pub self_pay_amount: i64,
    pub payment_ratio: String,
    pub calc_detail: String,
    pub insurance_receipt_no: Option<String>,
    pub payment_receipt_no: Option<String>,
    pub requested_by: String,
    pub confirmed_by: Option<String>,
    pub report_void_id: Option<String>,
    pub reject_reason: Option<String>,
    pub created_at: String,
    pub completed_at: Option<String>,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub id: String,
    pub bill_item_id: String,
    pub report_no: String,
    pub status: String,
    pub issued_by: String,
    pub issued_at: String,
    pub voided_at: Option<String>,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportVoid {
    pub id: String,
    pub report_id: String,
    pub bill_item_id: String,
    pub reason: String,
    pub voided_by: String,
    pub refund_id: Option<String>,
    pub created_at: String,
}
