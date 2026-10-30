/// 手机端的坚果云中转：`/dav/workbench-sync/`（PLAN-MOBILE §3.2、§3.6）。
///
/// 与 PC `src-tauri/src/sync/relay.rs` 同一条路：`MKCOL`（首次，405 视为已存在）→
/// `PROPFIND` Depth:1 列目录 → `GET` 别人那些 `device-*.enc` → 合并 →
/// 有本地写入才 `PUT` 自己那份 → `clear_dirty(那份快照自己的 sealed_at)`。
///
/// 目录钉子写在类型层（§3.6 那张表），不靠自觉：只接受以 `/workbench-sync/` 结尾的 base，
/// 拒绝 `medicine-sync` / `workbench-backup`；文件名只认 `device-*.enc`。
library;

import 'dart:convert';
import 'dart:typed_data';

import 'package:http/http.dart' as http;

import 'dav_xml.dart';
import 'net_error.dart';

export 'dav_xml.dart' show DavEntry, DavParseException;

const defaultBase = 'https://dav.jianguoyun.com/dav/workbench-sync/';
const requiredSegment = '/workbench-sync/';
const filePrefix = 'device-';
const fileSuffix = '.enc';

/// 一次最多读几份对端文件（1–2 台手机 + 1 台 PC 的真实用量上留余量）。
const maxPeers = 8;

/// 单份对端文件的体积上限：个人量级的 gzip 快照是几十 KB，超这数说明地址填到了别处。
const maxBytes = 32 * 1024 * 1024;

class WebDavException implements Exception {
  final String message;
  WebDavException(this.message);
  @override
  String toString() => message;
}

String fileNameFor(String device) => '$filePrefix$device$fileSuffix';

String _peerId(String name) => name
    .replaceFirst(RegExp('^$filePrefix'), '')
    .replaceFirst(RegExp('${RegExp.escape(fileSuffix)}\$'), '');

/// 把备份的 base 折成同步的 base：同一账号、父目录下的**兄弟目录**。
/// 与 Rust `relay::sync_base_from_backup_base` 同一条；测试桩也靠它指路。
String syncBaseFromBackupBase(String backupBase) {
  final trimmed = backupBase.trim().replaceFirst(RegExp(r'/+$'), '');
  final cut = trimmed.lastIndexOf('/');
  if (cut < 0) return trimmed;
  return '${trimmed.substring(0, cut)}/workbench-sync/';
}

class WebDavRemote {
  final String base;
  final String user;
  final String pass;
  final http.Client client;
  final Duration timeout;

  WebDavRemote._(this.base, this.user, this.pass, this.client, this.timeout);

  /// `base` 必须以 `/workbench-sync/` 结尾。§3.6：写错目录不是小事，直接拒绝而不是警告。
  factory WebDavRemote(
    String base,
    String user,
    String pass, {
    http.Client? client,
    Duration timeout = const Duration(seconds: 60),
  }) {
    final trimmed = base.trim();
    if (!trimmed.endsWith(requiredSegment)) {
      throw WebDavException('同步目录必须是 $requiredSegment 结尾的独立目录，不能与备份或药箱共用');
    }
    if (trimmed.contains('medicine-sync')) {
      throw WebDavException('那是药箱的同步目录，工作台的手机同步必须写进自己的 workbench-sync');
    }
    if (trimmed.contains('workbench-backup')) {
      throw WebDavException('那是每日备份的目录，手机同步的 device-*.enc 会把备份客户端的列目录搅浑');
    }
    if (user.trim().isEmpty || pass.trim().isEmpty) {
      throw WebDavException('还没在设置里填坚果云邮箱与应用密码');
    }
    return WebDavRemote._(
      trimmed,
      user.trim(),
      pass,
      client ?? http.Client(),
      timeout,
    );
  }

  String _url([String? name]) {
    if (name == null) return base;
    if (!name.startsWith(filePrefix)) {
      throw WebDavException('同步目录里只认 $filePrefix*$fileSuffix 这一种文件名，「$name」不是');
    }
    return '$base${Uri.encodeComponent(name)}';
  }

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
    if (body != null) {
      req.bodyBytes = Uint8List.fromList(body);
    }
    try {
      return await client.send(req).timeout(timeout);
    } on WebDavException {
      rethrow;
    } catch (e) {
      throw WebDavException(cloudNetworkError(e));
    }
  }

  Future<Uint8List> _read(http.StreamedResponse resp) async {
    final bytes = await resp.stream.toBytes().timeout(timeout);
    if (bytes.length > maxBytes) {
      throw WebDavException('文件体积异常（超过 32 MiB），已停止读取');
    }
    return bytes;
  }

  /// 401/403 的文案沿用备份那条已经调好的句子 —— 它准确指出了最常见的两个错因。
  Never _httpError(int code) {
    if (code == 401 || code == 403) {
      throw WebDavException(
          '坚果云拒绝了凭据：检查账号是否填的是注册邮箱、密码是否用「第三方应用管理」里的应用密码（不是登录密码）');
    }
    if (code == 404) {
      throw WebDavException('同步目录还不存在：它可能从没同步过，或完整路径填错了');
    }
    throw WebDavException('坚果云返回了 $code');
  }

  /// 坚果云不允许在根下直接建文件，必须先有集合。
  /// 405（已存在）与 409（父级不存在，多半是路径填错了）都在这里翻成中文。
  Future<void> ensureDir() async {
    final resp = await _send('MKCOL', Uri.parse(_url()), const {}, null);
    final code = resp.statusCode;
    // 响应体要丢掉，否则连接不会回到池里
    await resp.stream.drain<void>();
    switch (code) {
      case 201:
      case 204:
      case 405:
        return;
      case 409:
        throw WebDavException('上层目录不存在：检查坚果云里 workbench-sync 的完整路径');
      default:
        _httpError(code);
    }
  }

  /// 列目录，只返回 `device-*.enc` —— 备份件、目录、以及任何别的名字都不该被上层看见。
  Future<List<DavEntry>> list() async {
    final body = '<?xml version="1.0" encoding="utf-8"?>'
        '<D:propfind xmlns:D="DAV:"><D:prop><D:displayname/>'
        '<D:getcontentlength/><D:resourcetype/></D:prop></D:propfind>';
    final resp = await _send(
      'PROPFIND',
      Uri.parse(_url()),
      const {
        'Depth': '1',
        'Content-Type': 'application/xml; charset=utf-8',
      },
      utf8.encode(body),
    );
    final bytes = await _read(resp);
    if (resp.statusCode != 207 && resp.statusCode != 200) {
      _httpError(resp.statusCode);
    }
    final collection = Uri.parse(base).path;
    final entries = parseMultistatus(utf8.decode(bytes), collection);
    return [
      for (final e in entries)
        if (!e.isDir && e.name.startsWith(filePrefix) && e.name.endsWith(fileSuffix)) e,
    ];
  }

  Future<void> put(String name, List<int> bytes) async {
    final resp = await _send(
      'PUT',
      Uri.parse(_url(name)),
      const {'Content-Type': 'application/octet-stream'},
      bytes,
    );
    await resp.stream.drain<void>();
    if (resp.statusCode != 200 && resp.statusCode != 201 && resp.statusCode != 204) {
      _httpError(resp.statusCode);
    }
  }

  Future<Uint8List> get(String name) async {
    final resp = await _send('GET', Uri.parse(_url(name)), const {}, null);
    final bytes = await _read(resp);
    if (resp.statusCode != 200) _httpError(resp.statusCode);
    return bytes;
  }

  void close() => client.close();

  /// 对端文件的失败要点名是哪一台机器：「读远端失败」和「读远端 device-phone-a.enc 失败：…」
  /// 是两种可执行程度完全不同的句子。
  static WebDavException wrapPeer(String name, Object e) =>
      WebDavException('读远端 $name 失败：${e is WebDavException ? e.message : e}');

  static String peerIdOf(String name) => _peerId(name);
  static String ourFileName(String device) => fileNameFor(device);
}

