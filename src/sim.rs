//! 保险冲账 / 自费退款回执的本地模拟网关。
//! 真实环境中替换为保险核心系统与支付渠道调用；此处仅生成回执号并记录。

use std::sync::Mutex;

#[derive(Debug, Clone, serde::Serialize)]
pub struct RefundReceipt {
    pub receipt_no: String,
    pub channel: String,
    pub amount: i64,
    pub original_receipt_no: String,
    pub status: String,
}

pub struct MockGateway {
    inner: Mutex<Vec<RefundReceipt>>,
}

impl MockGateway {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(Vec::new()),
        }
    }

    fn issue(channel: &str, amount: i64, original_receipt_no: &str) -> RefundReceipt {
        let seq = 10_000 + (std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
            % 80_000) as u64;
        let prefix = match channel {
            "INSURANCE" => "INS-REF",
            _ => "PAY-REF",
        };
        RefundReceipt {
            receipt_no: format!("{prefix}-{seq}"),
            channel: channel.to_string(),
            amount,
            original_receipt_no: original_receipt_no.to_string(),
            status: "SUCCESS".to_string(),
        }
    }

    /// 保险冲账（退回保险基金）
    pub fn insurance_chargeback(&self, amount: i64, original_receipt_no: &str) -> RefundReceipt {
        let r = Self::issue("INSURANCE", amount, original_receipt_no);
        tracing::info!("[模拟保险网关] 冲账 {amount} 分, 回执 {}", r.receipt_no);
        self.inner.lock().unwrap().push(r.clone());
        r
    }

    /// 自费渠道退款
    pub fn self_pay_refund(&self, amount: i64, original_receipt_no: &str) -> RefundReceipt {
        let r = Self::issue("SELF_PAY", amount, original_receipt_no);
        tracing::info!("[模拟支付网关] 退款 {amount} 分, 回执 {}", r.receipt_no);
        self.inner.lock().unwrap().push(r.clone());
        r
    }

    pub fn all_receipts(&self) -> Vec<RefundReceipt> {
        self.inner.lock().unwrap().clone()
    }
}
