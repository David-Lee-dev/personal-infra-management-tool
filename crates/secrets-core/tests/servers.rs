//! 서버 기록 — 등록 · 편집 · 해제 · 계정 기록. 서버에는 닿지 않는다.

use std::collections::BTreeMap;
use std::sync::Mutex;

use secrets_core::port::Clock;
use secrets_core::server::{
    AccountChoice, AccountKey, AccountKeys, AccountOrigin, AccountState, AwsFacts, CreatedKey,
    NewAccount, NewServer, Role, Server, ServerEdit, ServerError, ServerKind, ServerStore, Servers,
};

struct Frozen;

impl Clock for Frozen {
    fn now(&self) -> String {
        "2026-09-27T12:00:00+09:00".into()
    }
    fn today(&self) -> String {
        "2026-09-27".into()
    }
}

#[derive(Default)]
struct Memory {
    servers: Mutex<BTreeMap<String, Server>>,
    archived: Mutex<Vec<String>>,
}

impl ServerStore for Memory {
    fn list(&self) -> Vec<Result<Server, String>> {
        self.servers
            .lock()
            .unwrap()
            .values()
            .cloned()
            .map(Ok)
            .collect()
    }
    fn load(&self, id: &str) -> Result<Server, ServerError> {
        self.servers
            .lock()
            .unwrap()
            .get(id)
            .cloned()
            .ok_or_else(|| ServerError::Missing(id.into()))
    }
    fn insert(&self, server: &Server) -> Result<(), ServerError> {
        let mut all = self.servers.lock().unwrap();
        if all.contains_key(&server.id) {
            return Err(ServerError::Taken(server.id.clone()));
        }
        all.insert(server.id.clone(), server.clone());
        Ok(())
    }
    fn replace(&self, server: &Server) -> Result<(), ServerError> {
        let mut all = self.servers.lock().unwrap();
        if !all.contains_key(&server.id) {
            return Err(ServerError::Missing(server.id.clone()));
        }
        all.insert(server.id.clone(), server.clone());
        Ok(())
    }
    fn archive(&self, id: &str) -> Result<(), ServerError> {
        self.servers
            .lock()
            .unwrap()
            .remove(id)
            .ok_or_else(|| ServerError::Missing(id.into()))?;
        self.archived.lock().unwrap().push(id.into());
        Ok(())
    }
}

#[derive(Default)]
struct Keys {
    imported: Mutex<Vec<String>>,
}

impl AccountKeys for Keys {
    fn create(&self, _: &str, _: &str, _: &str) -> Result<CreatedKey, ServerError> {
        unreachable!("기록에서는 키를 만들지 않는다")
    }
    fn import(&self, server: &str, login: &str, source: &str) -> Result<AccountKey, ServerError> {
        self.imported.lock().unwrap().push(source.into());
        Ok(AccountKey::Vault {
            path: format!("keys/server/{server}/{login}/key"),
        })
    }
    fn public_key(&self, _: &Server, _: &AccountKey) -> Result<String, ServerError> {
        unreachable!()
    }
    fn private_path(&self, _: &Server, _: &AccountKey) -> Result<Option<String>, ServerError> {
        unreachable!()
    }
    fn discard(&self, _: &AccountKey) {}
    fn archive(&self, _: &str, _: &AccountKey) -> Result<(), ServerError> {
        Ok(())
    }
}

fn account(login: &str, role: Role, key: AccountChoice) -> NewAccount {
    NewAccount {
        login: login.into(),
        role,
        purpose: String::new(),
        key,
    }
}

fn local(name: &str, address: &str) -> NewServer {
    NewServer {
        name: name.into(),
        group: String::new(),
        address: address.into(),
        port: 22,
        kind: ServerKind::Other,
        aws: None,
        workspace: "/srv".into(),
        workspace_group: "workspace".into(),
        note: String::new(),
        account: account(
            "david",
            Role::User,
            AccountChoice::File {
                path: "~/.ssh/nemo-mac".into(),
            },
        ),
        admin_access: false,
    }
}

fn ec2(name: &str, address: &str) -> NewServer {
    NewServer {
        kind: ServerKind::Ec2,
        aws: Some(AwsFacts {
            account: "320042238085".into(),
            region: "ap-northeast-2".into(),
            instance: "i-0b97".into(),
        }),
        account: account(
            "ubuntu",
            Role::Admin,
            AccountChoice::Pem {
                keypair: "tuk-key".into(),
            },
        ),
        admin_access: true,
        ..local(name, address)
    }
}

struct Fixture {
    store: Memory,
    keys: Keys,
}

impl Fixture {
    fn new() -> Fixture {
        Fixture {
            store: Memory::default(),
            keys: Keys::default(),
        }
    }

    fn servers(&self) -> Servers<'_> {
        Servers::new(&self.store, &self.keys, &Frozen)
    }
}

fn edit_of(server: &Server) -> ServerEdit {
    ServerEdit {
        name: server.name.clone(),
        group: server.group.clone(),
        address: server.address.clone(),
        port: server.port,
        kind: server.kind,
        aws: server.aws.clone(),
        admin: server.admin.clone(),
        workspace: server.workspace.clone(),
        workspace_group: server.workspace_group.clone(),
        note: server.note.clone(),
    }
}

mod register {
    use super::*;

    #[test]
    fn a_local_server_is_recorded_with_its_first_account_pointing_at_the_key_file() {
        let f = Fixture::new();
        let server = f
            .servers()
            .register(&local("nemo-mac", "100.115.77.18"))
            .unwrap();

        assert_eq!(server.id, "nemo-mac");
        assert_eq!(server.admin, None);
        let david = server.account("david").unwrap();
        assert_eq!(
            david.key,
            AccountKey::File {
                path: "~/.ssh/nemo-mac".into()
            }
        );
        assert_eq!(david.origin, AccountOrigin::Registered);
        assert_eq!(david.state, AccountState::Unverified);
        assert_eq!(f.store.load("nemo-mac").unwrap(), server);
    }

    #[test]
    fn an_aws_server_with_a_pem_admin_becomes_its_own_admin_access() {
        let f = Fixture::new();
        let server = f
            .servers()
            .register(&ec2("tuk-api-server", "54.116.119.214"))
            .unwrap();
        assert_eq!(server.admin.as_deref(), Some("ubuntu"));
        assert_eq!(
            server.account("ubuntu").unwrap().key,
            AccountKey::Pem {
                keypair: "tuk-key".into()
            }
        );
    }

    #[test]
    fn a_pem_key_on_a_server_outside_aws_is_refused() {
        let f = Fixture::new();
        let request = NewServer {
            kind: ServerKind::Other,
            ..ec2("vpn", "54.116.177.2")
        };
        assert!(matches!(
            f.servers().register(&request),
            Err(ServerError::Invalid(_))
        ));
    }

    #[test]
    fn only_an_admin_account_can_be_the_admin_access() {
        let f = Fixture::new();
        let request = NewServer {
            admin_access: true,
            ..local("nemo", "nemo.ts.net")
        };
        assert!(matches!(
            f.servers().register(&request),
            Err(ServerError::Invalid(_))
        ));
    }

    #[test]
    fn the_same_name_or_the_same_address_and_port_is_taken() {
        let f = Fixture::new();
        f.servers().register(&local("nemo", "nemo.ts.net")).unwrap();

        assert!(matches!(
            f.servers().register(&local("nemo", "10.0.0.9")),
            Err(ServerError::Taken(_))
        ));
        assert!(matches!(
            f.servers().register(&local("nemo-2", "nemo.ts.net")),
            Err(ServerError::Taken(_))
        ));
        let other_port = NewServer {
            port: 2222,
            ..local("nemo-ssh2", "nemo.ts.net")
        };
        assert!(f.servers().register(&other_port).is_ok());
    }

    #[test]
    fn an_imported_key_is_copied_only_after_every_other_check_passed() {
        let f = Fixture::new();
        f.servers().register(&local("nemo", "nemo.ts.net")).unwrap();
        let clash = NewServer {
            account: account(
                "nemo",
                Role::User,
                AccountChoice::Import {
                    path: "/Users/d/.ssh/id".into(),
                },
            ),
            ..local("nemo", "other.ts.net")
        };

        assert!(f.servers().register(&clash).is_err());
        assert!(f.keys.imported.lock().unwrap().is_empty());
    }

    #[test]
    fn an_address_a_shell_would_read_differently_is_refused() {
        let f = Fixture::new();
        assert!(
            f.servers()
                .register(&local("x", "-oProxyCommand=id"))
                .is_err()
        );
    }
}

mod edit {
    use super::*;

    #[test]
    fn a_new_address_is_kept_in_the_one_record_every_account_reads() {
        let f = Fixture::new();
        let server = f
            .servers()
            .register(&ec2("tuk-api-server", "54.116.119.214"))
            .unwrap();

        let moved = f
            .servers()
            .edit(
                &server.id,
                &ServerEdit {
                    address: "3.3.3.3".into(),
                    ..edit_of(&server)
                },
            )
            .unwrap();

        assert_eq!(moved.address, "3.3.3.3");
        assert_eq!(moved.id, server.id, "id 는 바뀌지 않는다");
        assert_eq!(moved.accounts, server.accounts);
    }

    #[test]
    fn the_admin_access_must_be_an_admin_account_of_that_server() {
        let f = Fixture::new();
        let server = f.servers().register(&local("nemo", "nemo.ts.net")).unwrap();
        let to = |login: &str| ServerEdit {
            admin: Some(login.into()),
            ..edit_of(&server)
        };
        assert!(matches!(
            f.servers().edit(&server.id, &to("david")),
            Err(ServerError::Invalid(_))
        ));
        assert!(matches!(
            f.servers().edit(&server.id, &to("ghost")),
            Err(ServerError::Missing(_))
        ));
    }

    #[test]
    fn a_server_with_pem_accounts_cannot_move_outside_aws() {
        let f = Fixture::new();
        let server = f.servers().register(&ec2("vpn", "54.116.177.2")).unwrap();
        let outside = ServerEdit {
            kind: ServerKind::Other,
            ..edit_of(&server)
        };
        assert!(f.servers().edit(&server.id, &outside).is_err());
    }
}

mod unregister {
    use super::*;

    #[test]
    fn a_server_that_an_environment_uses_stays() {
        let f = Fixture::new();
        let server = f.servers().register(&local("nemo", "nemo.ts.net")).unwrap();

        let used = vec!["nemo-marketing-server/prod".to_string()];
        assert!(f.servers().unregister(&server.id, &used).is_err());
        assert!(f.store.load(&server.id).is_ok());

        f.servers().unregister(&server.id, &[]).unwrap();
        assert!(f.store.load(&server.id).is_err());
        assert_eq!(*f.store.archived.lock().unwrap(), vec![server.id]);
    }
}

mod accounts {
    use super::*;

    #[test]
    fn an_existing_account_is_recorded_without_touching_the_server() {
        let f = Fixture::new();
        let server = f.servers().register(&local("nemo", "nemo.ts.net")).unwrap();

        let server = f
            .servers()
            .add_account(
                &server.id,
                &account("infra", Role::User, AccountChoice::Agent),
            )
            .unwrap();

        assert_eq!(server.account("infra").unwrap().key, AccountKey::Agent);
        assert!(matches!(
            f.servers().add_account(
                &server.id,
                &account("infra", Role::User, AccountChoice::Agent)
            ),
            Err(ServerError::Taken(_))
        ));
    }

    #[test]
    fn forgetting_the_admin_access_clears_it_and_a_used_account_stays() {
        let f = Fixture::new();
        let server = f.servers().register(&ec2("vpn", "54.116.177.2")).unwrap();

        assert!(
            f.servers()
                .forget_account(&server.id, "ubuntu", &["x/prod".into()])
                .is_err()
        );
        let server = f
            .servers()
            .forget_account(&server.id, "ubuntu", &[])
            .unwrap();
        assert!(server.accounts.is_empty());
        assert_eq!(server.admin, None);
    }

    #[test]
    fn an_account_this_tool_installed_is_not_just_forgotten() {
        let f = Fixture::new();
        let mut server = f.servers().register(&local("nemo", "nemo.ts.net")).unwrap();
        server.accounts[0].origin = AccountOrigin::Created;
        f.store.replace(&server).unwrap();

        assert!(matches!(
            f.servers().forget_account(&server.id, "david", &[]),
            Err(ServerError::Invalid(_))
        ));
    }

    #[test]
    fn the_admin_access_account_cannot_be_demoted() {
        let f = Fixture::new();
        let server = f.servers().register(&ec2("vpn", "54.116.177.2")).unwrap();
        assert!(
            f.servers()
                .edit_account(&server.id, "ubuntu", Role::User, "")
                .is_err()
        );
        let server = f
            .servers()
            .edit_account(&server.id, "ubuntu", Role::Admin, " 비상용 ")
            .unwrap();
        assert_eq!(server.account("ubuntu").unwrap().purpose, "비상용");
    }
}

#[test]
fn listing_puts_grouped_servers_first_then_by_name() {
    let f = Fixture::new();
    for (name, address, group) in [
        ("vpn", "1.1.1.1", ""),
        ("b", "2.2.2.2", "툭"),
        ("a", "3.3.3.3", "툭"),
    ] {
        f.servers()
            .register(&NewServer {
                group: group.into(),
                ..local(name, address)
            })
            .unwrap();
    }
    let (servers, errors) = f.servers().list();
    let names: Vec<&str> = servers.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, vec!["a", "b", "vpn"]);
    assert!(errors.is_empty());
}
