<script>
  import { yuan, pctFromBp } from '../lib/format.js';

  export let bill;
</script>

<div class="summary-grid">
  <div class="card stat">
    <div class="stat-label">患者</div>
    <div class="stat-value">{bill.patient.name}</div>
    <div class="stat-sub">{bill.patient.identifier} · 账单 {bill.billNo}</div>
  </div>
  <div class="card stat">
    <div class="stat-label">调整前应付（原价合计）</div>
    <div class="stat-value">{yuan(bill.originalTotal)}</div>
    <div class="stat-sub down">优惠分摊 −{yuan(bill.discountTotal)}</div>
  </div>
  <div class="card stat">
    <div class="stat-label">调整前实付（付款构成）</div>
    <div class="stat-value">{yuan(bill.paidTotal)}</div>
    <div class="stat-sub">
      保险 {yuan(bill.paymentBreakdown.insuranceOriginal)}
      （{pctFromBp(bill.paymentBreakdown.insuranceRatioBp)}） ·
      自费 {yuan(bill.paymentBreakdown.selfPayOriginal)}
    </div>
  </div>
  <div class="card stat highlight">
    <div class="stat-label">累计已退</div>
    <div class="stat-value">−{yuan(bill.refundedTotal)}</div>
    <div class="stat-sub">
      保险冲账 −{yuan(bill.insuranceRefunded)} · 自费退回 −{yuan(bill.selfPayRefunded)}
    </div>
  </div>
  <div class="card stat final">
    <div class="stat-label">调整后应付</div>
    <div class="stat-value big">{yuan(bill.adjustedTotal)}</div>
    <div class="stat-sub">= 实付 {yuan(bill.paidTotal)} − 退款 {yuan(bill.refundedTotal)}</div>
  </div>
</div>
