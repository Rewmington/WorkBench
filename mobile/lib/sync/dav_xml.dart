/// WebDAV 的 PROPFIND 多状态解析。备份、药箱两条只读路共用这一份 ——
/// 坚果云的命名空间前缀不稳定（`D:` / `s0:` 都见过），按本地名取元素才换不坏。
library;

import 'package:xml/xml.dart';

class DavEntry {
  final String name;
  final int size;
  final bool isDir;
  const DavEntry(this.name, this.size, this.isDir);
}

class DavParseException implements Exception {
  final String message;
  DavParseException(this.message);
  @override
  String toString() => message;
}

List<DavEntry> parseMultistatus(String xml, String collectionHref) {
  late XmlDocument doc;
  try {
    doc = XmlDocument.parse(xml);
  } catch (e) {
    throw DavParseException('列目录返回的不是合法 XML：$e');
  }
  final out = <DavEntry>[];
  for (final response in doc.descendants.whereType<XmlElement>().where(
        (e) => e.name.local == 'response',
      )) {
    final href = response.descendants
        .whereType<XmlElement>()
        .where((e) => e.name.local == 'href')
        .map((e) => e.innerText.trim())
        .firstOrNull;
    if (href == null) continue;
    final isDir = response.descendants
        .whereType<XmlElement>()
        .any((e) => e.name.local == 'collection');
    final size = int.tryParse(response.descendants
            .whereType<XmlElement>()
            .where((e) => e.name.local == 'getcontentlength')
            .map((e) => e.innerText.trim())
            .firstOrNull ??
        '') ??
        0;
    final raw = href.split('/').where((s) => s.isNotEmpty).lastOrNull ?? '';
    final name = percentDecode(raw);
    // 集合自己也在响应里，跳过它
    if (name.isEmpty ||
        href.replaceFirst(RegExp(r'/+$'), '') ==
            collectionHref.replaceFirst(RegExp(r'/+$'), '')) {
      continue;
    }
    out.add(DavEntry(name, size, isDir));
  }
  return out;
}

String percentDecode(String raw) {
  try {
    return Uri.decodeComponent(raw);
  } catch (_) {
    return raw;
  }
}
