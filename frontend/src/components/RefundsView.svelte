<script>
  import { get } from '../api.js'
  import { fen, REASON_LABEL } from '../format.js'
  import RefundDetailModal from './RefundDetailModal.svelte'

  let refunds = $state([])
  let error = $state('')
  let viewRefundId = $state(null)

  $effect(() => {
    get('/api/refunds').then(r => refunds = r).catch(e => error = e.message)
  })
</script>

<div class="card">
  <h3>退款记录（{refunds.length}）</h3>
  {#if error}<div class="error-box">{error}</div>{/if}
  {#if refunds.length === 0}<p class="muted">暂无退款记录</p>{/if}
  {#if refunds.length}
    <table>
      <thead>
        <tr><th>退款单号</th><th>账单</th><th>患者</th><th>原因</th><th class="num">金额</th><th>经办人</th><th>时间</th><th></th></tr>
      </thead>
      <tbody>
        {#each refunds as r}
          <tr>
            <td><span class="badge badge-orange">{r.refund_no}</span></td>
            <td>{r.bill_no}</td>
            <td>{r.patient_name}</td>
            <td>{REASON_LABEL[r.reason] || r.reason}{#if r.reason_detail}<div class="muted">{r.reason_detail}</div>{/if}</td>
            <td class="num red">-{fen(r.total_cents)}</td>
            <td>{r.operator_name}</td>
            <td class="muted">{r.created_at}</td>
            <td><button class="btn btn-sm btn-outline" onclick={() => viewRefundId = r.id}>明细</button></td>
          </tr>
        {/each}
      </tbody>
    </table>
  {/if}
</div>

{#if viewRefundId}
  <RefundDetailModal refundId={viewRefundId} onClose={() => viewRefundId = null} />
{/if}

<style>
  .red { color: #b91c1c; }
</style>
