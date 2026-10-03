<script>
  import { post, saveSession } from '../api.js'

  let { onLogin } = $props()

  const accounts = [
    { username: 'doctor', name: '张医生', role: '医生 · 开单/取消医嘱/标记执行' },
    { username: 'cashier', name: '李收费', role: '收费员 · 开账单/办理退项' },
    { username: 'reporter', name: '王报告', role: '报告处理员 · 签发/作废报告' },
    { username: 'admin', name: '管理员', role: '管理员 · 全部权限' },
  ]

  let username = $state('cashier')
  let password = $state('123456')
  let error = $state('')
  let loading = $state(false)

  async function submit() {
    error = ''
    loading = true
    try {
      const data = await post('/api/auth/login', { username, password })
      saveSession(data.token, data.user)
      onLogin(data.user)
    } catch (e) {
      error = e.message
    } finally {
      loading = false
    }
  }
</script>

<div class="login-page">
  <div class="login-card">
    <h1>🏥 诊所退项与费用调整系统</h1>
    <p class="muted">检查项目退项 · 费用调整 · 退款构成核对</p>

    <div class="accounts">
      {#each accounts as a}
        <button
          class="account" class:selected={username === a.username}
          onclick={() => { username = a.username; password = '123456' }}
        >
          <strong>{a.name}</strong>
          <span>{a.role}</span>
        </button>
      {/each}
    </div>

    <form onsubmit={(e) => { e.preventDefault(); submit() }}>
      <label>账号
        <input bind:value={username} autocomplete="username" />
      </label>
      <label>密码
        <input type="password" bind:value={password} autocomplete="current-password" />
      </label>
      {#if error}<div class="error-box">{error}</div>{/if}
      <button class="btn" type="submit" disabled={loading}>
        {loading ? '登录中…' : '登 录'}
      </button>
      <p class="muted center">演示账号密码均为 123456</p>
    </form>
  </div>
</div>

<style>
  .login-page {
    min-height: 100vh; display: flex; align-items: center; justify-content: center;
    background: linear-gradient(135deg, #0f4c81 0%, #1a6db0 100%);
  }
  .login-card {
    background: #fff; border-radius: 14px; padding: 36px 40px;
    width: 460px; box-shadow: 0 12px 40px rgba(0,0,0,.25);
  }
  h1 { font-size: 20px; margin: 0 0 4px; color: #0f4c81; }
  .accounts { display: grid; grid-template-columns: 1fr 1fr; gap: 8px; margin: 18px 0; }
  .account {
    border: 1.5px solid #e5e7eb; background: #f9fafb; border-radius: 8px;
    padding: 10px 12px; text-align: left; display: flex; flex-direction: column; gap: 2px;
  }
  .account span { font-size: 12px; color: #6b7280; }
  .account.selected { border-color: #0f4c81; background: #eef4fa; }
  form { display: flex; flex-direction: column; gap: 12px; }
  label { display: flex; flex-direction: column; gap: 4px; font-size: 13px; color: #374151; }
  input { padding: 8px 10px; border: 1px solid #d1d5db; border-radius: 6px; }
  .center { text-align: center; }
</style>
