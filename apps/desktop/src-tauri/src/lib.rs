#[tauri::command]
fn healthcheck() -> &'static str {
    library_core::product_codename()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![healthcheck])
        .run(tauri::generate_context!())
        .expect("failed to run Darkwave desktop shell");
}

#[cfg(test)]
mod tests {
    #[test]
    fn healthcheck_returns_product_codename() {
        assert_eq!(super::healthcheck(), "Darkwave");
    }
}
