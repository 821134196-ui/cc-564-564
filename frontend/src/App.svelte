<script>
  import { onMount } from 'svelte';
  import { api, setToken, getToken } from './lib/api.js';
  import { ROLE_LABEL } from './lib/format.js';
  import SummaryCards from './lib/SummaryCards.svelte';
  import ItemTable from './lib/ItemTable.svelte';
  import ActionDialog from './lib/ActionDialog.svelte';
  import TraceDialog from './lib/TraceDialog.svelte';

  let users = [];
  let me = null;
  let bills = [];
  let billId = 'bill_001';
  let bill = null;
  let reportsById = {};

  let action = null;   // {type,item}
  let actionError = '';
  let busy = false;
  let trace = null;
  let toast = '';

  function showToast(msg) {
    toast = msg;
    setTimeout(() => (toast = ''), 3500);
  }

  onMount(async () => {
    try {
      users = await api.users();
    } catch {
      users = [];
    }
    if (getToken()) await loadMe();
  });

  async function loadMe() {
    try {
      me = await api.me();
      bills = await api.bills();
      if (bills.length && !bills.find((b) => b.id === billId)) billId = bills[0].id;
      await loadBill();
    } catch (e) {
      me = null;
    }
  }

  async function login(token) {
    setToken(token);
    await loadMe();
  }

  function logout() {
    setToken('');
    me = null;
    bill = null;
  }

  async function loadBill() {
    if (!me) return;
    bill = await api.bill(billId);
    reportsById = {};
    for (const r of bill.reports) reportsById[r.id] = r;
  }

  async function switchBill(e) {
    billId = e.target.value;
    await loadBill();
  }

  function onAction(e) {
    action = e.detail;
    actionError = '';
  }

  async function onSubmit(e) {
    const payload = e.detail;
    const item = action.item;
    busy = true;
    actionError = '';
    try {
      if (payload.type === 'execute') {
        await api.execute(billId, item.id, payload.qty);
        showToast(`已登记 ${item.name} 执行 ${payload.qty} 项`);
      } else if (payload.type === 'cancel') {
        await api.cancel(billId, item.id, payload.reason, payload.remark);
        showToast(`已登记取消：${item.name}`);
      } else if (payload.type === 'issue') {
        await api.issueReport(billId, item.id);
        showToast(`报告已出具：${item.name}`);
      } else if (payload.type === 'refund') {
        const r = await api.createRefund(billId, {
          itemId: item.id,
          qty: payload.qty,
          cancelReason: payload.cancelReason,
          cancelRemark: payload.cancelRemark
        });
        if (r.status === 'AWAITING_VOID') {
          showToast('退项已受理：该项目已出报告，需报告处理人员作废报告后退款到账');
        } else {
          showToast(`退项完成，实退 ¥${(r.netRefund / 100).toFixed(2)}（保险/自费已分别退回）`);
        }
      } else if (payload.type === 'void') {
        await api.voidReport(reportIdFor(item), payload.reason);
        showToast('报告已作废，关联退款已完成并出具回执');
      }
      action = null;
      await loadBill();
    } catch (err) {
      actionError = err.message;
    } finally {
      busy = false;
    }
  }

  function reportIdFor(item) {
    const r = bill.reports.find((x) => x.billItemId === item.id && x.status === 'ISSUED');
    return r ? r.id : null;
  }

  async function onTrace(e) {
    trace = await api.trace(e.detail.id);
  }
</script>

<header class="topbar">
  <div class="brand">🏥 诊所检查退项与费用调整系统</div>
  <div class="role-box">
    {#if me}
      <select value={billId} on:change={switchBill} class="bill-select">
        {#each bills as b}
          <option value={b.id}>{b.billNo}</option>
        {/each}
      </select>
      <span class="who">
        <span class="role-badge">{ROLE_LABEL[me.role]}</span> {me.name}
      </span>
      <button class="btn sm" on:click={logout}>切换岗位</button>
    {:else}
      <span class="muted">请选择岗位登录（本地演示）</span>
    {/if}
  </div>
</header>

<main>
  {#if !me}
    <section class="login">
      <h2>岗位登录</h2>
      <p class="muted">医生、收费人员、报告处理人员按岗位权限操作；患者仅可查看本人账单与退款明细。</p>
      <div class="login-grid">
        {#each users as u}
          <button class="card login-card" on:click={() => login(u.token)}>
            <span class="role-badge big">{ROLE_LABEL[u.role]}</span>
            <strong>{u.name}</strong>
            <span class="muted">{u.role}</span>
          </button>
        {/each}
      </div>
    </section>
  {:else if bill}
    <SummaryCards {bill} />

    <section class="card block">
      <h3>检查项目与退项办理</h3>
      <p class="muted">
        每项展示执行状态、原价、优惠分摊与可退金额；前台依据取消原因办理退项。
        已出报告项目须先作废报告；部分执行仅退未执行部分；同一项目重复退项只成功一次。
      </p>
      <div class="table-wrap">
        <ItemTable items={bill.items} role={me.role} {reportsById}
          on:action={onAction} on:trace={onTrace} />
      </div>
    </section>

    <section class="card block">
      <h3>付款构成与退款回执</h3>
      <table class="pay-table">
        <thead>
          <tr><th>付款方式</th><th>原始付款</th><th>累计已退</th><th>原始回执号</th></tr>
        </thead>
        <tbody>
          <tr>
            <td>保险（基金）</td>
            <td>¥{(bill.paymentBreakdown.insuranceOriginal / 100).toFixed(2)}</td>
            <td>−¥{(bill.insuranceRefunded / 100).toFixed(2)}</td>
            <td><code>{bill.payments.find((p) => p.method === 'INSURANCE')?.receiptNo || '—'}</code></td>
          </tr>
          <tr>
            <td>自费（患者支付）</td>
            <td>¥{(bill.paymentBreakdown.selfPayOriginal / 100).toFixed(2)}</td>
            <td>−¥{(bill.selfPayRefunded / 100).toFixed(2)}</td>
            <td><code>{bill.payments.find((p) => p.method === 'SELF_PAY')?.receiptNo || '—'}</code></td>
          </tr>
        </tbody>
      </table>
    </section>
  {/if}
</main>

{#if toast}<div class="toast">{toast}</div>{/if}

<ActionDialog {action} error={actionError} {busy}
  on:close={() => (action = null)} on:submit={onSubmit} />
{#if trace}<TraceDialog {trace} on:close={() => (trace = null)} />{/if}
