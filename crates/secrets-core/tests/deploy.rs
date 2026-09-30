//! 배포 스크립트 — 있는 환경에만, 이름이 맞는 것만, 비어 있지 않은 것만, 바뀐 것만 쓴다.

use std::collections::BTreeMap;
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
                server: "i-1".into(),
                instance: None,
                address: None,
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

/// 스크립트 이름 → 내용.
#[derive(Default)]
struct Scripts {
    texts: Mutex<BTreeMap<String, String>>,
    saves: Mutex<u32>,
}

impl DeployScripts for Scripts {
    fn names(&self, _: &str, _: &str) -> Result<Vec<String>, ProjectError> {
        Ok(self.texts.lock().unwrap().keys().cloned().collect())
    }
    fn location(&self, project: &str, environment: &str, script: &str) -> String {
        format!("/v/projects/{project}/deploy/{environment}/{script}.sh")
    }
    fn load(&self, _: &str, _: &str, script: &str) -> Result<Option<String>, ProjectError> {
        Ok(self.texts.lock().unwrap().get(script).cloned())
    }
    fn save(
        &self,
        _: &str,
        _: &str,
        script: &str,
        text: &str,
    ) -> Result<Option<String>, ProjectError> {
        *self.saves.lock().unwrap() += 1;
        let before = self
            .texts
            .lock()
            .unwrap()
            .insert(script.into(), text.into());
        Ok(before.map(|_| format!("/v/archive/{script}-old.sh")))
    }
    fn remove(&self, _: &str, _: &str, script: &str) -> Result<Option<String>, ProjectError> {
        let before = self.texts.lock().unwrap().remove(script);
        Ok(before.map(|_| format!("/v/archive/{script}-old.sh")))
    }
}

#[test]
fn a_missing_script_reads_as_none_with_its_place() {
    let scripts = Scripts::default();
    let found = Deployment::new(&Store, &scripts)
        .script("api", "prod", "deploy")
        .unwrap();
    assert_eq!(found.name, "deploy");
    assert_eq!(found.path, "/v/projects/api/deploy/prod/deploy.sh");
    assert_eq!(found.text, None);
}

#[test]
fn saves_normalized_text_and_skips_an_unchanged_one() {
    let scripts = Scripts::default();
    let deploy = Deployment::new(&Store, &scripts);

    let first = deploy
        .save_script("api", "prod", "deploy", "set -eu\r\necho hi")
        .unwrap();
    assert!(!first.unchanged && first.archived.is_none());
    assert_eq!(
        scripts.texts.lock().unwrap()["deploy"],
        "set -eu\necho hi\n"
    );

    let same = deploy
        .save_script("api", "prod", "deploy", "set -eu\necho hi\n")
        .unwrap();
    assert!(same.unchanged);
    assert_eq!(*scripts.saves.lock().unwrap(), 1);

    let next = deploy
        .save_script("api", "prod", "deploy", "echo bye\n")
        .unwrap();
    assert_eq!(next.archived.as_deref(), Some("/v/archive/deploy-old.sh"));
}

#[test]
fn keeps_several_named_scripts_per_environment() {
    let scripts = Scripts::default();
    let deploy = Deployment::new(&Store, &scripts);
    deploy
        .save_script("api", "prod", "deploy", "echo deploy\n")
        .unwrap();
    deploy
        .save_script("api", "prod", " migrate ", "echo migrate\n")
        .unwrap();

    assert_eq!(
        deploy.scripts("api", "prod").unwrap(),
        vec!["deploy".to_string(), "migrate".to_string()]
    );
    assert_eq!(
        deploy
            .script("api", "prod", "migrate")
            .unwrap()
            .text
            .as_deref(),
        Some("echo migrate\n")
    );
}

#[test]
fn removes_a_script_to_the_archive_and_refuses_a_missing_one() {
    let scripts = Scripts::default();
    let deploy = Deployment::new(&Store, &scripts);
    deploy
        .save_script("api", "prod", "restart", "echo restart\n")
        .unwrap();

    let kept = deploy.remove_script("api", "prod", "restart").unwrap();
    assert_eq!(kept, "/v/archive/restart-old.sh");
    assert!(deploy.scripts("api", "prod").unwrap().is_empty());
    assert!(deploy.remove_script("api", "prod", "restart").is_err());
}

#[test]
fn refuses_an_unknown_environment_a_bad_name_and_a_blank_script() {
    let scripts = Scripts::default();
    let deploy = Deployment::new(&Store, &scripts);
    assert!(deploy.script("api", "dev", "deploy").is_err());
    assert!(
        deploy
            .save_script("api", "dev", "deploy", "echo\n")
            .is_err()
    );
    assert!(
        deploy
            .save_script("api", "prod", "deploy", "   \n")
            .is_err()
    );
    for bad in ["", "a/b", "..", "a.b", "배포", "a b", "-x"] {
        assert!(
            deploy.save_script("api", "prod", bad, "echo\n").is_err(),
            "{bad}"
        );
    }
    assert_eq!(*scripts.saves.lock().unwrap(), 0);
}
