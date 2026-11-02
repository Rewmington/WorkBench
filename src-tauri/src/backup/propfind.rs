use roxmltree::Document;

use crate::error::{Error, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub size: u64,
    pub is_dir: bool,
    /// `getlastmodified` 折成 epoch 秒。服务器没给、或给的不是 HTTP 日期时为 `None`
    /// —— 那是"不知道"，不是"很旧"，排序时与"知道"的分开对待。
    pub modified: Option<i64>,
}

/// WebDAV 的时间戳是 IMF-fixdate（`Tue, 28 Oct 2026 22:03:15 GMT`，RFC 7231）。
/// chrono 的 `parse_from_rfc2822` 在这个形状上会失败 —— 它只认数字时区，而按 HTTP
/// 规范发的是字面量 `GMT`。所以先把"零偏移的那几种写法"折成 `+0000`，别的原样交给它。
/// 解析不动就当没有：宁可退回"按 listing 顺序"，也不要把一个读不懂的日期当成 0 ——
/// 那会让这份信封在"取最新几份"的排序里永远垫底，等于悄悄宣布它不存在。
/// chrono 还会核对星期名与日期是否一致（`Tue, 28 Oct 2026` 其实是周三 → 解析失败），
/// 所以时钟写错星期名的服务器会落到"不知道"这一档，而不是被当成同一时刻。
fn parse_http_date(raw: &str) -> Option<i64> {
    let s = raw.trim();
    let parsed = match s.rsplit_once(' ') {
        Some((head, zone))
            if matches!(
                zone.to_ascii_uppercase().as_str(),
                "GMT" | "UTC" | "UT" | "Z" | "-0000"
            ) =>
        {
            chrono::DateTime::parse_from_rfc2822(&format!("{head} +0000"))
        }
        _ => chrono::DateTime::parse_from_rfc2822(s),
    };
    parsed.map(|d| d.timestamp()).ok()
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
        let modified = response
            .descendants()
            .find(|n| n.has_tag_name("getlastmodified"))
            .and_then(|n| n.text())
            .and_then(parse_http_date);

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
        out.push(Entry {
            name,
            size,
            is_dir,
            modified,
        });
    }
    Ok(out)
}

/// WebDAV 的 href 是百分号编码的，文件名里有中文/空格时要还原。
pub(crate) fn percent_decode(raw: &str) -> String {
    let bytes = raw.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        // 先确认后面两枚是 ASCII 十六进制，再切 `&raw[i + 1..i + 3]`：
        // 合法的 `%XX` 必然落在字符边界上，但**畸形**的 `%`（后面紧跟一个多字节字符，
        // 例如对端吐出的 `%中`）会让切片指针落在 UTF-8 序列中间，那是直接 panic。
        // 这个解析器面对的是别人家的 WebDAV 响应，不能假设它格式正确。
        let escape = bytes[i] == b'%'
            && i + 2 < bytes.len()
            && bytes[i + 1].is_ascii_hexdigit()
            && bytes[i + 2].is_ascii_hexdigit();
        if escape {
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
    use chrono::TimeZone;

    /// 桩服务器故意混用两种前缀，解析必须都不漏
    const MIXED: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:" xmlns:s0="DAV:">
  <D:response><D:href>/dav/workbench-backup/</D:href>
    <D:propstat><D:prop><D:resourcetype><D:collection/></D:resourcetype></D:prop></D:propstat></D:response>
  <s0:response><s0:href>/dav/workbench-backup/anvil-20261028T220315Z-1a2b3c4d.abackup</s0:href>
    <s0:propstat><s0:prop><s0:getcontentlength>4096</s0:getcontentlength>
      <s0:getlastmodified>Wed, 28 Oct 2026 22:03:15 GMT</s0:getlastmodified>
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

    /// 「没给时间」与「时间很旧」必须是两个值：后者会让这份信封在"取最新 8 份"里
    /// 永远垫底，等于对一台还在正常同步的设备宣布它不存在。
    #[test]
    fn reads_last_modified_and_keeps_absent_separate_from_zero() {
        let entries = parse_multistatus(MIXED, "/dav/workbench-backup/").unwrap();
        let want = chrono::Utc
            .with_ymd_and_hms(2026, 10, 28, 22, 3, 15)
            .unwrap()
            .timestamp();
        assert_eq!(entries[0].modified, Some(want), "s0: 前缀那份也要读到");
        assert_eq!(entries[1].modified, None, "服务器没给这条属性时不是 0");
    }

    /// 数字时区那一种（`+0000`）也要接住：这一列只用来排序，认不出来就等于
    /// 放弃"按最近写过挑窗口"。
    #[test]
    fn accepts_the_gmt_literal_and_a_numeric_offset_alike() {
        let gmt = parse_http_date("Wed, 28 Oct 2026 22:03:15 GMT").unwrap();
        let numeric = parse_http_date("Wed, 28 Oct 2026 22:03:15 +0000").unwrap();
        assert_eq!(gmt, numeric);
        let plus2 = parse_http_date("Thu, 29 Oct 2026 00:03:15 +0200").unwrap();
        assert_eq!(gmt, plus2, "同一瞬间的不同写法要折成同一个秒数");
        assert_eq!(parse_http_date("2026-10-28T22:03:15Z"), None, "不是 HTTP 日期就当不知道");
    }

    #[test]
    fn an_unparseable_date_is_unknown_not_epoch_zero() {
        let xml = r#"<D:multistatus xmlns:D="DAV:"><D:response><D:href>/dav/x/a.enc</D:href>
    <D:propstat><D:prop><D:getlastmodified>昨天下午吧</D:getlastmodified><D:resourcetype/></D:prop></D:propstat></D:response></D:multistatus>"#;
        let entries = parse_multistatus(xml, "/dav/x/").unwrap();
        assert_eq!(entries[0].modified, None, "读不懂的日期原样当不知道：{entries:?}");
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

    /// 对端的 href 是别人写的。合法的 `%XX` 后两枚必是 ASCII，切点自然在边界上；
    /// 真正会 panic 的是**畸形**的 `%` —— 后面紧跟一个多字节字符时切片落在 UTF-8
    /// 序列中间。这一条防的是"别人家的 WebDAV 实现"，不是防自己。
    #[test]
    fn a_malformed_escape_stays_literal_instead_of_panicking() {
        assert_eq!(percent_decode("%E4%B8%AD"), "中");
        assert_eq!(percent_decode("%中"), "%中", "畸形转义原样留着");
        assert_eq!(percent_decode("%zz1"), "%zz1");
        assert_eq!(percent_decode("100%"), "100%");
        assert_eq!(percent_decode("%"), "%");
    }

    #[test]
    fn a_hostile_href_does_not_take_the_whole_listing_down() {
        let xml = r#"<D:multistatus xmlns:D="DAV:"><D:response><D:href>/dav/x/%中.abackup</D:href>
    <D:propstat><D:prop><D:getcontentlength>1</D:getcontentlength><D:resourcetype/></D:prop></D:propstat></D:response></D:multistatus>"#;
        let entries = parse_multistatus(xml, "/dav/x/").expect("畸形转义不该让整个清单读不出");
        assert_eq!(entries.len(), 1, "{entries:?}");
    }
}
