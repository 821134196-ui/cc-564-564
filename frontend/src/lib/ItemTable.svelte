<script>
  import { createEventDispatcher } from 'svelte';
  import {
    yuan, ITEM_STATUS, REPORT_STATUS, REFUND_STATUS, CANCEL_REASON
  } from './format.js';

  export let items = [];
  export let role = null;
  export let reportsById = {};

  const dispatch = createEventDispatcher();

  function statusClass(s) {
    return 'st st-' + s.toLowerCase().replaceAll('_', '-');
  }
  function reportClass(s) {
    return 'rp rp-' + s.toLowerCase();
  }
  $: pendingRefund = (it) => it.refunds.find((r) => r.status === 'AWAITING_VOID');
</script>

<table class="items">
  <thead>
    <tr>
      <th>检查项目</th>
      <th class="num">执行状态</th>
      <th class="num">数量 / 已执行 / 已退</th>
      <th class="num">原价</th>
      <th class="num">优惠分摊</th>
      <th class="num">成交金额</th>
      <th class="num">可退数量</th>
      <th class="num">可退金额</th>
      <th>报告</th>
      <th>操作</th>
    </tr>
  </thead>
  <tbody>
    {#each items as it (it.id)}
      <!-- 顶层行（单项 或 套餐父行） -->
      <tr class:pkg={it.category === 'PACKAGE'}>
        <td>
          <strong>{it.name}</strong>
          <span class="code">{it.code}</span>
          {#if it.category === 'PACKAGE'}<span class="tag">套餐</span>{/if}
          {#if it.cancelReason}
            <div class="reason-line">取消：{CANCEL_REASON[it.cancelReason] || it.cancelReason}
              {it.cancelRemark ? `（${it.cancelRemark}）` : ''}</div>
          {/if}
        </td>
        <td class="num"><span class={statusClass(it.status)}>{ITEM_STATUS[it.status] || it.status}</span></td>
        <td class="num">{it.quantity} / {it.executedQty} / {it.refundedQty}</td>
        <td class="num">{yuan(it.originalTotal)}</td>
        <td class="num discount">−{yuan(it.discountTotalAmount)}</td>
        <td class="num">{yuan(it.paidTotal)}</td>
        <td class="num">{it.category === 'PACKAGE' ? '按子项' : it.refundableQty}</td>
        <td class="num">{it.category === 'PACKAGE' ? yuan(it.refundableAmount) : yuan(it.refundableAmount)}</td>
        <td>
          <span class={reportClass(it.reportStatus)}>{REPORT_STATUS[it.reportStatus]}</span>
        </td>
        <td class="ops">
          {#if it.category !== 'PACKAGE'}
            {#if role === 'DOCTOR'}
              <button class="btn sm" on:click={() => dispatch('action', { type: 'execute', item: it })}
                disabled={it.quantity - it.executedQty - it.refundedQty <= 0 || it.reportStatus === 'ISSUED'}>
                登记执行
              </button>
              <button class="btn sm" on:click={() => dispatch('action', { type: 'cancel', item: it })}
                disabled={it.reportStatus === 'ISSUED' || it.executedQty === it.quantity || it.refundedQty > 0}>
                登记取消
              </button>
            {:else if role === 'CASHIER'}
              <button class="btn sm primary" on:click={() => dispatch('action', { type: 'refund', item: it })}
                disabled={!!it.nonRefundableReason && !pendingRefund(it)}>
                办理退项
              </button>
            {:else if role === 'REPORT_STAFF'}
              {#if it.reportStatus === 'NONE' && it.executedQty > 0}
                <button class="btn sm" on:click={() => dispatch('action', { type: 'issue', item: it })}>出具报告</button>
              {/if}
              {#if pendingRefund(it)}
                <button class="btn sm warn" on:click={() => dispatch('action', { type: 'void', item: it })}>
                  作废报告并退款
                </button>
              {/if}
            {/if}
          {/if}
        </td>
      </tr>

      <!-- 不可退原因提示 -->
      {#if it.nonRefundableReason && it.category !== 'PACKAGE'}
        <tr class="reason-row">
          <td colspan="10">
            <span class="lock">⛔ {it.nonRefundableReason}</span>
            {#if pendingRefund(it) && role === 'REPORT_STAFF'}
              <button class="btn sm warn" on:click={() => dispatch('action', { type: 'void', item: it })}>
                前往作废报告
              </button>
            {/if}
          </td>
        </tr>
      {/if}
      {#if it.category === 'PACKAGE'}
        <tr class="reason-row pkg-hint"><td colspan="10">套餐按具体子项退项；已分摊的套餐优惠随对应子项扣减。</td></tr>
      {/if}

      <!-- 套餐子项 -->
      {#each it.children as ch (ch.id)}
        <tr class="child">
          <td>
            <span class="indent">└ {ch.name}</span>
            <span class="code">{ch.code}</span>
            <span class="tag alt">套餐子项</span>
            {#if ch.cancelReason}
              <div class="reason-line">取消：{CANCEL_REASON[ch.cancelReason] || ch.cancelReason}</div>
            {/if}
          </td>
          <td class="num"><span class={statusClass(ch.status)}>{ITEM_STATUS[ch.status] || ch.status}</span></td>
          <td class="num">{ch.quantity} / {ch.executedQty} / {ch.refundedQty}</td>
          <td class="num">{yuan(ch.originalTotal)}</td>
          <td class="num discount">−{yuan(ch.discountTotalAmount)}</td>
          <td class="num">{yuan(ch.paidTotal)}</td>
          <td class="num">{ch.refundableQty}</td>
          <td class="num">{yuan(ch.refundableAmount)}</td>
          <td><span class={reportClass(ch.reportStatus)}>{REPORT_STATUS[ch.reportStatus]}</span></td>
          <td class="ops">
            {#if role === 'DOCTOR'}
              <button class="btn sm" on:click={() => dispatch('action', { type: 'execute', item: ch })}
                disabled={ch.quantity - ch.executedQty - ch.refundedQty <= 0 || ch.reportStatus === 'ISSUED'}>登记执行</button>
              <button class="btn sm" on:click={() => dispatch('action', { type: 'cancel', item: ch })}
                disabled={ch.reportStatus === 'ISSUED' || ch.executedQty === ch.quantity || ch.refundedQty > 0}>登记取消</button>
            {:else if role === 'CASHIER'}
              <button class="btn sm primary" on:click={() => dispatch('action', { type: 'refund', item: ch })}
                disabled={!!ch.nonRefundableReason && !pendingRefund(ch)}>办理退项</button>
            {:else if role === 'REPORT_STAFF'}
              {#if ch.reportStatus === 'NONE' && ch.executedQty > 0}
                <button class="btn sm" on:click={() => dispatch('action', { type: 'issue', item: ch })}>出具报告</button>
              {/if}
              {#if pendingRefund(ch)}
                <button class="btn sm warn" on:click={() => dispatch('action', { type: 'void', item: ch })}>作废报告并退款</button>
              {/if}
            {/if}
          </td>
        </tr>
        {#if ch.nonRefundableReason}
          <tr class="reason-row child-reason"><td colspan="10"><span class="lock">⛔ {ch.nonRefundableReason}</span></td></tr>
        {/if}
      {/each}

      <!-- 退款记录 -->
      {#each it.refunds as rf (rf.id)}
        <tr class="refund-row">
          <td colspan="10">
            <div class="rf-line">
              <span class="st st-refund">{REFUND_STATUS[rf.status]}</span>
              <span>退 {rf.refundQty} 项 · 原价 {yuan(rf.grossRefund)} · 分摊优惠 −{yuan(rf.discountShare)} ·
                <strong>实退 {yuan(rf.netRefund)}</strong>
                （保险 {yuan(rf.insuranceAmount)} / 自费 {yuan(rf.selfPayAmount)}）
              </span>
              <span class="muted">原因：{CANCEL_REASON[rf.cancelReason] || rf.cancelReason}</span>
              <button class="btn xs" on:click={() => dispatch('trace', rf)}>计算明细 / 追查</button>
            </div>
          </td>
        </tr>
      {/each}
      {#each it.children as ch (ch.id + '-rf')}
        {#each ch.refunds as rf (rf.id)}
          <tr class="refund-row child-rf">
            <td colspan="10">
              <div class="rf-line indent-line">
                <span class="st st-refund">{REFUND_STATUS[rf.status]}</span>
                <span>【{ch.name}】退 {rf.refundQty} 项 · 原价 {yuan(rf.grossRefund)} · 优惠 −{yuan(rf.discountShare)} ·
                  <strong>实退 {yuan(rf.netRefund)}</strong>
                  （保险 {yuan(rf.insuranceAmount)} / 自费 {yuan(rf.selfPayAmount)}）</span>
                <button class="btn xs" on:click={() => dispatch('trace', rf)}>计算明细 / 追查</button>
              </div>
            </td>
          </tr>
        {/each}
      {/each}
    {/each}
  </tbody>
</table>
