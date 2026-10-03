<script>
  import { get, post } from '../api.js'
  import { fen, BILL_STATUS_LABEL } from '../format.js'
  import BillDetail from './BillDetail.svelte'

  let { user } = $props()

  let bills = $state([])
  let patients = $state([])
  let packages = $state([])
  let selectedId = $state(null)
  let error = $state('')
  let showNewBill = $state(false)

  // 开单表单
  let nbPatient = $state(1)
  let nbItems = $state([])
  let nbPackages = $state([])
  let nbPayments = $state([{ method: 'cash', amount: '' }])
  let nbError = $state('')
  let nbOk = $state('')

  const canCreate = $derived(user.role === 'cashier' || user.role === 'admin')

  async function load() {
    error = ''
    try {
      ;[bills, patients, packages] = await Promise.all([
        get('/api/bills'), get('/api/patients'), get('/api/packages')
      ])
    } catch (e) { error = e.message }
  }
  load()

  const nbTotal = $derived(
    nbItems.reduce((s, i) => s + (Number(i.list) || 0) - (Number(i.discount) || 0), 0) +
    nbPackages.reduce((s, p) => s + (packages.find(x => x.id === Number(p.id))?.price_cents || 0), 0)
  )
  const nbPaid = $derived(nbPayments.reduce((s, p) => s + Math.round(Number(p.amount || 0) * 100), 0))

  function addItem() { nbItems = [...nbItems, { code: '', name: '', list: '', discount: 0 }] }
  function removeItem(i) { nbItems = nbItems.filter((_, x) => x !== i) }
  function togglePackage(id) {
    nbPackages = nbPackages.some(p => p.id === id)
      ? nbPackages.filter(p => p.id !== id)
      : [...nbPackages, { id }]
  }
  function addPayment() { nbPayments = [...nbPayments, { method: 'insurance', amount: '' }] }

  async function submitBill() {
    nbError = ''; nbOk = ''
    try {
      const body = {
        patient_id: Number(nbPatient),
        items: nbItems.map(i => ({
          code: i.code || 'ITEM', name: i.name,
          list_price_cents: Math.round(Number(i.list) * 100),
          discount_cents: Math.round(Number(i.discount || 0) * 100),
        })),
        packages: nbPackages.map(p => ({ package_id: Number(p.id) })),
        payments: nbPayments.map(p => ({ method: p.method, amount_cents: Math.round(Number(p.amount) * 100) })),
      }
      const bill = await post('/api/bills', body)
      nbOk = `开单成功：${bill.bill_no}`
      nbItems = []; nbPackages = []; nbPayments = [{ method: 'cash', amount: '' }]
      showNewBill = false
      await load()
      selectedId = bill.id
    } catch (e) { nbError = e.message }
  }
</script>

{#if selectedId}
  <BillDetail billId={selectedId} {user} onBack={() => { selectedId = null; load() }} />
{:else}
  <div class="card">
    <div class="row-between">
      <h3 style="margin:0">账单列表</h3>
      {#if canCreate}
        <button class="btn btn-sm" onclick={() => showNewBill = !showNewBill}>
          {showNewBill ? '收起' : '＋ 开新账单'}
        </button>
      {/if}
    </div>
    {#if error}<div class="error-box">{error}</div>{/if}
    {#if nbOk}<div class="ok-box">{nbOk}</div>{/if}

    {#if showNewBill}
      <div class="new-bill">
        <div class="form-row">
          <label>患者
            <select bind:value={nbPatient}>
              {#each patients as p}<option value={p.id}>{p.name}（{p.gender} · {p.age}岁）</option>{/each}
            </select>
          </label>
          <div>
            <div class="muted" style="margin-bottom:4px">套餐模板（点击加入）</div>
            {#each packages as pkg}
              <button class="btn btn-sm pkg" class:btn-outline={!nbPackages.some(p => p.id === pkg.id)}
                onclick={() => togglePackage(pkg.id)}>
                {pkg.name} {fen(pkg.price_cents)}
              </button>
            {/each}
          </div>
        </div>

        <div class="muted" style="margin:10px 0 4px">单项检查</div>
        {#each nbItems as item, i}
          <div class="item-row">
            <input placeholder="编码" bind:value={item.code} style="width:80px" />
            <input placeholder="项目名称" bind:value={item.name} style="flex:1" />
            <input placeholder="原价(元)" type="number" bind:value={item.list} style="width:100px" />
            <input placeholder="优惠(元)" type="number" bind:value={item.discount} style="width:90px" />
            <button class="btn btn-sm btn-danger" onclick={() => removeItem(i)}>删</button>
          </div>
        {/each}
        <button class="btn btn-sm btn-outline" onclick={addItem}>＋ 加单项</button>

        <div class="muted" style="margin:12px 0 4px">支付构成（合计须等于应付 {fen(nbTotal)}）</div>
        {#each nbPayments as p}
          <div class="item-row">
            <select bind:value={p.method}>
              <option value="insurance">医保</option><option value="cash">现金</option>
              <option value="wechat">微信</option><option value="alipay">支付宝</option>
            </select>
            <input placeholder="金额(元)" type="number" bind:value={p.amount} style="width:120px" />
          </div>
        {/each}
        <button class="btn btn-sm btn-outline" onclick={addPayment}>＋ 加支付方式</button>

        <div class="sum-line">
          应付合计 <b>{fen(nbTotal)}</b> ／ 已填支付 <b class:bad={nbPaid !== nbTotal}>{fen(nbPaid)}</b>
        </div>
        {#if nbError}<div class="error-box">{nbError}</div>{/if}
        <button class="btn" onclick={submitBill} disabled={nbPaid !== nbTotal || nbTotal <= 0}>提交开单</button>
      </div>
    {/if}

    <table>
      <thead>
        <tr><th>账单号</th><th>患者</th><th>项目数</th><th class="num">应付</th><th class="num">实收</th><th class="num">已退</th><th>状态</th><th>创建时间</th><th></th></tr>
      </thead>
      <tbody>
        {#each bills as b}
          <tr>
            <td>{b.bill_no}</td>
            <td>{b.patient_name}</td>
            <td>{b.item_count}</td>
            <td class="num">{fen(b.total_payable_cents)}</td>
            <td class="num">{fen(b.total_paid_cents)}</td>
            <td class="num" class:refunded={b.total_refunded_cents > 0}>{fen(b.total_refunded_cents)}</td>
            <td><span class="badge" class:badge-orange={b.status !== 'paid'} class:badge-blue={b.status === 'paid'}>
              {BILL_STATUS_LABEL[b.status] || b.status}</span></td>
            <td class="muted">{b.created_at}</td>
            <td><button class="btn btn-sm btn-outline" onclick={() => selectedId = b.id}>详情</button></td>
          </tr>
        {/each}
      </tbody>
    </table>
  </div>
{/if}

<style>
  .row-between { display: flex; justify-content: space-between; align-items: center; margin-bottom: 12px; }
  .refunded { color: #b91c1c; }
  .new-bill { border: 1px dashed #93c5fd; border-radius: 8px; padding: 14px; margin-bottom: 16px; background: #f8fbff; }
  .form-row { display: flex; gap: 24px; align-items: flex-start; }
  .form-row label { display: flex; flex-direction: column; gap: 4px; font-size: 13px; }
  select, input { padding: 6px 8px; border: 1px solid #d1d5db; border-radius: 6px; }
  .pkg { margin-right: 6px; }
  .item-row { display: flex; gap: 8px; margin-bottom: 6px; align-items: center; }
  .sum-line { margin: 12px 0; font-size: 14px; }
  .bad { color: #b91c1c; }
</style>
