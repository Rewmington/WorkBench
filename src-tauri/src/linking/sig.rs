use sha2::{Digest, Sha256};

/// 内容指纹。用**归一化之后**的 JSON 算，所以仓库里改个缩进、调一下键序
/// 都不会被读成「内容变了」（否则会每天误报一次「网站版本较新」）。
pub fn content_sig(normalized_json: &str) -> String {
    let mut h = Sha256::new();
    h.update(normalized_json.as_bytes());
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whitespace_and_key_order_do_not_change_the_signature() {
        let a = content_sig(r#"{"count":1,"items":[{"name":"x","url":"y"}]}"#);
        let b = content_sig(r#"{"count":1,"items":[{"name":"x","url":"y"}]}"#);
        assert_eq!(a, b);

        let c = content_sig(r#"{"count":1,"items":[{"name":"x","url":"z"}]}"#);
        assert_ne!(a, c, "真改了内容要能看出来");
        assert_eq!(a.len(), 64);
    }
}
