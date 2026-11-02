use crate::error::Error;

/// 前端拦掉默认 contextmenu 后只传「点的是哪一类东西」，菜单本身是 Win32 原生件。
#[tauri::command]
pub fn context_menu(window: tauri::Window, target: String) -> Result<(), Error> {
    crate::ui::context::popup(window, crate::ui::context::MenuKind::parse(&target))
}

/// 只放行"能安全交给系统浏览器"的地址。
///
/// 前端的 `SAFE_URI` 还允许 `#` / `/` / `./` / `../`，那是给 Markdown 渲染看的，
/// 不是可以开窗口的地址 —— 尤其 `//host/share` 会被 `ShellExecute` 当成 UNC 路径，
/// Windows 会拿当前凭据去访问它。所以这道判断必须在 Rust 侧再来一次，
/// 不能只信前端（前端那道是给人看的提示，不是边界）。
fn browser_address(target: &str) -> Result<&str, Error> {
    const ALLOWED: [&str; 4] = ["http://", "https://", "mailto:", "ftp://"];
    let t = target.trim();
    let ok = ALLOWED
        .iter()
        .any(|p| t.len() >= p.len() && t.as_bytes()[..p.len()].eq_ignore_ascii_case(p.as_bytes()));
    if ok {
        Ok(t)
    } else {
        Err(Error::msg("这个地址我不适合交给系统浏览器打开"))
    }
}

/// 用系统默认程序打开 URL / 路径。走 Rust 侧 ShellExecute 而不是前端的 window.open：
/// 后者在 webview 里会开出一个新的应用窗口，那是最露馅的「像网页」行为之一。
#[tauri::command]
pub fn open_url(target: String) -> Result<(), Error> {
    crate::win32::launch::open_shell(browser_address(&target)?)
}

/// 托盘菜单里那条「打开数据文件夹」的命令入口：命令面板走 JS，够不到 `ui/menus.rs`，
/// 所以这里转调同一个 handler，两个入口共用同一套行为（不是"每项能力都要有命令"，
/// 见 `src/composables/useActions.ts` 上那段边界说明）。
#[tauri::command]
pub fn open_data_dir(app: tauri::AppHandle) {
    crate::ui::menus::handle(&app, crate::ui::menus::ID_OPEN_DIR);
}

#[cfg(test)]
mod tests {
    use super::browser_address;

    fn ok(t: &str) {
        assert!(browser_address(t).is_ok(), "{t} 该放行");
    }
    fn no(t: &str) {
        assert!(browser_address(t).is_err(), "{t} 不该被交给系统浏览器");
    }

    #[test]
    fn web_and_mail_addresses_pass() {
        ok("https://example.com/a?b=1");
        ok("http://example.com");
        ok("mailto:someone@example.com");
        ok("ftp://example.com/pub");
        // 大小写与前后空白不改变判断
        ok("  HTTPS://Example.com ");
    }

    #[test]
    fn unc_and_smb_shapes_are_refused() {
        // 这两条会被 ShellExecute 当成本机路径，Windows 会拿当前凭据去访问
        no("//host/share");
        no("\\\\host\\share");
        no("file:///C:/Windows/system32");
        no("javascript:alert(1)");
        no("#section-in-page");
        no("./relative.md");
        no("相对路径不是地址");
        no("");
    }

    #[test]
    fn a_multibyte_early_char_does_not_panic_the_prefix_check() {
        // 前缀是按字节切的；若切点落在多字节字符中间就该 panic，这里要求它只是被拒
        no("h中文中文中文中文");
        no("中https://example.com");
    }
}
