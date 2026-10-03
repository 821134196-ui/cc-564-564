//! calc_refund 纯计算边界测试：比例拆分、总额上限、分渠道上限与溢出转移。

use clinic_refund::models::{BillItem, Payment};
use clinic_refund::service::calc_refund;

fn item(unit: i64, list: i64, qty: i64) -> BillItem {
    BillItem {
        id: "it_x".into(),
        bill_id: "bill".into(),
        parent_item_id: None,
        code: "X".into(),
        name: "X".into(),
        category: "STANDALONE".into(),
        unit_price: unit,
        list_unit_price: list,
        quantity: qty,
        executed_qty: 0,
        refunded_qty: 0,
        discount_total: 0,
        status: "SCHEDULED".into(),
        cancel_reason: None,
        cancel_remark: None,
        cancelled_by: None,
        cancelled_at: None,
        report_status: "NONE".into(),
        sort_no: 0,
    }
}

fn payments(ins: i64, self_pay: i64) -> Vec<Payment> {
    vec![
        Payment {
            id: "p1".into(),
            bill_id: "bill".into(),
            method: "INSURANCE".into(),
            amount: ins,
            receipt_no: "INS".into(),
            paid_at: "t".into(),
        },
        Payment {
            id: "p2".into(),
            bill_id: "bill".into(),
            method: "SELF_PAY".into(),
            amount: self_pay,
            receipt_no: "SELF".into(),
            paid_at: "t".into(),
        },
    ]
}

#[test]
fn proportional_split_and_discount() {
    let it = item(90, 100, 2); // 原价 200，优惠 20，实退 180
    let pays = payments(1000, 1000);
    let c = calc_refund(&it, 2, &pays, 0, 0, 0).unwrap();
    assert_eq!(c.gross, 200);
    assert_eq!(c.discount_share, 20);
    assert_eq!(c.net, 180);
    assert_eq!(c.insurance, 90);
    assert_eq!(c.self_pay, 90);
    assert_eq!(c.insurance + c.self_pay, c.net);
}

#[test]
fn total_cap_cannot_exceed_original_payment() {
    let it = item(100, 100, 2);
    let pays = payments(100, 100); // 原付 200
    // 已退 150，本次再退 100 → 250 > 200 必须拦截
    let err = calc_refund(&it, 1, &pays, 150, 75, 75).unwrap_err();
    let s = format!("{err:?}");
    assert!(s.contains("超过原付款"), "{s}");
}

#[test]
fn insurance_cap_overflow_moves_to_self_pay() {
    let it = item(100, 100, 1);
    // 保险/自费各 100；保险已退 80（剩余 20），自费已退 0（剩余 100）
    let pays = payments(100, 100);
    let c = calc_refund(&it, 1, &pays, 80, 80, 0).unwrap();
    assert_eq!(c.insurance, 20, "保险受渠道上限封顶");
    assert_eq!(c.self_pay, 80, "溢出部分转由自费承担");
}

#[test]
fn both_channel_caps_exhausted_rejected() {
    let it = item(100, 100, 1);
    let pays = payments(100, 100);
    // 总剩余 100，但自费剩余为 0、保险剩余 100，比例拆分会给自费 50 → 拦截
    let err = calc_refund(&it, 1, &pays, 100, 100, 100);
    assert!(err.is_err(), "渠道额度不足必须拦截");
}

#[test]
fn rounding_remainder_goes_to_self_pay() {
    let it = item(1, 1, 1);
    let pays = payments(1, 2); // 保险 1/3
    let c = calc_refund(&it, 1, &pays, 0, 0, 0).unwrap();
    assert_eq!(c.insurance, 0);
    assert_eq!(c.self_pay, 1, "基点整数运算尾差归自费");
}
