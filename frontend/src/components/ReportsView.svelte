<script>
  import { get, post } from '../api.js'
  import { fen } from '../format.js'
  import RefundDetailModal from './RefundDetailModal.svelte'

  let { user } = $props()

  let pendingItems = $state([])   // 已执行待签发
  let reports = $state([])
  let error = $state('')
  let msg = $state('')
  let viewRefundId = $state(null)

  const isReporter = $derived(user.role === 'reporter' || user.role === 'admin')

  async function load() {
    error = ''
    try {
      ;[pendingItems, reports] = await Promise.all([
        get('/api/items?status=executed'),
        get('/api/reports'),
      ])
    } catch (e) { error = e.message }
  }
  $effect(() => { load() })

  async function issue(itemId) {
    const summary = prompt('请输入报告摘要：', '检查未见明显异常。')
    if (summary === null) return
    msg = ''; error = ''
    try {
      await post(`/api/items/${itemId}/report`, { summary })
      msg = '报告已签发'
      await load()
    } catch (e) { error = e.message }
  }

  async function voidReport(reportId) {
    const reason = prompt('请输入作废原因（必填）：', '报告内容有误，需重新出具')
    if (!reason) return
    msg = ''; error = ''
    try {
      await post(`/api/reports/${reportId}/void`, { reason })
      msg = '报告已作废；对应项目回到「已执行」，可由收费台按「报告作废」办理退款'
      await load()
    } catch (e) { error = e.message }
  }
</script>

{#if error}<div class="error-box">{error}</div>{/if}
{#if msg}<div class="ok-box">{msg}</div>{/if}

<div class="card">
  <h3>待签发报告（已执行项目 {pendingItems.length} 项）</h3>
  {#if pendingItems.length === 0}<p class="muted">暂无待签发项目</p>{/if}
  {#if pendingItems.length}
    <table>
      <thead><tr><th>项目</th><th>账单</th><th>患者</th><th class="num">应付</th><th></th></tr></thead>
      <tbody>
        {#each pendingItems as it}
          <tr>
            <td><b>{it.name}</b> <span class="muted">{it.code}</span></td>
            <td>{it.bill_no}</td>
            <td>{it.patient_name}</td>
            <td class="num">{fen(it.payable_cents)}</td>
            <td>
              {#if isReporter}
                <button class="btn btn-sm" onclick={() => issue(it.id)}>签发报告</button>
              {:else}
                <span class="muted">仅报告处理员可签发</span>
              {/if}
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
  {/if}
</div>

<div class="card">
  <h3>报告记录（{reports.length}）</h3>
  {#if reports.length === 0}<p class="muted">暂无报告</p>{/if}
  {#if reports.length}
    <table>
      <thead><tr><th>#</th><th>项目</th><th>患者 / 账单</th><th>摘要</th><th>状态</th><th>签发</th><th>作废</th><th>关联退款</th><th></th></tr></thead>
      <tbody>
        {#each reports as r}
          <tr>
            <td>{r.id}</td>
            <td>{r.item_name}</td>
            <td>{r.patient_name}<div class="muted">{r.bill_no}</div></td>
            <td class="muted summary">{r.summary}</td>
            <td>
              {#if r.status === 'issued'}<span class="badge badge-green">已签发</span>
              {:else}<span class="badge badge-gray">已作废</span>{/if}
            </td>
            <td class="muted">{r.issued_by_name}<div>{r.issued_at}</div></td>
            <td class="muted">
              {#if r.voided_by_name}{r.voided_by_name}<div>{r.voided_at}</div><div>{r.void_reason}</div>{:else}—{/if}
            </td>
            <td>
              {#if r.refund_id}
                <button class="btn btn-sm btn-outline" onclick={() => viewRefundId = r.refund_id}>
                  {r.refund_no || `退款单 #${r.refund_id}`}
                </button>
              {:else}—{/if}
            </td>
            <td>
              {#if isReporter && r.status === 'issued'}
                <button class="btn btn-sm btn-danger" onclick={() => voidReport(r.id)}>作废</button>
              {/if}
            </td>
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
  .summary { max-width: 220px; font-size: 13px; }
</style>
