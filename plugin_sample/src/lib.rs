// Wasm側でホスト関数をインポートするための宣言
#[link(wasm_import_module = "host")]
unsafe extern "C" {
    fn echo(ptr: *const u8, len: usize);
}

/// プラグインのエントリポイント。ホストから呼び出される。
#[unsafe(no_mangle)]
pub extern "C" fn init() {
    let message = "こんにちは、Wasmプラグインより！";
    unsafe {
        // ホストのecho関数を呼び出し、メッセージを渡す
        echo(message.as_ptr(), message.len());
    }
}
