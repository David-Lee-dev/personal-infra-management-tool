// 릴리스 빌드에서 콘솔 창이 함께 뜨지 않게 한다.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! Tauri 진입점. 명령을 등록하고 창을 띄우는 일만 한다.

mod command;
mod dto;
mod progress;
mod wiring;

/// Finder · Dock 에서 띄운 앱은 PATH 가 `/usr/bin:/bin:/usr/sbin:/sbin` 뿐이라 Homebrew 등으로 깐
/// 도구(aws · gh …)를 찾지 못한다. 로그인 셸이 쓰는 PATH 를 물어 이어받는다. 묻지 못하면 흔한 자리를 붙인다.
fn inherit_login_path() {
    let current = std::env::var("PATH").unwrap_or_default();
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".into());
    let asked = std::process::Command::new(&shell)
        .args(["-l", "-c", "printf %s \"$PATH\""])
        .stdin(std::process::Stdio::null())
        .output()
        .ok()
        .filter(|out| out.status.success())
        .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_string())
        .filter(|path| !path.is_empty());
    let home = std::env::var("HOME").unwrap_or_default();
    let fallback = ["/opt/homebrew/bin", "/opt/homebrew/sbin", "/usr/local/bin", &format!("{home}/.cargo/bin")].join(":");
    let mut parts: Vec<String> = Vec::new();
    for dir in [asked.unwrap_or(fallback), current].join(":").split(':') {
        if !dir.is_empty() && !parts.iter().any(|p| p == dir) {
            parts.push(dir.to_string());
        }
    }
    // SAFETY: 다른 스레드가 생기기 전, main 의 맨 처음에 한 번만 바꾼다.
    unsafe { std::env::set_var("PATH", parts.join(":")) };
}

fn main() {
    inherit_login_path();
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
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
            command::switching::archive_account,
            command::keys::list_keys,
            command::keys::resolve_repo,
            command::keys::create_deploy_key,
            command::keys::retry_registration,
            command::keys::rotate_key,
            command::keys::set_purpose,
            command::keys::remove_key,
            command::keys::scan_unowned,
            command::keys::reveal_private_key,
            command::keys::ssh_hosts,
            command::aws::list_aws_keys,
            command::aws::check_private_key,
            command::aws::adopt_key_pair,
            command::etc::list_etc,
            command::etc::set_etc_purpose,
            command::etc::add_etc_consumer,
            command::etc::remove_etc_consumer,
            command::etc::etc_value,
            command::etc::etc_key_properties,
            command::iam::list_iam,
            command::iam::preview_iam,
            command::iam::create_iam,
            command::iam::add_iam_consumer,
            command::iam::remove_iam_consumer,
            command::iam::iam_env_lines,
            command::iam::remove_iam,
            command::iam::set_iam_purpose,
            command::iam::iam_last_used,
            command::iam::adoptable_iam,
            command::iam::adopt_iam,
            command::iam::mark_iam_cleanup,
            command::iam::unmark_iam_cleanup,
            command::projects::list_projects,
            command::projects::project_detail,
            command::projects::inspect_project_path,
            command::projects::create_project,
            command::projects::register_project,
            command::projects::pick_project_folder,
            command::projects::git_plan,
            command::projects::ignore_env_files,
            command::projects::connect_git,
            command::projects::repo_keys,
            command::projects::server_plan,
            command::projects::attach_server,
            command::projects::check_environment,
            command::projects::pull_plan,
            command::projects::pull_code,
            command::projects::choose_env_file,
            command::projects::compare_env,
            command::projects::push_env,
            command::projects::deploy_script,
            command::projects::save_deploy_script,
            command::projects::deploy_plan,
            command::projects::run_deploy,
            command::projects::update_project,
            command::projects::unregister_project,
            command::projects::update_environment,
            command::projects::remove_environment,
            command::projects::project_credentials,
            command::projects::credential_usage,
            command::servers::list_servers,
            command::servers::server_detail,
            command::servers::server_suggestions,
            command::servers::known_server_accounts,
            command::servers::register_server,
            command::servers::adopt_servers,
            command::servers::update_server,
            command::servers::unregister_server,
            command::servers::add_server_account,
            command::servers::edit_server_account,
            command::servers::forget_server_account,
            command::servers::pick_key_file,
            command::servers::connect_server_account,
            command::servers::server_ssh_command,
            command::servers::check_server_account,
            command::servers::inspect_server,
            command::servers::prepare_server,
            command::servers::create_server_accounts,
            command::servers::reinstall_server_account,
            command::servers::remove_server_account
        ])
        .run(tauri::generate_context!())
        .expect("Tauri 앱 실행 실패");
}
