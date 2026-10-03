// 金额单位：分。展示为人民币元。
export function yuan(fen) {
  if (fen === null || fen === undefined) return '—';
  const neg = fen < 0;
  const abs = Math.abs(fen);
  const s = (abs / 100).toLocaleString('zh-CN', { minimumFractionDigits: 2, maximumFractionDigits: 2 });
  return `${neg ? '-' : ''}¥${s}`;
}

export function pctFromBp(bp) {
  return `${(bp / 100).toFixed(2).replace(/\.?0+$/, '')}%`;
}

export const ITEM_STATUS = {
  SCHEDULED: '待执行',
  EXECUTED: '已执行',
  PARTIALLY_EXECUTED: '部分执行',
  CANCELLED: '已取消',
  REFUNDED: '已退款',
  PARTIALLY_REFUNDED: '部分退款'
};

export const REPORT_STATUS = {
  NONE: '未出报告',
  ISSUED: '已出报告',
  VOIDED: '报告已作废'
};

export const REFUND_STATUS = {
  AWAITING_VOID: '待报告作废',
  COMPLETED: '已完成',
  REJECTED: '已驳回'
};

export const CANCEL_REASON = {
  DOCTOR_CANCEL: '医生取消检查',
  EQUIPMENT_FAULT: '设备故障'
};

export const ROLE_LABEL = {
  DOCTOR: '医生',
  CASHIER: '收费人员',
  REPORT_STAFF: '报告处理人员',
  PATIENT: '患者'
};
