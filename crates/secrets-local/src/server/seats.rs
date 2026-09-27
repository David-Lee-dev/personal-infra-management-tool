//! 프로젝트가 고르고 쓰는 서버 계정 — 등록된 서버 기록에서 읽는다.

use secrets_core::project::{ServerSeat, ServerSeats};
use secrets_core::server::{AccountState, Role, ServerStore};

use super::{FileServers, VaultKeys};

pub struct RegisteredSeats;

impl ServerSeats for RegisteredSeats {
    fn seats(&self) -> Vec<ServerSeat> {
        let keys = VaultKeys;
        FileServers::standard()
            .list()
            .into_iter()
            .filter_map(Result::ok)
            .flat_map(|server| {
                server
                    .accounts
                    .iter()
                    // 풀 수 없는 키(AWS 사실이 없는 pem)는 등록 · 편집에서 막는다. 그래도 있으면
                    // 고를 수 없게 뺀다 — `None` 으로 두면 ssh 기본 키로 오해된다.
                    .filter_map(|account| {
                        let key = keys.path_of(&server, &account.key).ok()?;
                        Some(ServerSeat {
                            server: server.id.clone(),
                            server_name: server.name.clone(),
                            kind: server.kind.id().to_string(),
                            address: server.address.clone(),
                            port: server.port,
                            login: account.login.clone(),
                            admin: account.role == Role::Admin,
                            verified: account.state == AccountState::Verified,
                            key: key.map(|p| p.display().to_string()),
                        })
                    })
                    .collect::<Vec<_>>()
            })
            .collect()
    }
}
