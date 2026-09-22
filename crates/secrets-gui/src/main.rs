// 릴리스 빌드에서 콘솔 창이 함께 뜨지 않게 한다.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! Tauri 진입점. 명령을 등록하고 창을 띄우는 일만 한다.

mod command;
mod dto;
mod progress;
mod wiring;

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            command::tools::inspect,
            command::tools::install_tool,
            command::accounts::list_accounts,
            command::accounts::create_account,
            command::accounts::verify_account,
            command::accounts::replace_credential,
            command::login::provider_form,
            command::login::probe_credentials,
            command::login::probe_browser,
            command::login::discard_preparation,
            command::login::begin_browser_login,
            command::login::complete_browser_login,
            command::login::open_url,
            command::switching::activate_account,
            command::switching::deactivate_provider,
            command::switching::archive_account
        ])
        .run(tauri::generate_context!())
        .expect("Tauri 앱 실행 실패");
}
