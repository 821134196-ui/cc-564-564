<script>
  import { get, post } from '../api.js'
  import {
    fen, STATUS_LABEL, STATUS_CLASS, BILL_STATUS_LABEL,
    REASON_LABEL, METHOD_LABEL, FLAG_LABEL
  } from '../format.js'
  import RefundDetailModal from './RefundDetailModal.svelte'

  let { billId, user, onBack } = $props()

  let bill = $state(null)
  let audit = $state([])
  let error = $state('')
  let actionMsg = $state('')
  let patientView = $state(false)
  let showAudit = $state(false)
  let viewRefundId = $state(null)

  // 退款面板
  let selected = $state(new Set())
  let reason = $state('doctor_cancel')
  let reasonDetail = $state('')
  let idemKey = $state(crypto.randomUUID())
  let preview = $state(null)
  let refundResult = $state(null)
  let refundError = $state('')
  let busy = $state(false)

  const isCashier = $derived(user.role === 'cashier' || user.role === 'admin')
  const isDoctor = $derived(user.role === 'doctor' || user.role === 'admin')

  async function load() {
    error = ''
    try {
      ;[bill, audit] = await Promise.all([
        get(`/api/bills/${billId}`),
        get(`/api/bills/${billId}/audit`),
      ])
    } catch (e) { error = e.message }
  }
  $effect(() => { load() })

  function toggleItem(id) {
    const next = new Set(selected)
    next.has(id) ? next.delete(id) : next.add(id)
    selected = next
    preview = null
  }

  function selectAllRefundable() {
    selected = new Set(bill.items.filter(i => i.refundable).map(i => i.id))
    preview = null
  }

  async function doPreview() {
    refundError = ''; preview = null
    try {
      preview = await post(`/api/bills/${billId}/refunds/preview`, {
        item_ids: [...selected], reason, reason_detail: reasonDetail,
      })
    } catch (e) {
      refundError = e.skipped?.length
        ? `${e.message}：${e.skipped.map(s => `${s.name}（${s.reason}）`).join('；')}`
        : e.message
    }
  }

  async function doRefund() {
    refundError = ''; refundResult = null; busy = true
    try {
      refundResult = await post(`/api/bills/${billId}/refunds`, {
        item_ids: [...selected], reason, reason_detail: reasonDetail,
        idempotency_key: idemKey,
      })
      selected = new Set(); preview = null
      idemKey = crypto.randomUUID()
      await load()
    } catch (e) {
      refundError = e.skipped?.length
        ? `${e.message}：${e.skipped.map(s => `${s.name}（${s.reason}）`).join('；')}`
        : e.message
    } finally { busy = false }
  }

  async function itemAction(path, body) {
    actionMsg = ''; error = ''
    try {
      await post(path, body)
      actionMsg = '操作成功'
      await load()
    } catch (e) { error = e.message }
  }

  const currentDue = $derived(bill ? bill.total_payable_cents - bill.total_refunded_cents : 0)
</script>

<button class="btn btn-sm btn-outline" onclick={onBack}>← 返回账单列表</button>

{#if error}<div class="error-box">{error}</div>{/if}
{#if actionMsg}<div class="ok-box">{actionMsg}</div>{/if}
{#if !bill && !error}<p class="muted">加载中…</p>{/if}

{#if bill}
  <!-- 账单头 -->
  <div class="card">
    <div class="bill-head">
      <div>
        <h3 style="margin:0">账单 {bill.bill_no}
          <span class="badge" class:badge-orange={bill.status !== 'paid'} class:badge-blue={bill.status === 'paid'}>
            {BILL_STATUS_LABEL[bill.status]}</span>
        </h3>
        <div class="muted">患者：{bill.patient.name}（{bill.patient.gender} · {bill.patient.age}岁）｜ 创建：{bill.created_at}</div>
      </div>
      <div class="head-actions">
        <button class="btn btn-sm" class:btn-outline={!patientView} onclick={() => patientView = !patientView}>
          {patientView ? '工作人员视图' : '👤 患者视图'}
        </button>
        <button class="btn btn-sm btn-outline" onclick={() => showAudit = !showAudit}>操作日志</button>
      </div>
    </div>

    <!-- 金额总览：调整前后 -->
    <div class="totals">
      <div class="total-box"><div class="t-label">原价合计</div><div class="t-val">{fen(bill.total_list_cents)}</div></div>
      <div class="total-box"><div class="t-label">优惠合计</div><div class="t-val minus">-{fen(bill.total_discount_cents)}</div></div>
      <div class="total-box"><div class="t-label">{patientView ? '调整前应付' : '应付合计'}</div><div class="t-val">{fen(bill.total_payable_cents)}</div></div>
      <div class="total-box"><div class="t-label">实收</div><div class="t-val">{fen(bill.total_paid_cents)}</div></div>
      <div class="total-box refund"><div class="t-label">已退合计</div><div class="t-val">-{fen(bill.total_refunded_cents)}</div></div>
      <div class="total-box highlight"><div class="t-label">{patientView ? '调整后应负担' : '当前应负担'}</div><div class="t-val">{fen(currentDue)}</div></div>
    </div>

    <!-- 支付构成 -->
    <div class="pay-mix">
      <span class="muted">原付款构成：</span>
      {#each bill.payments as p}
        <span class="chip">{METHOD_LABEL[p.method] || p.method} {fen(p.amount_cents)}</span>
      {/each}
      {#if bill.refunded_by_method.length}
        <span class="muted" style="margin-left:16px">已退：</span>
        {#each bill.refunded_by_method as m}
          <span class="chip chip-refund">{METHOD_LABEL[m.method] || m.method} {fen(m.amount_cents)}</span>
        {/each}
      {/if}
    </div>
  </div>

  <!-- 项目明细 -->
  <div class="card">
    <div class="row-between">
      <h3 style="margin:0">检查项目（{bill.items.length}）</h3>
      {#if isCashier && !patientView}
        <button class="btn btn-sm btn-outline" onclick={selectAllRefundable}>全选可退项</button>
      {/if}
    </div>
    <table>
      <thead>
        <tr>
          {#if isCashier && !patientView}<th></th>{/if}
          <th>项目</th><th>套餐</th><th>状态</th>
          <th class="num">原价</th><th class="num">优惠分摊</th><th class="num">应付/可退</th>
          {#if patientView}<th>退项说明</th>{:else}<th>操作</th>{/if}
        </tr>
      </thead>
      <tbody>
        {#each bill.items as it}
          <tr class:row-refunded={it.status === 'refunded'}>
            {#if isCashier && !patientView}
              <td>
                {#if it.refundable}
                  <input type="checkbox" checked={selected.has(it.id)} onchange={() => toggleItem(it.id)} />
                {/if}
              </td>
            {/if}
            <td>
              <b>{it.name}</b> <span class="muted">{it.code}</span>
              {#if it.cancel_flag}
                <div><span class="badge badge-orange">{FLAG_LABEL[it.cancel_flag]}</span>
                  <span class="muted">{it.cancel_note}</span></div>
              {/if}
              {#if it.report}
                <div class="muted">报告 #{it.report.id}（{it.report.status === 'voided' ? '已作废' : '已签发'}）
                  {#if it.report.refund_id}→ 退款单 #{it.report.refund_id}{/if}</div>
              {/if}
            </td>
            <td>{#if it.package_name}<span class="badge badge-purple">{it.package_name}</span>{:else}<span class="muted">—</span>{/if}</td>
            <td><span class="badge {STATUS_CLASS[it.status]}">{STATUS_LABEL[it.status]}</span></td>
            <td class="num">{fen(it.list_price_cents)}</td>
            <td class="num">{it.discount_cents ? `-${fen(it.discount_cents)}` : '—'}</td>
            <td class="num">
              {#if it.status === 'refunded'}
                <span class="muted">已退 {fen(it.refunded_cents)}</span>
              {:else}
                <b>{fen(it.payable_cents)}</b>
              {/if}
            </td>
            {#if patientView}
              <td class="muted">
                {#if it.status === 'refunded'}✅ 已退项，退回 {fen(it.refunded_cents)}
                {:else if it.non_refundable_reason}⛔ {it.non_refundable_reason}
                {:else}—{/if}
              </td>
            {:else}
              <td>
                {#if it.non_refundable_reason && it.status !== 'refunded'}
                  <div class="muted reason">{it.non_refundable_reason}</div>
                {/if}
                {#if isDoctor && it.status === 'pending'}
                  <button class="btn btn-sm btn-outline" onclick={() => itemAction(`/api/items/${it.id}/execute`)}>标记执行</button>
                  <button class="btn btn-sm btn-outline" onclick={() => itemAction(`/api/items/${it.id}/flag`, { flag: 'doctor_cancel', note: '医生取消该项检查' })}>取消医嘱</button>
                  <button class="btn btn-sm btn-outline" onclick={() => itemAction(`/api/items/${it.id}/flag`, { flag: 'equipment_failure', note: '设备故障无法执行' })}>设备故障</button>
                {/if}
              </td>
            {/if}
          </tr>
        {/each}
      </tbody>
    </table>
  </div>

  <!-- 退款办理（收费员） -->
  {#if isCashier && !patientView}
    <div class="card refund-panel">
      <h3>办理退项（收费台）</h3>
      {#if selected.size === 0}
        <p class="muted">勾选上表中的可退项目后办理。已执行/已出报告的项目不可退；已作废报告的项目请选择「报告作废后退款」。</p>
      {:else}
        <div class="form-line">
          <label>退款原因
            <select bind:value={reason} onchange={() => preview = null}>
              {#each Object.entries(REASON_LABEL) as [k, v]}<option value={k}>{v}</option>{/each}
            </select>
          </label>
          <label style="flex:1">补充说明
            <input bind:value={reasonDetail} placeholder="选填，如：CT 机故障，改约下次" />
          </label>
        </div>
        <div class="form-line">
          <button class="btn btn-outline" onclick={doPreview}>试算预览</button>
          <button class="btn" onclick={doRefund} disabled={busy}>{busy ? '提交中…' : '确认退款'}</button>
          <span class="muted">已选 {selected.size} 项（幂等键 {idemKey.slice(0, 8)}…，重复提交不会重复退款）</span>
        </div>
      {/if}
      {#if refundError}<div class="error-box">{refundError}</div>{/if}

      {#if preview}
        <div class="preview-box">
          <b>试算结果：</b>可退 {preview.items.length} 项，合计 <b>{fen(preview.total_cents)}</b>
          <div class="chips">
            {#each preview.components as c}
              <span class="chip chip-refund">{METHOD_LABEL[c.method] || c.method} 退 {fen(c.amount_cents)}</span>
            {/each}
          </div>
          {#each preview.items as p}
            <div class="muted">· {p.name}：{fen(p.amount_cents)}（{p.calc_detail.discount_note}）</div>
          {/each}
          {#each preview.skipped as s}
            <div class="skip-line">· {s.name} 跳过：{s.reason}</div>
          {/each}
        </div>
      {/if}

      {#if refundResult}
        <div class="ok-box">
          退款成功：{refundResult.refund_no}，共 {fen(refundResult.total_cents)}。
          {#each refundResult.components as c}
            <div>{METHOD_LABEL[c.method] || c.method} 退回 {fen(c.amount_cents)}（回执 {c.receipt_no}）</div>
          {/each}
          {#if refundResult.skipped?.length}
            <div>跳过：{refundResult.skipped.map(s => `${s.name}（${s.reason}）`).join('；')}</div>
          {/if}
          <button class="btn btn-sm btn-outline" style="margin-top:6px" onclick={() => viewRefundId = refundResult.id}>查看退款单详情</button>
        </div>
      {/if}
    </div>
  {/if}

  <!-- 患者视图说明 -->
  {#if patientView}
    <div class="card patient-note">
      <h3>费用调整说明（给患者）</h3>
      {#if bill.total_refunded_cents === 0}
        <p class="muted">本账单暂无退项记录。</p>
      {:else}
        <p>您本次账单原应付 <b>{fen(bill.total_payable_cents)}</b>，已办理退项 <b class="red">{fen(bill.total_refunded_cents)}</b>，
          调整后应负担 <b>{fen(currentDue)}</b>。退款将按原付款渠道退回：</p>
        <div class="chips">
          {#each bill.refunded_by_method as m}
            <span class="chip chip-refund">{METHOD_LABEL[m.method] || m.method} 退 {fen(m.amount_cents)}</span>
          {/each}
        </div>
      {/if}
      <p class="muted">上表「退项说明」列标出了每个项目的退项结果或不能退款的原因。</p>
    </div>
  {/if}

  <!-- 退款记录 -->
  <div class="card">
    <h3>本账单退款记录（{bill.refunds.length}）</h3>
    {#if bill.refunds.length === 0}<p class="muted">暂无退款记录</p>{/if}
    {#each bill.refunds as r}
      <div class="refund-line">
        <span class="badge badge-orange">{r.refund_no}</span>
        <span>{REASON_LABEL[r.reason] || r.reason}</span>
        <span class="red">-{fen(r.total_cents)}</span>
        <span class="muted">{r.operator_name} · {r.created_at}</span>
        <button class="btn btn-sm btn-outline" onclick={() => viewRefundId = r.id}>明细</button>
      </div>
    {/each}
  </div>

  <!-- 操作日志 -->
  {#if showAudit}
    <div class="card">
      <h3>操作日志</h3>
      {#if audit.length === 0}<p class="muted">暂无日志</p>{/if}
      {#each audit as a}
        <div class="audit-line">
          <span class="muted">{a.created_at}</span>
          <span class="badge badge-gray">{a.actor_name} · {a.actor_role}</span>
          <span>{a.detail}</span>
        </div>
      {/each}
    </div>
  {/if}
{/if}

{#if viewRefundId}
  <RefundDetailModal refundId={viewRefundId} onClose={() => viewRefundId = null} />
{/if}

<style>
  .bill-head { display: flex; justify-content: space-between; align-items: flex-start; gap: 12px; flex-wrap: wrap; }
  .head-actions { display: flex; gap: 8px; }
  .totals { display: flex; gap: 12px; margin: 16px 0 10px; flex-wrap: wrap; }
  .total-box {
    background: #f8fafc; border: 1px solid #e5e7eb; border-radius: 8px;
    padding: 10px 16px; min-width: 130px;
  }
  .total-box.refund { background: #fef2f2; border-color: #fecaca; }
  .total-box.highlight { background: #eef4fa; border-color: #93c5fd; }
  .t-label { font-size: 12px; color: #6b7280; }
  .t-val { font-size: 18px; font-weight: 700; font-variant-numeric: tabular-nums; }
  .t-val.minus { color: #15803d; }
  .refund .t-val { color: #b91c1c; }
  .pay-mix { display: flex; align-items: center; gap: 6px; flex-wrap: wrap; }
  .chip {
    background: #eef2f7; border-radius: 12px; padding: 3px 12px;
    font-size: 13px; font-variant-numeric: tabular-nums;
  }
  .chip-refund { background: #fee2e2; color: #b91c1c; }
  .row-between { display: flex; justify-content: space-between; align-items: center; margin-bottom: 10px; }
  .row-refunded td { background: #fafafa; color: #9ca3af; }
  .reason { font-size: 12px; max-width: 220px; }
  .refund-panel { border-left: 4px solid #0f4c81; }
  .form-line { display: flex; gap: 12px; align-items: flex-end; margin: 10px 0; flex-wrap: wrap; }
  .form-line label { display: flex; flex-direction: column; gap: 4px; font-size: 13px; }
  select, input { padding: 7px 10px; border: 1px solid #d1d5db; border-radius: 6px; }
  .preview-box { background: #f8fbff; border: 1px dashed #93c5fd; border-radius: 8px; padding: 12px 16px; margin-top: 10px; }
  .chips { display: flex; gap: 6px; margin: 8px 0; flex-wrap: wrap; }
  .skip-line { color: #c2410c; font-size: 13px; }
  .patient-note { border-left: 4px solid #ffb020; }
  .red { color: #b91c1c; }
  .refund-line {
    display: flex; gap: 14px; align-items: center; padding: 8px 0;
    border-bottom: 1px solid #f0f2f5; font-size: 14px;
  }
  .audit-line { display: flex; gap: 10px; padding: 6px 0; border-bottom: 1px solid #f5f5f5; font-size: 13px; align-items: center; }
</style>
