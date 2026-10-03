export function fen(cents) {
  if (cents === null || cents === undefined) return '-'
  const sign = cents < 0 ? '-' : ''
  return `${sign}¥${(Math.abs(cents) / 100).toFixed(2)}`
}

export const STATUS_LABEL = {
  pending: '待执行',
  executed: '已执行',
  reported: '已出报告',
  refunded: '已退项'
}

export const STATUS_CLASS = {
  pending: 'badge-blue',
  executed: 'badge-green',
  reported: 'badge-purple',
  refunded: 'badge-gray'
}

export const BILL_STATUS_LABEL = {
  paid: '已收费',
  partially_refunded: '部分退项',
  fully_refunded: '全部退项'
}

export const REASON_LABEL = {
  doctor_cancel: '医生取消检查',
  equipment_failure: '设备故障无法执行',
  patient_request: '患者要求退项',
  report_voided: '报告作废后退款'
}

export const METHOD_LABEL = {
  insurance: '医保',
  cash: '现金',
  wechat: '微信',
  alipay: '支付宝'
}

export const FLAG_LABEL = {
  doctor_cancel: '医生已取消',
  equipment_failure: '设备故障'
}

export const ROLE_LABEL = {
  doctor: '医生',
  cashier: '收费员',
  reporter: '报告处理员',
  admin: '管理员'
}
