fn main() {
    // tauri-build 会把图标编进 Windows 资源、tauri-macros 会把 128x128.png 当默认窗口图标嵌进去，
    // 但两边都没对 icons/ 声明 rerun-if-changed —— 实测过：换掉 icon.ico 后重新构建，
    // 从 exe 里抽出来的图标四角还是不透明白，也就是任务栏上还是旧图标而文件明明是新的。
    // 所以这里自己盯住图标目录。
    println!("cargo:rerun-if-changed=icons");
    tauri_build::build()
}
