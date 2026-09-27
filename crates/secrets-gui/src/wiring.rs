//! 이 머신에 붙는 배선. 어떤 구현을 쓸지는 여기서만 고른다.

use secrets_core::aws::AwsGateway;
use secrets_core::aws::iam::Issuer;
use secrets_core::enrollment::Enrollment;
use secrets_core::etc::EtcBook;
use secrets_core::key::Keyring;
use secrets_core::project::{
    CodePull, Deployer, Deployment, EnvSync, GitLink, ProjectEditor, Projects, ServerLink,
};
use secrets_core::server::{AccountProvisioning, Servers};
use secrets_local::adapter::{accounts::CliAccounts, clock::SystemClock, registry::FileRegistry};
use secrets_local::aws::CliAws;
use secrets_local::etc::FileEtc;
use secrets_local::hosts::SshHosts;
use secrets_local::iam::{CliIam, FileIam};
use secrets_local::keys::{FileKeys, GhKeys};
use secrets_local::project::{
    FileDeployScripts, FileProjects, GhRepos, LocalEnv, LocalGit, LocalWorkspace, SshCode,
    SshDeploy, SshEnv, SshProbe, VaultRepoKeys,
};
use secrets_local::server::{FileServers, RegisteredSeats, VaultKeys};

pub struct Wiring {
    gateway: CliAccounts,
    registry: FileRegistry,
    keys: GhKeys,
    cloud: CliAws,
    remote: SshHosts,
    vault: FileKeys,
    iam: CliIam,
    iam_vault: FileIam,
    etc: FileEtc,
    project_store: FileProjects,
    workspace: LocalWorkspace,
    servers: FileServers,
    pub clock: SystemClock,
}

impl Wiring {
    pub fn get() -> &'static Wiring {
        static WIRING: std::sync::OnceLock<Wiring> = std::sync::OnceLock::new();
        WIRING.get_or_init(|| {
            // 게이트웨이와 레지스트리가 같은 보관소를 공유한다. 확인된 자격의
            // 자리를 그대로 계정에게 넘기기 위한 것이다.
            let store = std::sync::Arc::new(secrets_local::adapter::PreparationStore::new());
            Wiring {
                gateway: CliAccounts::new(store.clone()),
                registry: FileRegistry::new(store),
                keys: GhKeys,
                cloud: CliAws,
                remote: SshHosts,
                vault: FileKeys,
                iam: CliIam,
                iam_vault: FileIam,
                etc: FileEtc,
                project_store: FileProjects,
                workspace: LocalWorkspace,
                servers: FileServers::standard(),
                clock: SystemClock,
            }
        })
    }

    pub fn enrollment(&self) -> Enrollment<'_> {
        Enrollment::new(&self.gateway, &self.registry, &self.clock)
    }

    pub fn aws(&self) -> &dyn AwsGateway {
        &self.cloud
    }

    pub fn servers(&self) -> Servers<'_> {
        Servers::new(&self.servers, &VaultKeys, &self.clock)
    }

    /// 서버에 들어가 계정을 만들고 걷어내는 일.
    pub fn provisioning(&self) -> AccountProvisioning<'_> {
        AccountProvisioning::new(&self.servers, &VaultKeys, &self.remote, &self.clock)
    }

    pub fn server_store(&self) -> &FileServers {
        &self.servers
    }

    pub fn issuer(&self) -> Issuer<'_> {
        Issuer::new(&self.iam, &self.iam_vault, &self.clock)
    }

    pub fn etc_book(&self) -> EtcBook<'_> {
        EtcBook::new(&self.etc, &self.clock)
    }

    pub fn etc_vault(&self) -> &FileEtc {
        &self.etc
    }

    pub fn keyring(&self) -> Keyring<'_> {
        Keyring::new(&self.keys, &self.vault, &self.clock)
    }

    pub fn projects(&self) -> Projects<'_> {
        Projects::new(&self.project_store, &self.workspace, &self.clock)
    }

    pub fn git_link(&self) -> GitLink<'_> {
        GitLink::new(
            &self.project_store,
            &self.workspace,
            &LocalGit,
            &VaultRepoKeys,
            &GhRepos,
        )
    }

    pub fn server_link(&self) -> ServerLink<'_> {
        ServerLink::new(
            &self.project_store,
            &self.workspace,
            &RegisteredSeats,
            &SshProbe,
            &self.clock,
        )
    }

    pub fn code_pull(&self) -> CodePull<'_> {
        CodePull::new(
            &self.project_store,
            &self.workspace,
            &RegisteredSeats,
            &SshProbe,
            &VaultRepoKeys,
            &SshCode,
        )
    }

    pub fn env_sync(&self) -> EnvSync<'_> {
        EnvSync::new(
            &self.project_store,
            &self.workspace,
            &RegisteredSeats,
            &LocalEnv,
            &SshEnv,
        )
    }

    pub fn deployment(&self) -> Deployment<'_> {
        Deployment::new(&self.project_store, &FileDeployScripts)
    }

    /// 배포는 환경 변수 비교를 함께 쓴다. `env` 는 부르는 쪽이 `env_sync()` 로 만들어 넘긴다.
    pub fn deployer<'a>(&'a self, env: &'a EnvSync<'a>) -> Deployer<'a> {
        Deployer::new(
            &self.project_store,
            &self.workspace,
            &RegisteredSeats,
            &SshProbe,
            &LocalGit,
            env,
            &FileDeployScripts,
            &SshDeploy,
        )
    }

    /// 등록한 뒤의 수정과 제거. 기록 디렉토리를 옮기는 일도 기록 저장소가 한다.
    pub fn project_editor(&self) -> ProjectEditor<'_> {
        ProjectEditor::new(
            &self.project_store,
            &self.project_store,
            &self.workspace,
            &RegisteredSeats,
            &SshProbe,
        )
    }

    pub fn project_store(&self) -> &FileProjects {
        &self.project_store
    }
}
