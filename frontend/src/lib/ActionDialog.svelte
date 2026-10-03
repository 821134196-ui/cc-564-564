<script>
  import { createEventDispatcher } from 'svelte';
  import { yuan } from './format.js';

  export let action = null; // {type, item}
  export let error = '';
  export let busy = false;

  const dispatch = createEventDispatcher();

  let qty = 1;
  let reason = 'DOCTOR_CANCEL';
  let remark = '';
  let voidReason = '';

  $: if (action) {
    qty = action.type === 'execute' ? 1 : Math.max(1, action.item.refundableQty || 1);
    reason = 'DOCTOR_CANCEL';
    remark = '';
    voidReason = '';
    error = '';
  }

  $: maxQty = action ? action.item.quantity - action.item.executedQty - action.item.refundedQty : 0;

  function submit() {
    const a = action;
    if (a.type === 'execute') dispatch('submit', { type: 'execute', qty: Number(qty) });
    if (a.type === 'cancel') dispatch('submit', { type: 'cancel', reason, remark });
    if (a.type === 'refund')
      dispatch('submit', {
        type: 'refund',
        qty: Number(qty),
        cancelReason: reason,
        cancelRemark: remark || null
      });
    if (a.type === 'void') dispatch('submit', { type: 'void', reason: voidReason });
    if (a.type === 'issue') dispatch('submit', { type: 'issue' });
  }

  const TITLES = {
    execute: '医生 · 登记检查执行',
    cancel: '医生 · 登记取消（设备故障 / 医生取消）',
    refund: '收费台 · 办理退项',
    void: '报告处理 · 作废报告并完成退款',
    issue: '报告处理 · 出具报告'
  };
</script>

{#if action}
  <div class="modal-mask" on:click={() => dispatch('close')}>
    <div class="modal" on:click|stopPropagation>
      <h3>{TITLES[action.type]}</h3>
      <div class="item-head">
        <strong>{action.item.name}</strong>
        <span class="muted">{action.item.code}</span>
      </div>

      {#if action.type === 'execute'}
        <label>本次执行数量（剩余可执行 {maxQty}，部分执行可分次登记）</label>
        <input type="number" min="1" max={maxQty} bind:value={qty} />
      {/if}

      {#if action.type === 'cancel'}
        <label>取消原因</label>
        <select bind:value={reason}>
          <option value="DOCTOR_CANCEL">医生取消检查</option>
          <option value="EQUIPMENT_FAULT">设备故障导致无法执行</option>
        </select>
        <label>备注（可选）</label>
        <input type="text" bind:value={remark} placeholder="如：CT 球管过热停机" />
      {/if}

      {#if action.type === 'refund'}
        <div class="calc-preview">
          <div>原价单价：<strong>{yuan(action.item.listUnitPrice)}</strong></div>
          <div>成交单价（已分摊优惠）：<strong>{yuan(action.item.unitPrice)}</strong></div>
          <div>可退数量：<strong>{action.item.refundableQty}</strong>
            <span class="muted">（总 {action.item.quantity} − 已执行 {action.item.executedQty} − 已退 {action.item.refundedQty}）</span>
          </div>
          <div>预计可退：<strong class="accent">{yuan(action.item.unitPrice * Math.min(Number(qty) || 0, action.item.refundableQty))}</strong>
            <span class="muted">按账单原保险/自费付款构成分别退回，提交后展示回执</span>
          </div>
        </div>
        <label>退项数量</label>
        <input type="number" min="1" max={action.item.refundableQty} bind:value={qty} />
        <label>取消原因（前台依据原因办理）</label>
        <select bind:value={reason}>
          <option value="DOCTOR_CANCEL">医生取消检查</option>
          <option value="EQUIPMENT_FAULT">设备故障导致无法执行</option>
        </select>
        <label>备注（可选）</label>
        <input type="text" bind:value={remark} />
      {/if}

      {#if action.type === 'void'}
        <p class="warn-text">
          该项目已出报告，退款须先作废报告。作废后退款单自动完成，
          报告作废记录与退款单相互关联、可双向追查。
        </p>
        <label>作废原因</label>
        <textarea rows="3" bind:value={voidReason} placeholder="如：设备故障导致项目取消，报告作废"></textarea>
      {/if}

      {#if action.type === 'issue'}
        <p>确认为该已执行项目出具报告？出具后未作废前不可退项。</p>
      {/if}

      {#if error}<div class="error">⛔ {error}</div>{/if}

      <div class="modal-ops">
        <button class="btn" on:click={() => dispatch('close')} disabled={busy}>取消</button>
        <button class="btn primary" on:click={submit} disabled={busy}>
          {busy ? '处理中…' : '确认'}
        </button>
      </div>
    </div>
  </div>
{/if}
