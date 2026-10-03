//! 账单聚合视图：在原始行数据之上计算执行状态、原价、优惠分摊、
//! 可退数量/金额以及不能退款的原因，并汇总调整前后的总金额。

use crate::models::*;
use serde::Serialize;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemView {
    #[serde(flatten)]
    pub item: BillItem,
    pub original_total: i64,      // 原价合计 = list_unit_price * quantity
    pub paid_total: i64,          // 成交合计 = unit_price * quantity（优惠分摊后）
    pub discount_total_amount: i64,
    pub executed_amount: i64,     // 已执行部分成交额
    pub refunded_amount: i64,     // 已退款净额
    pub refundable_qty: i64,      // 可退数量
    pub refundable_amount: i64,   // 可退净额
    pub non_refundable_reason: Option<String>,
    pub refunds: Vec<Refund>,
    pub children: Vec<ItemView>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BillView {
    #[serde(flatten)]
    pub bill: Bill,
    pub patient: Patient,
    pub items: Vec<ItemView>,
    pub payments: Vec<Payment>,
    pub reports: Vec<Report>,
    // 金额汇总（分）
    pub original_total: i64,       // 调整前：所有项目原价合计
    pub discount_total: i64,       // 优惠分摊合计
    pub paid_total: i64,           // 调整前：患者实付合计（= 付款构成之和）
    pub refunded_total: i64,       // 累计已退
    pub insurance_refunded: i64,
    pub self_pay_refunded: i64,
    pub adjusted_total: i64,       // 调整后应付 = paid - refunded
    pub payment_breakdown: PaymentBreakdown,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PaymentBreakdown {
    pub insurance_original: i64,
    pub self_pay_original: i64,
    pub insurance_ratio_bp: i64, // 保险占比（基点 0..10000）
    pub self_pay_ratio_bp: i64,
}

pub fn build_view(
    bill: Bill,
    patient: Patient,
    mut items: Vec<BillItem>,
    payments: Vec<Payment>,
    refunds: Vec<Refund>,
    reports: Vec<Report>,
) -> BillView {
    items.sort_by_key(|i| i.sort_no);

    let insurance_original = payments
        .iter()
        .filter(|p| p.method == "INSURANCE")
        .map(|p| p.amount)
        .sum::<i64>();
    let self_pay_original = payments
        .iter()
        .filter(|p| p.method == "SELF_PAY")
        .map(|p| p.amount)
        .sum::<i64>();
    let paid_total = insurance_original + self_pay_original;
    let (ins_bp, self_bp) = if paid_total > 0 {
        let b = insurance_original * 10_000 / paid_total;
        (b, 10_000 - b)
    } else {
        (0, 0)
    };

    let refunds_for = |id: &str| {
        let mut rs: Vec<Refund> = refunds.iter().filter(|r| r.bill_item_id == id).cloned().collect();
        rs.sort_by_key(|r| r.created_at.clone());
        rs
    };

    let build_item = |item: BillItem| -> ItemView {
        let original_total = item.list_unit_price * item.quantity;
        let paid = item.unit_price * item.quantity;
        let item_refunds = refunds_for(&item.id);
        let refunded_qty = item_refunds
            .iter()
            .filter(|r| r.status == "COMPLETED")
            .map(|r| r.refund_qty)
            .sum::<i64>()
            .max(item.refunded_qty);
        let refunded_amount = item_refunds
            .iter()
            .filter(|r| r.status == "COMPLETED")
            .map(|r| r.net_refund)
            .sum();

        // 可退数量 = 总数 - 已执行 - 已退（部分执行时只退未执行部分）
        let refundable_qty =
            (item.quantity - item.executed_qty - refunded_qty).max(0);

        let executed_amount = item.unit_price * item.executed_qty;
        let refundable_amount = item.unit_price * refundable_qty;

        let non_refundable_reason = decide_block_reason(&item, refundable_qty, &item_refunds);

        ItemView {
            original_total,
            paid_total: paid,
            discount_total_amount: original_total - paid,
            executed_amount,
            refunded_amount,
            refundable_qty,
            refundable_amount,
            non_refundable_reason,
            refunds: item_refunds,
            item,
            children: Vec::new(),
        }
    };

    // 组装套餐父子结构（套餐父行计价，组件展示构成）
    let mut all_views: Vec<ItemView> = items.into_iter().map(build_item).collect();
    let mut component_map: std::collections::HashMap<String, Vec<ItemView>> =
        std::collections::HashMap::new();
    let mut roots: Vec<ItemView> = Vec::new();
    for v in all_views.drain(..) {
        if let Some(pid) = v.item.parent_item_id.clone() {
            component_map.entry(pid).or_default().push(v);
        } else {
            roots.push(v);
        }
    }
    for root in roots.iter_mut() {
        if let Some(mut kids) = component_map.remove(&root.item.id) {
            kids.sort_by_key(|k| k.item.sort_no);
            // 套餐父行本身不承载单价成交：成交价/已执行/可退金额均取子项之和，
            // 原价与优惠仍展示套餐口径
            if root.item.category == "PACKAGE" {
                let paid: i64 = kids.iter().map(|k| k.paid_total).sum();
                let executed: i64 = kids.iter().map(|k| k.executed_amount).sum();
                let refunded: i64 = kids.iter().map(|k| k.refunded_amount).sum();
                let refundable_amt: i64 = kids.iter().map(|k| k.refundable_amount).sum();
                root.paid_total = paid;
                root.discount_total_amount = root.original_total - paid;
                root.executed_amount = executed;
                root.refunded_amount = refunded;
                root.refundable_qty = 0;
                root.refundable_amount = refundable_amt; // 供前台展示套餐整体可退额度
            }
            root.children = kids;
        }
    }
    roots.sort_by_key(|r| r.item.sort_no);
    let top = roots;

    // 汇总（只统计顶层行：套餐父行计价，组件不重复计）
    let mut original_total = 0;
    let mut discount_total = 0;
    let mut refunded_total = 0;
    let mut insurance_refunded = 0;
    let mut self_pay_refunded = 0;
    for v in &top {
        original_total += v.original_total;
        discount_total += v.discount_total_amount;
        for r in &v.refunds {
            if r.status == "COMPLETED" {
                refunded_total += r.net_refund;
                insurance_refunded += r.insurance_amount;
                self_pay_refunded += r.self_pay_amount;
            }
        }
        for c in &v.children {
            for r in &c.refunds {
                if r.status == "COMPLETED" {
                    refunded_total += r.net_refund;
                    insurance_refunded += r.insurance_amount;
                    self_pay_refunded += r.self_pay_amount;
                }
            }
        }
    }

    BillView {
        bill,
        patient,
        items: top,
        payments,
        reports,
        original_total,
        discount_total,
        paid_total,
        refunded_total,
        insurance_refunded,
        self_pay_refunded,
        adjusted_total: paid_total - refunded_total,
        payment_breakdown: PaymentBreakdown {
            insurance_original,
            self_pay_original,
            insurance_ratio_bp: ins_bp,
            self_pay_ratio_bp: self_bp,
        },
    }
}

/// 判定某行为何当前不能退（None 表示可退）
fn decide_block_reason(
    item: &BillItem,
    refundable_qty: i64,
    refunds: &[Refund],
) -> Option<String> {
    if item.category == "PACKAGE" {
        return Some("套餐项目请按具体子项退项".into());
    }
    let pending = refunds.iter().any(|r| r.status == "AWAITING_VOID");
    if pending {
        return Some("已有退项申请等待报告作废处理".into());
    }
    // 已全部执行：即使报告作废也无未执行数量可退，优先提示
    if item.quantity > 0 && item.executed_qty >= item.quantity {
        return Some("项目已全部执行，不可退".into());
    }
    if item.report_status == "ISSUED" {
        return Some("项目已出报告，须先由报告处理人员作废报告后方可退款".into());
    }
    if refundable_qty <= 0 {
        return Some("已无未执行数量（可能已退）".into());
    }
    None
}
