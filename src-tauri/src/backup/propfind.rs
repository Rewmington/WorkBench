use roxmltree::Document;

use crate::error::{Error, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub size: u64,
    pub is_dir: bool,
}

/// 解析 PROPFIND 的 207 多状态响应。
///
/// 坚果云返回的命名空间前缀不稳定（`D:` / `s0:` 都见过），所以这里**按本地名取元素**
/// 而不是写死前缀 —— 写死前缀的解析器换个服务器就全空了。
pub fn parse_multistatus(xml: &str, collection_href: &str) -> Result<Vec<Entry>> {
    let doc = Document::parse(xml).map_err(|e| Error::msg(format!("列目录返回的不是合法 XML：{e}")))?;

    let mut out = Vec::new();
    // roxmltree 0.21 的 `root()` 是 document 节点，multistatus 是它的孩子，
    // 所以 response 只能从 descendants 里取，root().children() 会一个都找不到。
    for response in doc.descendants().filter(|n| n.has_tag_name("response")) {
        let href = response
            .children()
            .find(|n| n.has_tag_name("href"))
            .and_then(|n| n.text().map(str::to_owned));
        let Some(href) = href else { continue };

        let is_dir = response
            .descendants()
            .any(|n| n.has_tag_name("collection"));
        let size = response
            .descendants()
            .find(|n| n.has_tag_name("getcontentlength"))
            .and_then(|n| n.text()?.parse().ok())
            .unwrap_or(0);

        let name = href
            .rsplit('/')
            .next()
            .filter(|s| !s.is_empty())
            .map(percent_decode)
            .unwrap_or_default();
        // 集合自己也在响应里，跳过它
        if name.is_empty() || href.trim_end_matches('/') == collection_href.trim_end_matches('/') {
            continue;
        }
        out.push(Entry { name, size, is_dir });
    }
    Ok(out)
}

/// WebDAV 的 href 是百分号编码的，文件名里有中文/空格时要还原。
fn percent_decode(raw: &str) -> String {
    let bytes = raw.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(v) = u8::from_str_radix(&raw[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

pub fn percent_encode(name: &str) -> String {
    let mut out = String::new();
    for byte in name.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*byte as char)
            }
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 桩服务器故意混用两种前缀，解析必须都不漏
    const MIXED: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:" xmlns:s0="DAV:">
  <D:response><D:href>/dav/workbench-backup/</D:href>
    <D:propstat><D:prop><D:resourcetype><D:collection/></D:resourcetype></D:prop></D:propstat></D:response>
  <s0:response><s0:href>/dav/workbench-backup/anvil-20261028T220315Z-1a2b3c4d.abackup</s0:href>
    <s0:propstat><s0:prop><s0:getcontentlength>4096</s0:getcontentlength>
      <D:resourcetype/></s0:prop></s0:propstat></s0:response>
  <D:response><D:href>/dav/workbench-backup/%E4%B8%AD%E6%96%87.abackup</D:href>
    <D:propstat><D:prop><D:getcontentlength>12</D:getcontentlength><D:resourcetype/></D:prop></D:propstat></D:response>
</D:multistatus>"#;

    #[test]
    fn reads_entries_regardless_of_namespace_prefix() {
        let entries = parse_multistatus(MIXED, "/dav/workbench-backup/").unwrap();
        assert_eq!(entries.len(), 2, "集合自身不该算进去：{entries:?}");
        assert_eq!(entries[0].name, "anvil-20261028T220315Z-1a2b3c4d.abackup");
        assert_eq!(entries[0].size, 4096);
        assert!(!entries[0].is_dir);
        assert_eq!(entries[1].name, "中文.abackup", "href 要还原百分号编码");
    }

    #[test]
    fn garbage_is_reported_not_silently_empty() {
        let err = parse_multistatus("<not xml", "/x/").unwrap_err();
        assert!(err.to_string().contains("XML"), "{err}");
    }

    #[test]
    fn encode_decode_round_trips_a_chinese_name() {
        let name = "快照 01.abackup";
        let enc = percent_encode(name);
        assert!(!enc.contains(' '), "{enc}");
        assert_eq!(percent_decode(&enc), name);
    }
}
