// 릴리스 빌드에서 콘솔 창이 함께 뜨지 않게 한다.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! Tauri 진입점. 명령을 등록하고 창을 띄우는 일만 한다.

mod command;
mod dto;
mod progress;
mod wiring;

fn main() {
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
            command::hosts::list_instance_accounts,
            command::hosts::inspect_instance,
            command::hosts::prepare_instance,
            command::hosts::create_instance_accounts,
            command::hosts::connect_instance_account,
            command::hosts::reinstall_instance_account,
            command::hosts::remove_instance_account,
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
            command::projects::connect_git
        ])
        .run(tauri::generate_context!())
        .expect("Tauri 앱 실행 실패");
}
