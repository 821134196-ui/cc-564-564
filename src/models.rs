use serde::{Deserialize, Serialize};
use sqlx::FromRow;

// ---------- 数据库行 ----------

#[derive(Debug, Clone, FromRow)]
pub struct UserRow {
    pub id: i64,
    pub username: String,
    pub password_hash: String,
    pub display_name: String,
    pub role: String,
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct PatientRow {
    pub id: i64,
    pub name: String,
    pub gender: String,
    pub age: i64,
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct PackageRow {
    pub id: i64,
    pub name: String,
    pub price_cents: i64,
    pub items_json: String,
}

#[derive(Debug, Clone, FromRow)]
pub struct BillRow {
    pub id: i64,
    pub bill_no: String,
    pub patient_id: i64,
    pub status: String,
    pub total_list_cents: i64,
    pub total_discount_cents: i64,
    pub total_payable_cents: i64,
    pub total_paid_cents: i64,
    pub total_refunded_cents: i64,
    pub created_at: String,
}

#[derive(Debug, Clone, FromRow)]
pub struct BillItemRow {
    pub id: i64,
    pub bill_id: i64,
    pub code: String,
    pub name: String,
    pub list_price_cents: i64,
    pub discount_cents: i64,
    pub payable_cents: i64,
    pub package_id: Option<i64>,
    pub package_name: Option<String>,
    pub status: String,
    pub cancel_flag: Option<String>,
    pub cancel_note: Option<String>,
    pub refunded_cents: i64,
    pub created_at: String,
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct PaymentRow {
    pub id: i64,
    pub bill_id: i64,
    pub method: String,
    pub amount_cents: i64,
    pub receipt_no: String,
    pub created_at: String,
}

#[derive(Debug, Clone, FromRow)]
pub struct ReportRow {
    pub id: i64,
    pub bill_item_id: i64,
    pub bill_id: i64,
    pub status: String,
    pub summary: String,
    pub issued_by: i64,
    pub issued_at: String,
    pub voided_by: Option<i64>,
    pub voided_at: Option<String>,
    pub void_reason: Option<String>,
    pub refund_id: Option<i64>,
}

#[derive(Debug, Clone, FromRow)]
pub struct RefundRow {
    pub id: i64,
    pub refund_no: String,
    pub bill_id: i64,
    pub reason: String,
    pub reason_detail: String,
    pub operator_id: i64,
    pub status: String,
    pub total_cents: i64,
    pub idempotency_key: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, FromRow)]
pub struct RefundItemRow {
    pub id: i64,
    pub refund_id: i64,
    pub bill_item_id: i64,
    pub amount_cents: i64,
    pub calc_detail: String,
    pub report_void_id: Option<i64>,
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct RefundComponentRow {
    pub id: i64,
    pub refund_id: i64,
    pub method: String,
    pub amount_cents: i64,
    pub receipt_no: String,
}

// ---------- API DTO ----------

#[derive(Debug, Deserialize)]
pub struct LoginReq {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct UserInfo {
    pub id: i64,
    pub username: String,
    pub display_name: String,
    pub role: String,
}

#[derive(Debug, Serialize)]
pub struct ItemView {
    pub id: i64,
    pub code: String,
    pub name: String,
    pub list_price_cents: i64,
    pub discount_cents: i64,
    pub payable_cents: i64,
    pub package_name: Option<String>,
    pub status: String,
    pub cancel_flag: Option<String>,
    pub cancel_note: Option<String>,
    pub refunded_cents: i64,
    pub refundable: bool,
    /// 不可退原因（可退时为 null），患者端直接展示
    pub non_refundable_reason: Option<String>,
    pub report: Option<ReportBrief>,
}

#[derive(Debug, Serialize, Clone)]
pub struct ReportBrief {
    pub id: i64,
    pub status: String,
    pub summary: String,
    pub issued_at: String,
    pub void_reason: Option<String>,
    pub refund_id: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct BillDetail {
    pub id: i64,
    pub bill_no: String,
    pub status: String,
    pub patient: PatientRow,
    pub total_list_cents: i64,
    pub total_discount_cents: i64,
    pub total_payable_cents: i64,
    pub total_paid_cents: i64,
    pub total_refunded_cents: i64,
    pub created_at: String,
    pub items: Vec<ItemView>,
    pub payments: Vec<PaymentRow>,
    /// 各渠道已退金额（用于核对“不超过原付款”）
    pub refunded_by_method: Vec<MethodAmount>,
    pub refunds: Vec<RefundSummary>,
}

#[derive(Debug, Serialize, Clone)]
pub struct MethodAmount {
    pub method: String,
    pub amount_cents: i64,
}

#[derive(Debug, Serialize)]
pub struct BillSummary {
    pub id: i64,
    pub bill_no: String,
    pub status: String,
    pub patient_name: String,
    pub total_payable_cents: i64,
    pub total_paid_cents: i64,
    pub total_refunded_cents: i64,
    pub item_count: i64,
    pub created_at: String,
}

#[derive(Debug, Serialize)]
pub struct RefundSummary {
    pub id: i64,
    pub refund_no: String,
    pub bill_id: i64,
    pub bill_no: String,
    pub patient_name: String,
    pub reason: String,
    pub reason_detail: String,
    pub operator_name: String,
    pub total_cents: i64,
    pub created_at: String,
}

#[derive(Debug, Serialize, Clone)]
pub struct SkippedItem {
    pub item_id: i64,
    pub name: String,
    pub reason: String,
}

#[derive(Debug, Deserialize)]
pub struct RefundReq {
    pub item_ids: Vec<i64>,
    pub reason: String,
    #[serde(default)]
    pub reason_detail: String,
    pub idempotency_key: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct RefundPreview {
    pub items: Vec<RefundItemPreview>,
    pub skipped: Vec<SkippedItem>,
    pub total_cents: i64,
    pub components: Vec<MethodAmount>,
}

#[derive(Debug, Serialize)]
pub struct RefundItemPreview {
    pub item_id: i64,
    pub name: String,
    pub amount_cents: i64,
    pub calc_detail: serde_json::Value,
    pub report_void_id: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct RefundDetail {
    pub id: i64,
    pub refund_no: String,
    pub bill_id: i64,
    pub reason: String,
    pub reason_detail: String,
    pub operator_name: String,
    pub total_cents: i64,
    pub created_at: String,
    pub items: Vec<RefundItemView>,
    pub components: Vec<RefundComponentRow>,
    /// 与本次退款关联的报告作废记录（可双向追查）
    pub linked_reports: Vec<ReportBrief>,
    pub skipped: Vec<SkippedItem>,
    /// 是否幂等重放（重复提交返回原单）
    pub replayed: bool,
}

#[derive(Debug, Serialize)]
pub struct RefundItemView {
    pub bill_item_id: i64,
    pub name: String,
    pub amount_cents: i64,
    pub calc_detail: serde_json::Value,
    pub report_void_id: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct CreateBillReq {
    pub patient_id: i64,
    #[serde(default)]
    pub items: Vec<CreateBillItem>,
    #[serde(default)]
    pub packages: Vec<CreateBillPackage>,
    pub payments: Vec<CreateBillPayment>,
}

#[derive(Debug, Deserialize)]
pub struct CreateBillItem {
    pub code: String,
    pub name: String,
    pub list_price_cents: i64,
    #[serde(default)]
    pub discount_cents: i64,
}

#[derive(Debug, Deserialize)]
pub struct CreateBillPackage {
    pub package_id: i64,
}

#[derive(Debug, Deserialize)]
pub struct CreateBillPayment {
    pub method: String,
    pub amount_cents: i64,
}

#[derive(Debug, Deserialize)]
pub struct FlagReq {
    pub flag: Option<String>,
    #[serde(default)]
    pub note: String,
}

#[derive(Debug, Deserialize)]
pub struct ReportReq {
    #[serde(default)]
    pub summary: String,
}

#[derive(Debug, Deserialize)]
pub struct VoidReq {
    pub reason: String,
}

#[derive(Debug, Serialize)]
pub struct ReportView {
    pub id: i64,
    pub bill_id: i64,
    pub bill_no: String,
    pub patient_name: String,
    pub bill_item_id: i64,
    pub item_name: String,
    pub status: String,
    pub summary: String,
    pub issued_by_name: String,
    pub issued_at: String,
    pub voided_by_name: Option<String>,
    pub voided_at: Option<String>,
    pub void_reason: Option<String>,
    pub refund_id: Option<i64>,
    pub refund_no: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct AuditView {
    pub id: i64,
    pub actor_name: Option<String>,
    pub actor_role: Option<String>,
    pub action: String,
    pub entity: String,
    pub entity_id: Option<i64>,
    pub detail: String,
    pub created_at: String,
}
