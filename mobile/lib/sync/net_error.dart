/// 坚果云请求失败时的**一句人话**（§5.5 的纪律：状态行不放异常栈和 URI）。
/// 同步与药箱两条路共用这一份，免得同一种失败在两处说两种话。
library;

String cloudNetworkError(Object e) {
  final s = e.toString();
  if (s.contains('TimeoutException')) {
    return '连接坚果云超时：检查网络，或稍后再试';
  }
  if (s.contains('Failed host lookup') || s.contains('No address associated')) {
    return '连不上坚果云：域名解析失败，检查手机网络后再试';
  }
  return '连不上坚果云：检查网络后再试';
}
