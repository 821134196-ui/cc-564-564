<script>
  import { get } from '../api.js'
  import { fen, REASON_LABEL, METHOD_LABEL } from '../format.js'

  let { refundId, onClose } = $props()

  let detail = $state(null)
  let error = $state('')

  $effect(() => {
    detail = null; error = ''
    get(`/api/refunds/${refundId}`).then(d => detail = d).catch(e => error = e.message)
  })
</script>

<div class="mask" onclick={onClose} role="presentation">
  <div class="modal" onclick={(e) => e.stopPropagation()} role="dialog">
    {#if error}<div class="error-box">{error}</div>{/if}
    {#if !detail && !error}<p class="muted">加载中…</p>{/if}
    {#if detail}
      <div class="head">
        <h3>退款单 {detail.refund_no}</h3>
        <button class="btn btn-sm btn-outline" onclick={onClose}>关闭</button>
      </div>
      <div class="meta">
        <span>原因：<b>{REASON_LABEL[detail.reason] || detail.reason}</b></span>
        {#if detail.reason_detail}<span>说明：{detail.reason_detail}</span>{/if}
        <span>经办：{detail.operator_name}</span>
        <span>时间：{detail.created_at}</span>
        {#if detail.replayed}<span class="badge badge-orange">幂等重放（未重复扣款）</span>{/if}
      </div>

      <h4>退款项目（{detail.items.length} 项，共 {fen(detail.total_cents)}）</h4>
      <table>
        <thead><tr><th>项目</th><th class="num">原价</th><th class="num">优惠分摊</th><th class="num">退款额</th><th>支付拆分</th><th>计算明细</th></tr></thead>
        <tbody>
          {#each detail.items as it}
            <tr>
              <td>{it.name}
                {#if it.report_void_id}<div><span class="badge badge-purple">关联作废报告 #{it.report_void_id}</span></div>{/if}
              </td>
              <td class="num">{fen(it.calc_detail.list_price_cents)}</td>
              <td class="num">{fen(it.calc_detail.discount_cents)}</td>
              <td class="num"><b>{fen(it.amount_cents)}</b></td>
              <td>
                {#each it.calc_detail.payment_split || [] as p}
                  <div>{METHOD_LABEL[p.method] || p.method} {fen(p.amount_cents)}</div>
                {/each}
              </td>
              <td class="muted detail-cell">{it.calc_detail.discount_note}；{it.calc_detail.split_basis}</td>
            </tr>
          {/each}
        </tbody>
      </table>

      <h4>渠道退回（本地模拟回执）</h4>
      <table>
        <thead><tr><th>渠道</th><th class="num">金额</th><th>回执号</th></tr></thead>
        <tbody>
          {#each detail.components as c}
            <tr><td>{METHOD_LABEL[c.method] || c.method}</td><td class="num">{fen(c.amount_cents)}</td><td class="muted">{c.receipt_no}</td></tr>
          {/each}
        </tbody>
      </table>

      {#if detail.linked_reports?.length}
        <h4>关联的报告作废记录</h4>
        {#each detail.linked_reports as r}
          <div class="void-box">
            报告 #{r.id}（已作废）— 作废原因：{r.void_reason || '-'} ／ 摘要：{r.summary}
          </div>
        {/each}
      {/if}

      {#if detail.skipped?.length}
        <h4>未退项目及原因</h4>
        {#each detail.skipped as s}
          <div class="skip-box">{s.name}（#{s.item_id}）：{s.reason}</div>
        {/each}
      {/if}
    {/if}
  </div>
</div>

<style>
  .mask {
    position: fixed; inset: 0; background: rgba(15, 23, 42, .45);
    display: flex; align-items: flex-start; justify-content: center;
    padding: 40px 16px; z-index: 100; overflow: auto;
  }
  .modal {
    background: #fff; border-radius: 12px; padding: 22px 26px;
    width: 900px; max-width: 96vw; max-height: 88vh; overflow: auto;
  }
  .head { display: flex; justify-content: space-between; align-items: center; }
  h3 { margin: 0; color: #0f4c81; }
  h4 { margin: 18px 0 8px; font-size: 14px; color: #374151; }
  .meta { display: flex; gap: 18px; flex-wrap: wrap; margin: 10px 0 4px; font-size: 13px; color: #4b5563; }
  .detail-cell { max-width: 260px; font-size: 12px; }
  .void-box { background: #f5f3ff; border: 1px solid #ddd6fe; border-radius: 6px; padding: 8px 12px; font-size: 13px; margin-bottom: 6px; }
  .skip-box { background: #fff7ed; border: 1px solid #fed7aa; border-radius: 6px; padding: 8px 12px; font-size: 13px; margin-bottom: 6px; }
</style>
