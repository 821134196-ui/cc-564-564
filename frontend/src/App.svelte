<script>
  import { getUser, clearSession } from './api.js'
  import { ROLE_LABEL } from './format.js'
  import Login from './components/Login.svelte'
  import BillsView from './components/BillsView.svelte'
  import RefundsView from './components/RefundsView.svelte'
  import ReportsView from './components/ReportsView.svelte'

  let user = $state(getUser())
  let tab = $state('bills')

  function onLogin(u) { user = u; tab = 'bills' }
  function logout() { clearSession(); user = null }

  const tabs = [
    { key: 'bills', label: '账单与退项' },
    { key: 'refunds', label: '退款记录' },
    { key: 'reports', label: '报告管理' },
  ]
</script>

{#if !user}
  <Login onLogin={onLogin} />
{:else}
  <div class="layout">
    <header>
      <div class="brand">🏥 诊所退项与费用调整系统</div>
      <nav>
        {#each tabs as t}
          <button class:active={tab === t.key} onclick={() => tab = t.key}>{t.label}</button>
        {/each}
      </nav>
      <div class="who">
        <span class="role-tag">{ROLE_LABEL[user.role] || user.role}</span>
        <span>{user.display_name}</span>
        <button class="link" onclick={logout}>退出</button>
      </div>
    </header>
    <main>
      {#if tab === 'bills'}
        <BillsView {user} />
      {:else if tab === 'refunds'}
        <RefundsView />
      {:else if tab === 'reports'}
        <ReportsView {user} />
      {/if}
    </main>
  </div>
{/if}

<style>
  :global(*) { box-sizing: border-box; }
  :global(body) {
    margin: 0;
    font-family: 'PingFang SC', 'Microsoft YaHei', 'Noto Sans CJK SC', system-ui, sans-serif;
    background: #f0f4f8;
    color: #1f2937;
  }
  :global(button) { font-family: inherit; cursor: pointer; }
  :global(input, select, textarea) { font-family: inherit; font-size: 14px; }

  .layout { min-height: 100vh; }
  header {
    display: flex; align-items: center; gap: 24px;
    background: #0f4c81; color: #fff; padding: 0 24px; height: 56px;
    position: sticky; top: 0; z-index: 10;
  }
  .brand { font-size: 17px; font-weight: 600; white-space: nowrap; }
  nav { display: flex; gap: 4px; flex: 1; }
  nav button {
    background: transparent; border: none; color: #cfe0f0;
    padding: 8px 16px; border-radius: 6px; font-size: 14px;
  }
  nav button.active, nav button:hover { background: rgba(255,255,255,.15); color: #fff; }
  .who { display: flex; align-items: center; gap: 10px; font-size: 14px; }
  .role-tag {
    background: #ffb020; color: #4a3200; font-size: 12px;
    padding: 2px 8px; border-radius: 10px; font-weight: 600;
  }
  .link { background: none; border: none; color: #cfe0f0; text-decoration: underline; }
  main { padding: 24px; max-width: 1280px; margin: 0 auto; }

  :global(.card) {
    background: #fff; border-radius: 10px; padding: 20px;
    box-shadow: 0 1px 3px rgba(15, 76, 129, .08);
    margin-bottom: 16px;
  }
  :global(.card h3) { margin: 0 0 14px; font-size: 15px; color: #0f4c81; }
  :global(table) { width: 100%; border-collapse: collapse; font-size: 14px; }
  :global(th) {
    text-align: left; padding: 8px 10px; color: #6b7280; font-weight: 500;
    border-bottom: 2px solid #e5e7eb; white-space: nowrap;
  }
  :global(td) { padding: 9px 10px; border-bottom: 1px solid #f0f2f5; vertical-align: top; }
  :global(tr:hover td) { background: #f8fafc; }
  :global(.num) { text-align: right; font-variant-numeric: tabular-nums; white-space: nowrap; }
  :global(.btn) {
    background: #0f4c81; color: #fff; border: none; border-radius: 6px;
    padding: 7px 16px; font-size: 14px;
  }
  :global(.btn:hover) { background: #0d426f; }
  :global(.btn:disabled) { background: #9ca3af; cursor: not-allowed; }
  :global(.btn-sm) { padding: 3px 10px; font-size: 13px; border-radius: 5px; }
  :global(.btn-outline) {
    background: #fff; color: #0f4c81; border: 1px solid #0f4c81;
  }
  :global(.btn-outline:hover) { background: #eef4fa; }
  :global(.btn-danger) { background: #b91c1c; }
  :global(.btn-danger:hover) { background: #991b1b; }
  :global(.badge) {
    display: inline-block; padding: 2px 9px; border-radius: 10px;
    font-size: 12px; font-weight: 500; white-space: nowrap;
  }
  :global(.badge-blue) { background: #dbeafe; color: #1d4ed8; }
  :global(.badge-green) { background: #dcfce7; color: #15803d; }
  :global(.badge-purple) { background: #ede9fe; color: #6d28d9; }
  :global(.badge-gray) { background: #e5e7eb; color: #4b5563; }
  :global(.badge-orange) { background: #ffedd5; color: #c2410c; }
  :global(.muted) { color: #6b7280; font-size: 13px; }
  :global(.error-box) {
    background: #fef2f2; border: 1px solid #fecaca; color: #b91c1c;
    border-radius: 8px; padding: 10px 14px; margin: 10px 0; font-size: 14px;
  }
  :global(.ok-box) {
    background: #f0fdf4; border: 1px solid #bbf7d0; color: #15803d;
    border-radius: 8px; padding: 10px 14px; margin: 10px 0; font-size: 14px;
  }
</style>
