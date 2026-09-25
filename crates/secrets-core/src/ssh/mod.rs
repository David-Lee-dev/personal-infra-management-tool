//! SSH 접속 설정 — 별칭마다 어느 인스턴스의 어느 계정인지 기록하고, 그룹마다 conf 파일을 만든다.
//!
//! `~/.ssh/config` 는 `Include` 한 줄로 이 파일들을 합친다. 주소와 키 경로는 시크릿 저장소의
//! 서버 계정 기록에서 가져오므로, 계정을 바꾼 뒤 다시 만들면 설정이 따라온다. 별칭 · 그룹 ·
//! 계정은 사용자가 정한다.
//!
//! ```text
//! ~/.secrets/ssh/hosts.toml      기록 — 별칭 · 그룹 · 인스턴스 · 계정
//! ~/.secrets/ssh/<그룹>.conf     만든 설정. 다시 만들 때 통째로 덮어쓴다
//! ```

use serde::{Deserialize, Serialize};

use crate::project::{ServerSeat, ServerSeats};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SshHost {
    pub alias: String,
    pub group: String,
    pub instance: String,
    pub login: String,
}

#[derive(Debug)]
pub enum SshError {
    Invalid(String),
    Missing(String),
    Storage(String),
}

impl std::fmt::Display for SshError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SshError::Invalid(detail) | SshError::Storage(detail) => write!(f, "{detail}"),
            SshError::Missing(what) => write!(f, "{what}을(를) 찾을 수 없습니다."),
        }
    }
}

/// `~/.ssh/config` 를 읽은 결과.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UserConfig {
    /// 이 도구의 conf 파일들을 `Include` 하는가.
    pub includes_ours: bool,
    /// 그 파일에 직접 적힌 Host 별칭들. `*` 같은 패턴은 뺀다.
    pub aliases: Vec<String>,
}

/// 별칭 기록이 놓이는 곳.
pub trait SshStore: Send + Sync {
    fn load(&self) -> Result<Vec<SshHost>, SshError>;
    /// 기록을 통째로 다시 쓴다.
    fn save(&self, hosts: &[SshHost]) -> Result<(), SshError>;
}

/// 만든 conf 파일과 `~/.ssh/config`.
pub trait SshFiles: Send + Sync {
    /// 지금 있는 conf 파일의 그룹 이름들.
    fn groups(&self) -> Vec<String>;
    fn write_group(&self, group: &str, text: &str) -> Result<(), SshError>;
    fn remove_group(&self, group: &str) -> Result<(), SshError>;
    fn user_config(&self) -> UserConfig;
    /// `~/.ssh/config` 맨 위에 `Include` 한 줄을 넣는다. 넣기 전에 원본을 보관한다.
    /// 보관한 자리를 돌려준다. 원본이 없었으면 없다.
    fn add_include(&self) -> Result<Option<String>, SshError>;
}

/// 화면에 보여 줄 별칭 하나 — 기록과, 시크릿 저장소에서 찾은 계정.
#[derive(Debug, Clone)]
pub struct HostView {
    pub host: SshHost,
    /// 기록한 인스턴스 · 계정을 시크릿 저장소에서 찾지 못하면 없다.
    pub seat: Option<ServerSeat>,
}

#[derive(Debug, Clone)]
pub struct Overview {
    pub hosts: Vec<HostView>,
    pub user: UserConfig,
    /// 직접 쓴 `~/.ssh/config` 에도 있는 별칭. 먼저 나오는 쪽이 쓰인다.
    pub duplicates: Vec<String>,
}

/// 별칭. 공백이나 패턴 글자가 있으면 SSH 가 다른 뜻으로 읽는다.
pub fn check_alias(text: &str) -> Result<String, SshError> {
    let alias = text.trim();
    let valid = !alias.is_empty()
        && alias
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'));
    if valid {
        Ok(alias.to_string())
    } else {
        Err(SshError::Invalid(
            "별칭에는 영문 · 숫자 · - · _ · .만 쓸 수 있습니다.".into(),
        ))
    }
}

/// 인스턴스 이름과 계정 이름으로 만든 별칭 — `<인스턴스 이름>-<계정>`. 별칭에 쓸 수 없는 글자는
/// `-` 로 바꾸고, 그러고 나서 이름이 남지 않으면(비었거나 전부 한글 등) 인스턴스 ID 를 쓴다.
pub fn alias_for(instance_name: &str, instance: &str, login: &str) -> String {
    let base = clean(instance_name);
    let base = if base.is_empty() {
        clean(instance)
    } else {
        base
    };
    clean(&format!("{base}-{login}"))
}

/// 별칭에 쓸 수 있는 글자만 남긴다. 나머지는 `-` 로 바꾸고, `-` 가 겹치거나 양끝에 오지 않게 한다.
fn clean(text: &str) -> String {
    let mut out: String = text
        .trim()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') {
                c
            } else {
                '-'
            }
        })
        .collect();
    while out.contains("--") {
        out = out.replace("--", "-");
    }
    out.trim_matches('-').to_string()
}

/// 그룹. conf 파일 이름이 된다.
pub fn check_group(text: &str) -> Result<String, SshError> {
    let group = text.trim();
    if group.is_empty() || group.starts_with('.') || group.contains(['/', '\\', ' ', '*']) {
        return Err(SshError::Invalid(
            "그룹 이름을 확인하세요. / · 공백 · *는 쓸 수 없습니다.".into(),
        ));
    }
    Ok(group.to_string())
}

/// 그룹 하나의 conf 파일 내용. 계정을 찾지 못한 별칭은 빼고 이유를 주석으로 남긴다.
pub fn render(group: &str, hosts: &[&HostView]) -> String {
    let mut text = format!(
        "# 인프라 콘솔이 만든 파일입니다. 앱의 SSH 접속 화면에서 바꾸세요.\n\
         # 직접 고친 내용은 다시 만들 때 사라집니다.\n# 그룹: {group}\n"
    );
    for view in hosts {
        let host = &view.host;
        match &view.seat {
            Some(seat) => text.push_str(&format!(
                "\nHost {}\n    HostName {}\n    User {}\n    IdentityFile {}\n    IdentitiesOnly yes\n",
                host.alias, seat.address, seat.login, seat.key_path
            )),
            None => text.push_str(&format!(
                "\n# {}: 서버 계정 {}/{}을(를) 시크릿 저장소에서 찾을 수 없어 뺐습니다.\n",
                host.alias, host.instance, host.login
            )),
        }
    }
    text
}

pub struct SshConfig<'a> {
    store: &'a dyn SshStore,
    files: &'a dyn SshFiles,
    seats: &'a dyn ServerSeats,
}

impl<'a> SshConfig<'a> {
    pub fn new(
        store: &'a dyn SshStore,
        files: &'a dyn SshFiles,
        seats: &'a dyn ServerSeats,
    ) -> SshConfig<'a> {
        SshConfig {
            store,
            files,
            seats,
        }
    }

    pub fn overview(&self) -> Result<Overview, SshError> {
        let hosts = self.views()?;
        let user = self.files.user_config();
        let duplicates = hosts
            .iter()
            .map(|v| v.host.alias.clone())
            .filter(|a| user.aliases.contains(a))
            .collect();
        Ok(Overview {
            hosts,
            user,
            duplicates,
        })
    }

    /// 별칭을 더하고 conf 파일을 다시 만든다.
    pub fn add(&self, host: &SshHost) -> Result<Overview, SshError> {
        let alias = check_alias(&host.alias)?;
        let group = check_group(&host.group)?;
        let mut hosts = self.store.load()?;
        if hosts.iter().any(|h| h.alias == alias) {
            return Err(SshError::Invalid(format!(
                "별칭 {alias}은(는) 이미 있습니다."
            )));
        }
        let found = self
            .seats
            .seats()
            .into_iter()
            .any(|s| s.instance == host.instance && s.login == host.login);
        if !found {
            return Err(SshError::Missing(format!(
                "서버 계정 {}/{}",
                host.instance, host.login
            )));
        }
        hosts.push(SshHost {
            alias,
            group,
            instance: host.instance.clone(),
            login: host.login.clone(),
        });
        self.store.save(&hosts)?;
        self.regenerate()?;
        self.overview()
    }

    /// 별칭을 빼고 conf 파일을 다시 만든다. 서버 계정과 키는 건드리지 않는다.
    pub fn remove(&self, alias: &str) -> Result<Overview, SshError> {
        let mut hosts = self.store.load()?;
        let before = hosts.len();
        hosts.retain(|h| h.alias != alias);
        if hosts.len() == before {
            return Err(SshError::Missing(format!("별칭 {alias}")));
        }
        self.store.save(&hosts)?;
        self.regenerate()?;
        self.overview()
    }

    /// 기록과 지금 시크릿 저장소의 계정으로 conf 파일을 전부 다시 만든다. 별칭이 없는 그룹의
    /// 파일은 지운다 — 그 디렉토리의 conf 파일은 전부 이 도구가 만든 것이다.
    pub fn regenerate(&self) -> Result<(), SshError> {
        let views = self.views()?;
        let mut groups: Vec<&str> = views.iter().map(|v| v.host.group.as_str()).collect();
        groups.sort();
        groups.dedup();
        for group in &groups {
            let mine: Vec<&HostView> = views.iter().filter(|v| v.host.group == *group).collect();
            self.files.write_group(group, &render(group, &mine))?;
        }
        for stale in self.files.groups() {
            if !groups.contains(&stale.as_str()) {
                self.files.remove_group(&stale)?;
            }
        }
        Ok(())
    }

    pub fn add_include(&self) -> Result<Option<String>, SshError> {
        if self.files.user_config().includes_ours {
            return Ok(None);
        }
        self.files.add_include()
    }

    fn views(&self) -> Result<Vec<HostView>, SshError> {
        let seats = self.seats.seats();
        let mut views: Vec<HostView> = self
            .store
            .load()?
            .into_iter()
            .map(|host| {
                let seat = seats
                    .iter()
                    .find(|s| s.instance == host.instance && s.login == host.login)
                    .cloned();
                HostView { host, seat }
            })
            .collect();
        views.sort_by(|a, b| (&a.host.group, &a.host.alias).cmp(&(&b.host.group, &b.host.alias)));
        Ok(views)
    }
}
