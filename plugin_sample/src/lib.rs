// Wasmにコンパイルするためのプラグインコード (src/lib.rs)

#![no_std]
use core::panic::PanicInfo;
use core::str;
// ★追加: 生ポインタを取得するためのマクロをインポート
use core::ptr::addr_of_mut;

// --- API Imports (変更なし) ---
#[link(wasm_import_module = "host")]
unsafe extern "C" {
    fn echo(ptr: i32, len: i32);
    fn get_buffer_line_len(line_num: i32) -> i32;
    fn get_buffer_line_data(line_num: i32, ptr: i32, len: i32) -> i32;
}

// --- Static Buffer (変更なし) ---
const MAX_BUFFER_SIZE: usize = 1024;
static mut SHARED_BUFFER: [u8; MAX_BUFFER_SIZE] = [0; MAX_BUFFER_SIZE];

// --- 安全なラッパー関数 ---

/// ホストから指定された行を文字列として取得するヘルパー関数。
/// 内部でグローバルな`SHARED_BUFFER`を使用します。
fn get_line(line_num: i32) -> Result<&'static str, &'static str> {
    unsafe {
        // 1. これから取得する行の長さをホストに問い合わせる
        let len = get_buffer_line_len(line_num);
        if len < 0 {
            return Err("API Error: Could not get line length.");
        }
        if len as usize > MAX_BUFFER_SIZE {
            return Err("Error: Line is too long to fit in the buffer.");
        }
        if len == 0 {
            return Ok(""); // 空行の場合は空の文字列を返す
        }

        // 2. 安全なサイズで実際の行データをバッファに取得する
        // ★修正点: addr_of_mut! を使って生ポインタを直接取得する
        let written_len = get_buffer_line_data(line_num, addr_of_mut!(SHARED_BUFFER) as i32, len);
        if written_len < 0 {
            return Err("API Error: Could not get line data.");
        }

        // 3. バッファの内容をUTF-8文字列スライスに変換して返す
        let line_slice = &SHARED_BUFFER[..written_len as usize];
        str::from_utf8(line_slice).map_err(|_| "Error: Line contains invalid UTF-8.")
    }
}

/// `echo` APIを安全に呼び出すヘルパー関数。
fn show_message(message: &str) {
    unsafe {
        echo(message.as_ptr() as i32, message.len() as i32);
    }
}

// --- ヘルパー関数を使ってシンプルになったinit関数 ---
#[unsafe(no_mangle)]
pub extern "C" fn init() {
    // 安全なヘルパー関数を使って0行目を取得する
    match get_line(0) {
        Ok(line_content) => {
            // 表示したいメッセージを組み立てる
            let prefix = "Line 0: ";
            let prefix_len = prefix.len();
            let content_len = line_content.len();

            // メッセージがバッファに収まるかチェック
            if prefix_len + content_len > MAX_BUFFER_SIZE {
                show_message("Error: Combined message is too long.");
                return;
            }

            // `SHARED_BUFFER`を再利用して、表示用の文字列を組み立てる
            unsafe {
                // ★修正点: `static mut`へのアクセスを安全なスライス経由で行う
                let buffer = &mut *addr_of_mut!(SHARED_BUFFER);

                // 1. プレフィックスをバッファの先頭にコピー
                buffer[..prefix_len].copy_from_slice(prefix.as_bytes());
                // 2. 行の内容をプレフィックスの直後にコピー
                buffer[prefix_len..prefix_len + content_len].copy_from_slice(line_content.as_bytes());
                
                // 3. 組み立てた文字列全体を一度に表示する
                let combined_slice = &buffer[..prefix_len + content_len];
                let final_message = str::from_utf8(combined_slice).unwrap_or("UTF-8 Error");
                show_message(final_message);
            }
        }
        Err(error_message) => {
            // エラーが発生した場合は、その内容を表示
            show_message(error_message);
        }
    }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {}
}

