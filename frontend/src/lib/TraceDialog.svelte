<script>
  import { createEventDispatcher } from 'svelte';
  import { yuan, REFUND_STATUS, CANCEL_REASON } from './format.js';

  export let trace = null; // {refund, reportVoid, report}
  const dispatch = createEventDispatcher();

  $: detail = trace && trace.refund.calcDetail ? JSON.parse(trace.refund.calcDetail) : null;
</script>

{#if trace}
  <div class="modal-mask" on:click={() => dispatch('close')}>
    <div class="modal wide" on:click|stopPropagation>
      <h3>退款计算明细与报告追查</h3>

      <div class="trace-head">
        <span class="st st-refund">{REFUND_STATUS[trace.refund.status]}</span>
        <span>退款单 <code>{trace.refund.id}</code></span>
        <span>原因：{CANCEL_REASON[trace.refund.cancelReason] || trace.refund.cancelReason}</span>
      </div>

      <table class="mini">
        <tr><td>退项数量</td><td>{trace.refund.refundQty}</td></tr>
        <tr><td>原价退款额（优惠前）</td><td>{yuan(trace.refund.grossRefund)}</td></tr>
        <tr><td>扣减优惠分摊</td><td>−{yuan(trace.refund.discountShare)}</td></tr>
        <tr class="strong"><td>实际退款</td><td>{yuan(trace.refund.netRefund)}</td></tr>
        <tr><td>退回保险（冲账）</td><td>{yuan(trace.refund.insuranceAmount)}</td></tr>
        <tr><td>退回自费（原路）</td><td>{yuan(trace.refund.selfPayAmount)}</td></tr>
        <tr><td>保险冲账回执</td><td><code>{trace.refund.insuranceReceiptNo || '—（无保险部分）'}</code></td></tr>
        <tr><td>自费退款回执</td><td><code>{trace.refund.paymentReceiptNo || '—（待处理或无自费部分）'}</code></td></tr>
      </table>

      {#if detail}
        <h4>计算过程</h4>
        <p class="muted">{detail.formula}</p>
        <table class="mini">
          <thead><tr><th>步骤</th><th>金额</th><th>说明</th></tr></thead>
          {#each detail.steps as s}
            <tr>
              <td>{s.step}</td>
              <td class:neg={s.amount < 0}>{s.amount < 0 ? '−' : ''}{yuan(Math.abs(s.amount))}</td>
              <td class="muted">{s.note}</td>
            </tr>
          {/each}
        </table>
        <h4>原始付款构成</h4>
        <table class="mini">
          <tr><td>保险原始付款</td><td>{yuan(detail.paymentComposition.insuranceOriginal)}（{(detail.paymentComposition.insuranceRatioBp / 100).toFixed(2)}%）</td></tr>
          <tr><td>自费原始付款</td><td>{yuan(detail.paymentComposition.selfPayOriginal)}（{(detail.paymentComposition.selfPayRatioBp / 100).toFixed(2)}%）</td></tr>
          <tr><td>本次前已退合计 / 保险 / 自费</td>
            <td>{yuan(detail.paymentComposition.alreadyRefundedTotal)} /
              {yuan(detail.paymentComposition.alreadyInsuranceRefunded)} /
              {yuan(detail.paymentComposition.alreadySelfPayRefunded)}</td></tr>
        </table>
      {/if}

      <h4>报告作废联动</h4>
      {#if trace.reportVoid && trace.report}
        <div class="trace-link ok">
          ✅ 已关联报告作废记录：<code>{trace.reportVoid.id}</code><br />
          报告号 {trace.report.reportNo}，当前状态：{trace.report.status === 'VOIDED' ? '已作废' : trace.report.status}<br />
          作废原因：{trace.reportVoid.reason}
        </div>
      {:else}
        <div class="trace-link">
          本退款不涉及报告作废（项目未出报告）；
          或状态为「待报告作废」，报告员作废后此处自动关联。
        </div>
      {/if}

      <div class="modal-ops">
        <button class="btn primary" on:click={() => dispatch('close')}>关闭</button>
      </div>
    </div>
  </div>
{/if}
