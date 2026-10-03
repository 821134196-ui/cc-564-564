const TOKEN_KEY = 'clinic_token'
const USER_KEY = 'clinic_user'

export function getToken() { return localStorage.getItem(TOKEN_KEY) || '' }
export function getUser() {
  try { return JSON.parse(localStorage.getItem(USER_KEY) || 'null') } catch { return null }
}
export function saveSession(token, user) {
  localStorage.setItem(TOKEN_KEY, token)
  localStorage.setItem(USER_KEY, JSON.stringify(user))
}
export function clearSession() {
  localStorage.removeItem(TOKEN_KEY)
  localStorage.removeItem(USER_KEY)
}

export class ApiError extends Error {
  constructor(status, body) {
    super(body?.error || `请求失败 (${status})`)
    this.status = status
    this.body = body || {}
    this.skipped = body?.skipped || []
  }
}

export async function api(method, path, body) {
  const headers = { 'content-type': 'application/json' }
  const token = getToken()
  if (token) headers['authorization'] = `Bearer ${token}`
  const resp = await fetch(path, {
    method,
    headers,
    body: body !== undefined ? JSON.stringify(body) : undefined
  })
  const data = await resp.json().catch(() => null)
  if (!resp.ok) throw new ApiError(resp.status, data)
  return data
}

export const get = (p) => api('GET', p)
export const post = (p, b) => api('POST', p, b ?? {})
