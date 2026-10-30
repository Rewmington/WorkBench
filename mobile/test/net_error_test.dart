/// 网络失败在状态行上只能说一句人话：不带 URI、不带异常类名。
library;

import 'dart:async';

import 'package:anvil_mobile/sync/net_error.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:http/http.dart' as http;

void main() {
  test('域名解析失败：告诉用户检查网络，而不是甩一整段异常', () {
    final e = http.ClientException(
      "Failed host lookup: 'dav.jianguoyun.com' (OS Error: No address associated "
      'with hostname, errno = 7), uri=https://dav.jianguoyun.com/dav/workbench-sync/',
    );
    final s = cloudNetworkError(e);
    expect(s, '连不上坚果云：域名解析失败，检查手机网络后再试');
    expect(s.contains('uri='), isFalse);
    expect(s.contains('ClientException'), isFalse);
  });

  test('超时与其它连接失败各一句', () {
    expect(cloudNetworkError(TimeoutException('slow')), contains('超时'));
    expect(cloudNetworkError(Exception('boom')), '连不上坚果云：检查网络后再试');
  });
}
