// FIX: Remove direct import of alloc/dealloc to avoid name collision
use std::alloc::{Layout};

// Host functions imported from the editor
#[link(wasm_import_module = "host")]
unsafe extern "C" {
    fn apply_text_style(line: u32, start_byte: u32, end_byte: u32, style_id: u32);
}

// Style IDs must match the host's SyntaxStyle enum
const STYLE_KEYWORD: u32 = 1;

const C_KEYWORDS: &[&str] = &[
    "auto", "break", "case", "char", "const", "continue", "default", "do", "double",
    "else", "enum", "extern", "float", "for", "goto", "if", "int", "long", "register",
    "return", "short", "signed", "sizeof", "static", "struct", "switch", "typedef",
    "union", "unsigned", "void", "volatile", "while",
];

/// Entry point called by the host to analyze and highlight a line of text.
#[unsafe(no_mangle)]
pub extern "C" fn highlight_line(line_idx: u32, content_ptr: *const u8, content_len: usize) {
    let content = unsafe {
        let slice = std::slice::from_raw_parts(content_ptr, content_len);
        std::str::from_utf8(slice).unwrap_or("")
    };

    if content.is_empty() {
        return;
    }

    // Simple tokenization by whitespace and punctuation
    let mut last_end = 0;
    for (start, part) in content.match_indices(|c: char| c.is_ascii_punctuation() || c.is_whitespace()) {
        let word = &content[last_end..start];
        if C_KEYWORDS.contains(&word) {
            unsafe {
                apply_text_style(line_idx, last_end as u32, start as u32, STYLE_KEYWORD);
            }
        }
        last_end = start + part.len();
    }
    // Check the last word
    let word = &content[last_end..];
    if C_KEYWORDS.contains(&word) {
         unsafe {
            apply_text_style(line_idx, last_end as u32, content.len() as u32, STYLE_KEYWORD);
        }
    }
}

/// Wasm memory allocation function, required by the host to pass strings.
#[unsafe(no_mangle)]
pub extern "C" fn alloc(size: i32) -> *mut u8 {
    let layout = Layout::from_size_align(size as usize, 1).unwrap();
    // FIX: Call the system allocator using its full path
    unsafe { std::alloc::alloc(layout) }
}

/// Wasm memory deallocation function (optional but good practice).
#[unsafe(no_mangle)]
pub extern "C" fn dealloc_str(ptr: *mut u8, size: i32) {
    let layout = Layout::from_size_align(size as usize, 1).unwrap();
    // FIX: Call the system deallocator using its full path
    unsafe { std::alloc::dealloc(ptr, layout) };
}

