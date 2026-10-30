/// 只实现 Anvil 同步用到的那四个动词的 WebDAV 桩（MKCOL / PROPFIND / GET / PUT）。
/// 与仓库根的 `tools/webdav-stub.mjs` 同一个角色，但跑在测试进程里，不需要另起 node。
library;

import 'dart:convert';
import 'dart:io';

class DavStub {
  final HttpServer _server;
  final String user;
  final String pass;

  /// 目录（默认是工作台同步那条；药箱只读测试用 `/dav/medicine-sync/`）。
  final String segment;

  /// 文件名 → 字节。只放同步目录那一层的文件。
  final Map<String, List<int>> files = {};
  bool dirExists = false;
  int puts = 0;

  /// 收到过的动词，用来钉「只读客户端连写方法都不发」。
  final List<String> methods = [];

  DavStub._(this._server, this.user, this.pass, this.segment) {
    _server.listen(_handle);
  }

  static Future<DavStub> start({
    String user = 'me@example.com',
    String pass = 'app-pass',
    String segment = '/dav/workbench-sync/',
  }) async {
    final server = await HttpServer.bind(InternetAddress.loopbackIPv4, 0);
    return DavStub._(server, user, pass, segment);
  }

  String get base => 'http://127.0.0.1:${_server.port}$segment';
  String get collectionPath => segment;

  Future<void> close() => _server.close(force: true);

  bool _authed(HttpRequest req) {
    final want = 'Basic ${base64.encode(utf8.encode('$user:$pass'))}';
    return req.headers.value(HttpHeaders.authorizationHeader) == want;
  }

  Future<void> _handle(HttpRequest req) async {
    final resp = req.response;
    methods.add(req.method);
    if (!_authed(req)) {
      resp.statusCode = 401;
      await resp.close();
      return;
    }
    final path = req.uri.path;
    final name = path.startsWith(collectionPath)
        ? Uri.decodeComponent(path.substring(collectionPath.length))
        : '';

    switch (req.method) {
      case 'MKCOL':
        if (path != collectionPath) {
          resp.statusCode = 409;
        } else if (dirExists) {
          resp.statusCode = 405;
        } else {
          dirExists = true;
          resp.statusCode = 201;
        }
        break;
      case 'PROPFIND':
        if (!dirExists) {
          resp.statusCode = 404;
        } else {
          resp.statusCode = 207;
          resp.headers.contentType = ContentType('application', 'xml', charset: 'utf-8');
          resp.write(_multistatus());
        }
        break;
      case 'GET':
        final bytes = files[name];
        if (dirExists && bytes != null) {
          resp.statusCode = 200;
          resp.add(bytes);
        } else {
          resp.statusCode = 404;
        }
        break;
      case 'PUT':
        if (!dirExists) {
          resp.statusCode = 409;
        } else {
          files[name] = await req.fold<List<int>>(<int>[], (a, b) => a..addAll(b));
          puts++;
          resp.statusCode = 201;
        }
        break;
      default:
        resp.statusCode = 405;
    }
    await resp.close();
  }

  /// 命名空间前缀故意混用（`D:` 与 `s0:`），解析器按本地名取元素才过得去。
  String _multistatus() {
    final b = StringBuffer('<?xml version="1.0" encoding="utf-8"?>')
      ..write('<D:multistatus xmlns:D="DAV:" xmlns:s0="DAV:">')
      ..write('<D:response><D:href>$collectionPath</D:href>'
          '<D:propstat><D:prop><D:resourcetype><D:collection/></D:resourcetype>'
          '</D:prop></D:propstat></D:response>');
    for (final entry in files.entries) {
      b.write('<s0:response><s0:href>$collectionPath${entry.key}</s0:href>'
          '<s0:propstat><s0:prop><s0:getcontentlength>${entry.value.length}'
          '</s0:getcontentlength><D:resourcetype/></s0:prop></s0:propstat></s0:response>');
    }
    b.write('</D:multistatus>');
    return b.toString();
  }
}
