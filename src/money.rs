//! 金额分摊工具：最大余数法，保证分摊结果之和严格等于总额（单位：分）。

/// 将 `amount` 按 `weights` 的比例拆分，返回与 weights 等长的结果。
/// 采用最大余数法：先取整，再把剩余 1 分逐个分给小数余数最大（并列时权重更大、下标更小）的项。
/// 所有权重为 0 时返回 None。
pub fn split_proportional(amount: i64, weights: &[i64]) -> Option<Vec<i64>> {
    let total: i128 = weights.iter().map(|w| *w as i128).sum();
    if total <= 0 || amount < 0 {
        return None;
    }
    let amount = amount as i128;
    let mut parts: Vec<i64> = Vec::with_capacity(weights.len());
    let mut remainders: Vec<(i128, i64, usize)> = Vec::with_capacity(weights.len()); // (余数, 权重, 下标)
    let mut assigned: i128 = 0;
    for (i, w) in weights.iter().enumerate() {
        let w = *w as i128;
        let exact = amount * w; // 分子，分母为 total
        let floor = exact / total;
        let rem = exact % total;
        parts.push(floor as i64);
        assigned += floor;
        remainders.push((rem, weights[i], i));
    }
    let mut leftover = (amount - assigned) as i64;
    // 余数降序；余数相同权重降序；再相同下标升序，保证结果确定
    remainders.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)).then(a.2.cmp(&b.2)));
    let mut i = 0usize;
    while leftover > 0 {
        let idx = remainders[i % remainders.len()].2;
        parts[idx] += 1;
        leftover -= 1;
        i += 1;
    }
    Some(parts)
}

/// 按 (渠道, 权重) 拆分金额，返回 (渠道, 金额) 列表，自动跳过权重为 0 的渠道。
pub fn split_by_method(amount: i64, method_weights: &[(String, i64)]) -> Option<Vec<(String, i64)>> {
    let active: Vec<&(String, i64)> = method_weights.iter().filter(|(_, w)| *w > 0).collect();
    if active.is_empty() {
        return None;
    }
    let weights: Vec<i64> = active.iter().map(|(_, w)| *w).collect();
    let parts = split_proportional(amount, &weights)?;
    Some(
        active
            .iter()
            .zip(parts.iter())
            .map(|((m, _), a)| (m.clone(), *a))
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_sum_equals_total() {
        // 套餐 800.00 按原价 [100,60,200,150,600] 分摊
        let parts = split_proportional(80000, &[10000, 6000, 20000, 15000, 60000]).unwrap();
        assert_eq!(parts.iter().sum::<i64>(), 80000);
        assert_eq!(parts, vec![7207, 4324, 14415, 10811, 43243]);
    }

    #[test]
    fn split_handles_zero_weights() {
        let parts = split_proportional(1000, &[0, 3, 0, 1]).unwrap();
        assert_eq!(parts, vec![0, 750, 0, 250]);
    }

    #[test]
    fn split_all_zero_returns_none() {
        assert!(split_proportional(1000, &[0, 0]).is_none());
    }

    #[test]
    fn split_one_cent_round_robin() {
        let parts = split_proportional(2, &[1, 1, 1]).unwrap();
        assert_eq!(parts.iter().sum::<i64>(), 2);
    }

    #[test]
    fn split_by_method_skips_zero_balance() {
        let r = split_by_method(1000, &[("insurance".into(), 0), ("cash".into(), 500)]).unwrap();
        assert_eq!(r, vec![("cash".to_string(), 1000)]);
    }
}
