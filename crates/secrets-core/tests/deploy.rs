//! 배포 스크립트 — 있는 환경에만, 비어 있지 않은 것만, 바뀐 것만 쓴다.

use std::sync::Mutex;

use secrets_core::project::{
    DeployScripts, Deployment, Environment, Origin, ProjectError, ProjectRecord, ProjectStore,
};

struct Store;

impl ProjectStore for Store {
    fn list(&self) -> Vec<Result<ProjectRecord, String>> {
        vec![Ok(self.load("api").unwrap())]
    }
    fn load(&self, name: &str) -> Result<ProjectRecord, ProjectError> {
        if name != "api" {
            return Err(ProjectError::Missing(name.into()));
        }
        Ok(ProjectRecord {
            name: "api".into(),
            group: "tuk".into(),
            path: "/w/api".into(),
            origin: Origin::Registered,
            created_at: "t".into(),
            environments: vec![Environment {
                name: "prod".into(),
                aws_account: "1".into(),
                machine: "ec2".into(),
                region: "r".into(),
                keypair: "k".into(),
                instance: "i-1".into(),
                instance_name: "web".into(),
                address: "3.3.3.3".into(),
                login: "app".into(),
                path: "/srv/api".into(),
                branch: "main".into(),
                connected_at: "t".into(),
                env_file: None,
                server_env_file: ".env".into(),
            }],
        })
    }
    fn insert(&self, _: &ProjectRecord) -> Result<(), ProjectError> {
        unreachable!()
    }
    fn replace(&self, _: &ProjectRecord) -> Result<(), ProjectError> {
        unreachable!("배포 스크립트는 프로젝트 기록을 고치지 않는다")
    }
}

#[derive(Default)]
struct Scripts {
    text: Mutex<Option<String>>,
    saves: Mutex<u32>,
}

impl DeployScripts for Scripts {
    fn location(&self, project: &str, environment: &str) -> String {
        format!("/v/projects/{project}/deploy/{environment}/script.sh")
    }
    fn load(&self, _: &str, _: &str) -> Result<Option<String>, ProjectError> {
        Ok(self.text.lock().unwrap().clone())
    }
    fn save(&self, _: &str, _: &str, text: &str) -> Result<Option<String>, ProjectError> {
        *self.saves.lock().unwrap() += 1;
        let before = self.text.lock().unwrap().replace(text.to_string());
        Ok(before.map(|_| "/v/archive/old.sh".to_string()))
    }
}

#[test]
fn a_missing_script_reads_as_none_with_its_place() {
    let scripts = Scripts::default();
    let found = Deployment::new(&Store, &scripts)
        .script("api", "prod")
        .unwrap();
    assert_eq!(found.path, "/v/projects/api/deploy/prod/script.sh");
    assert_eq!(found.text, None);
}

#[test]
fn saves_normalized_text_and_skips_an_unchanged_one() {
    let scripts = Scripts::default();
    let deploy = Deployment::new(&Store, &scripts);

    let first = deploy
        .save_script("api", "prod", "set -eu\r\necho hi")
        .unwrap();
    assert!(!first.unchanged && first.archived.is_none());
    assert_eq!(
        scripts.text.lock().unwrap().as_deref(),
        Some("set -eu\necho hi\n")
    );

    let same = deploy
        .save_script("api", "prod", "set -eu\necho hi\n")
        .unwrap();
    assert!(same.unchanged);
    assert_eq!(*scripts.saves.lock().unwrap(), 1);

    let next = deploy.save_script("api", "prod", "echo bye\n").unwrap();
    assert_eq!(next.archived.as_deref(), Some("/v/archive/old.sh"));
}

#[test]
fn refuses_an_unknown_environment_and_a_blank_script() {
    let scripts = Scripts::default();
    let deploy = Deployment::new(&Store, &scripts);
    assert!(deploy.script("api", "dev").is_err());
    assert!(deploy.save_script("api", "dev", "echo\n").is_err());
    assert!(deploy.save_script("api", "prod", "   \n").is_err());
    assert_eq!(*scripts.saves.lock().unwrap(), 0);
}
