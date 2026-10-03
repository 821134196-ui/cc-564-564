// 轻量 API 封装：Bearer 令牌 + 统一错误抛出
let token = localStorage.getItem('token') || '';

export function getToken() {
  return token;
}
export function setToken(t) {
  token = t;
  if (t) localStorage.setItem('token', t);
  else localStorage.removeItem('token');
}

async function request(method, path, body) {
  const res = await fetch(path, {
    method,
    headers: {
      'Content-Type': 'application/json',
      ...(token ? { Authorization: `Bearer ${token}` } : {})
    },
    body: body ? JSON.stringify(body) : undefined
  });
  const text = await res.text();
  let data = null;
  if (text) {
    try { data = JSON.parse(text); } catch { data = text; }
  }
  if (!res.ok) {
    const message = data && data.message ? data.message : `请求失败 HTTP ${res.status}`;
    const err = new Error(message);
    err.status = res.status;
    err.code = data && data.error;
    throw err;
  }
  return data;
}

export const api = {
  me: () => request('GET', '/api/me'),
  users: () => request('GET', '/api/users'),
  bills: () => request('GET', '/api/bills'),
  bill: (id) => request('GET', `/api/bills/${id}`),
  execute: (billId, itemId, qty) =>
    request('POST', `/api/bills/${billId}/items/${itemId}/execute`, { qty }),
  cancel: (billId, itemId, reason, remark) =>
    request('POST', `/api/bills/${billId}/items/${itemId}/cancel`, { reason, remark }),
  issueReport: (billId, itemId) =>
    request('POST', `/api/bills/${billId}/items/${itemId}/issue-report`),
  createRefund: (billId, payload) =>
    request('POST', `/api/bills/${billId}/refunds`, payload),
  voidReport: (reportId, reason) =>
    request('POST', `/api/reports/${reportId}/void`, { reason }),
  trace: (refundId) => request('GET', `/api/refunds/${refundId}/trace`)
};
