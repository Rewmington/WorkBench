/// 药箱的只读云端：`/dav/medicine-sync/device-*.json`。
///
/// 药箱只在手机上用，它自己会把全量表发布到这里；Anvil 只做 **PROPFIND + GET**，
/// 类型上就没有 MKCOL / PUT —— 不建目录、不写文件、不碰药箱的任何数据。
///
/// 与工作台自己的同步目录严格分开：base 必须以 `/medicine-sync/` 结尾，
/// 拒绝 `workbench-sync` / `workbench-backup`。凭据复用同一组坚果云账号
/// （应用密码是账号级的，任一把都能读；Anvil 不读药箱存在它自己 prefs 里的那把）。
library;

import 'dart:async';
import 'dart:convert';
import 'dart:typed_data';

import 'package:http/http.dart' as http;

import 'dav_xml.dart';
import 'net_error.dart';
import '../medicine.dart';

const medicineSegment = '/medicine-sync/';
const medicineCloudBase = 'https://dav.jianguoyun.com/dav/medicine-sync/';
const _filePrefix = 'device-';
const _fileSuffix = '.json';

/// 真实使用是 1–2 台设备；上限只是别对着陌生目录无界地拉。
const _maxFiles = 8;
const _maxBytes = 16 * 1024 * 1024;

class MedicineCloudException implements Exception {
  final String message;
  MedicineCloudException(this.message);
  @override
  String toString() => message;
}

class MedicineFetch {
  final List<MedicineDoc> docs;
  final bool dirMissing;
  final int fetchedAt;

  const MedicineFetch({
    required this.docs,
    required this.dirMissing,
    required this.fetchedAt,
  });
}

class MedicineCloud {
  final String base;
  final String user;
  final String pass;
  final http.Client client;
  final Duration timeout;

  MedicineCloud._(this.base, this.user, this.pass, this.client, this.timeout);

  factory MedicineCloud(
    String base,
    String user,
    String pass, {
    http.Client? client,
    Duration timeout = const Duration(seconds: 45),
  }) {
    final trimmed = base.trim();
    if (!trimmed.endsWith(medicineSegment)) {
      throw MedicineCloudException('药箱目录必须是 $medicineSegment 结尾，不能与工作台同步目录共用');
    }
    if (trimmed.contains('workbench-sync') || trimmed.contains('workbench-backup')) {
      throw MedicineCloudException('那是工作台自己的目录；药箱只读视图只读 medicine-sync');
    }
    if (user.trim().isEmpty || pass.trim().isEmpty) {
      throw MedicineCloudException('还没在设置里填坚果云邮箱与应用密码');
    }
    return MedicineCloud._(trimmed, user.trim(), pass, client ?? http.Client(), timeout);
  }

  String get _collection => Uri.parse(base).path;

  Map<String, String> get _auth => {
        'Authorization': 'Basic ${base64.encode(utf8.encode('$user:$pass'))}',
      };

  Future<http.StreamedResponse> _send(
    String method,
    Uri uri,
    Map<String, String> headers,
    List<int>? body,
  ) async {
    final req = http.Request(method, uri)
      ..headers.addAll(_auth)
      ..headers.addAll(headers);
    if (body != null) req.bodyBytes = Uint8List.fromList(body);
    try {
      return await client.send(req).timeout(timeout);
    } catch (e) {
      throw MedicineCloudException(cloudNetworkError(e));
    }
  }

  /// 与 `webdav.dart` 同一条规则：有界读，越界当场停；不要先把整份响应攒进内存再判断长度。
  Future<Uint8List> _read(http.StreamedResponse resp) async {
    final out = BytesBuilder(copy: false);
    try {
      await for (final chunk in resp.stream.timeout(timeout)) {
        out.add(chunk);
        if (out.length > _maxBytes) {
          throw MedicineCloudException('药箱快照体积异常（超过 16 MiB），已停止读取');
        }
      }
    } on TimeoutException {
      throw MedicineCloudException('读响应超时：对端没有把整份快照送完');
    }
    return out.toBytes();
  }

  Never _httpError(int code) {
    if (code == 401 || code == 403) {
      throw MedicineCloudException(
          '坚果云拒绝了凭据：检查账号是否填的是注册邮箱、密码是否用「第三方应用管理」里的应用密码（不是登录密码）');
    }
    throw MedicineCloudException('坚果云返回了 $code');
  }

  /// 读整个目录。目录不存在（药箱从没同步过）不是错误，`dirMissing=true` 由界面说人话。
  Future<MedicineFetch> fetch({required int nowMs}) async {
    final body = '<?xml version="1.0" encoding="utf-8"?>'
        '<D:propfind xmlns:D="DAV:"><D:prop><D:displayname/>'
        '<D:getcontentlength/><D:resourcetype/></D:prop></D:propfind>';
    final resp = await _send(
      'PROPFIND',
      Uri.parse(base),
      const {
        'Depth': '1',
        'Content-Type': 'application/xml; charset=utf-8',
      },
      utf8.encode(body),
    );
    final bytes = await _read(resp);
    if (resp.statusCode == 404) {
      return MedicineFetch(docs: const [], dirMissing: true, fetchedAt: nowMs);
    }
    if (resp.statusCode != 207 && resp.statusCode != 200) {
      _httpError(resp.statusCode);
    }
    final entries = parseMultistatus(utf8.decode(bytes), _collection)
        .where((e) =>
            !e.isDir && e.name.startsWith(_filePrefix) && e.name.endsWith(_fileSuffix))
        .take(_maxFiles)
        .toList();

    final docs = <MedicineDoc>[];
    for (final entry in entries) {
      final name = entry.name;
      final get = await _send(
        'GET',
        Uri.parse('$base${Uri.encodeComponent(name)}'),
        const {},
        null,
      );
      final raw = await _read(get);
      if (get.statusCode != 200) _httpError(get.statusCode);
      late Object? root;
      try {
        root = jsonDecode(utf8.decode(raw));
      } catch (_) {
        throw MedicineCloudException('药箱的 $name 不是合法 JSON：文件被改过或版本不认识');
      }
      if (root is! Map<String, Object?>) {
        throw MedicineCloudException('药箱的 $name 根节点不是对象');
      }
      docs.add(MedicineDoc.fromJson(root));
    }
    return MedicineFetch(docs: docs, dirMissing: false, fetchedAt: nowMs);
  }

  void close() => client.close();
}

