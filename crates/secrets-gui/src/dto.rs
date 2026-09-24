//! 화면으로 넘기는 표현.
//!
//! core 의 타입을 그대로 내보내지 않고 여기서 한 번 번역한다. 비밀값이 화면으로
//! 새지 않도록 경계를 한 곳에 모으기 위한 것이다.

use serde::{Deserialize, Serialize};

/// 프론트로 넘기는 표현. core 의 타입을 그대로 노출하지 않고 여기서 한 번 번역한다.
/// 비밀값이 프론트로 새지 않도록 경계를 한 곳으로 모으기 위한 것이다.
#[derive(Serialize, Clone)]
pub struct ToolRow {
    pub id: String,
    pub path: Option<String>,
    /// 사람에게 보여줄 설치 방법 한 줄.
    pub install: String,
    /// 설치 버튼을 달 수 있는가. false 면 안내만 한다.
    pub installable: bool,
    pub requirement: String,
    /// 파싱된 버전. 못 읽었으면 None.
    pub version: Option<String>,
    /// 버전 명령의 원문 첫 줄. 파싱 실패 시 근거로 보여준다.
    pub version_raw: Option<String>,
    pub meets_minimum: bool,
    pub minimum: Option<String>,
    pub minimum_reason: String,
    /// 컨텍스트별 격리가 가능한가. isolated | leaked | inconclusive | n/a
    pub isolation: &'static str,
    /// 격리에 쓰는 환경변수.
    pub isolation_env: String,
    /// 그렇게 판정한 근거.
    pub isolation_evidence: String,
}

/// 검사 한 판의 결과.
#[derive(Clone, Serialize)]
pub struct Snapshot {
    pub tools: Vec<ToolRow>,
    pub total: usize,
    pub found: usize,
    /// 쓸 수 없는 툴 — 없거나, 버전이 낮거나, 계정 격리가 깨졌다. 비어 있어야 정상이다.
    pub blocking: Vec<String>,
    /// 격리를 확인해야 하는 툴 수와 실제로 확인된 수.
    #[serde(rename = "isolationChecked")]
    pub isolation_checked: usize,
    pub isolated: usize,
}

/// 프론트로 넘기는 계정 표현.
#[derive(Serialize)]
pub struct AccountRow {
    pub slug: String,
    pub provider: &'static str,
    pub display: String,
    pub note: String,
    pub identity_kind: String,
    pub identity_name: String,
    /// 이 계정 전용 CLI 설정 홈. 격리의 실체라 사용자가 볼 수 있어야 한다.
    pub cli_home: String,
    pub verified_at: Option<String>,
    pub verified_ok: Option<bool>,
    pub verified_detail: Option<String>,
    /// 만료일 (`YYYY-MM-DD`). 적지 않았으면 None.
    pub expires: Option<String>,
    /// unset | ok | soon | expired
    pub expiry: &'static str,
    /// soon 이면 남은 일수, expired 면 지난 일수.
    pub expiry_days: Option<i64>,
    /// 만료됐을 때 무엇을 해야 하는가. 자격 종류마다 다르다.
    pub renewal_hint: &'static str,
    /// 이 자격이 가진 권한.
    pub scopes: Vec<String>,
    /// 지난 자격 교체 횟수.
    pub replacements: usize,
    /// 지금 전역으로 활성화된 계정인가.
    pub is_active: bool,
    /// 전역 전환으로 갈아끼울 자리. 지원하지 않으면 None.
    pub global_path: Option<String>,
    /// 전역 전환이 다른 도구에 영향을 줄 수 있으면 그 이유.
    pub caution: Option<&'static str>,
    /// 이 계정으로 커밋할 때 쓸 이메일.
    pub git_email: Option<String>,
    /// AWS 계정 번호.
    pub aws_account_id: Option<String>,
    /// root 에 액세스 키가 있는가.
    pub root_keys_present: Option<bool>,
    /// root 에 MFA 가 걸려 있는가.
    pub root_mfa: Option<bool>,
}

#[derive(Serialize)]
pub struct AccountList {
    pub accounts: Vec<AccountRow>,
    /// 읽지 못한 항목. 조용히 숨기면 계정이 사라진 것처럼 보인다.
    pub errors: Vec<String>,
    /// 만료가 임박했거나 지난 계정. 어느 탭에 있든 상시로 알린다.
    pub alerts: Vec<String>,
}

#[derive(Serialize)]
pub struct FieldSpec {
    pub key: &'static str,
    pub label: &'static str,
    pub secret: bool,
    pub help: &'static str,
    pub required: bool,
}

#[derive(Serialize)]
pub struct FormSpec {
    pub fields: Vec<FieldSpec>,
    pub guidance: &'static str,
    pub browser_label: Option<&'static str>,
    pub browser_url: Option<&'static str>,
    /// 이 provider 를 다루는 CLI 가 설치돼 있는가.
    pub tool_ready: bool,
    pub tool: &'static str,
    /// 입력 대신 브라우저 로그인으로 연결하는가.
    /// `credential` · `browser` · `browser-code` 중 하나.
    pub flow: &'static str,
}


/// 입력한 자격으로 신원을 미리 읽어 온다.
///
/// 계정을 만들기 전에 임시 홈에서 돌린다. 이름과 만료일을 사람이 추측해 적는
/// 대신 자격 자체에서 읽어 오기 위한 것이다.
#[derive(Serialize)]
pub struct ProbeResult {
    /// 확인이 끝난 자격을 가리키는 표. 계정을 만들 때 이것만 되돌려 보낸다.
    pub preparation: String,
    pub kind: String,
    pub name: String,
    pub slug: String,
    pub display: String,
    pub expires: Option<String>,
    pub scopes: Vec<String>,
    pub git_email: Option<String>,
    pub aws_account_id: Option<String>,
    pub root_keys_present: Option<bool>,
    pub root_mfa: Option<bool>,
}


/// 코드를 받아 와야 끝나는 로그인을 시작한다.
#[derive(Serialize)]
pub struct ChallengeResult {
    /// 두 번째 단계가 같은 로그인을 가리키게 하는 표.
    pub preparation: String,
    pub url: String,
    /// 브라우저 페이지에서 대조할 세션 번호.
    pub session: String,
    pub note: String,
}

/// 프론트로 넘기는 배포 키 표현. 개인 키는 여기 담기지 않는다 —
/// 따로 부르는 명령 하나로만 나간다.
#[derive(Serialize)]
pub struct KeyRow {
    pub r#ref: String,
    pub domain: &'static str,
    /// 이 키가 무엇에 쓰이는가 — `coding` · `deploy` · `ci`.
    pub purpose: String,
    pub repo: String,
    pub account: String,
    pub write: bool,
    pub algorithm: String,
    pub fingerprint: String,
    /// 개인 키가 놓인 디렉토리. 격리의 실체라 사용자가 볼 수 있어야 한다.
    pub path: String,
    pub created_at: String,
    /// local | registered | rotating
    pub state: &'static str,
    pub remote_id: Option<String>,
    pub registered_at: Option<String>,
}

#[derive(Serialize)]
pub struct KeyList {
    pub keys: Vec<KeyRow>,
    /// 읽지 못한 기록. 조용히 숨기면 키가 사라진 것처럼 보인다.
    pub errors: Vec<String>,
}

/// GitHub 에는 있는데 이 금고에 개인 키가 없는 것.
#[derive(Serialize)]
pub struct UnownedRow {
    pub r#ref: String,
    pub domain: &'static str,
    pub account: String,
    pub title: String,
    /// 계정에 붙은 키면 None.
    pub repo: Option<String>,
    pub fingerprint: String,
    pub remote_id: String,
    pub registered_at: Option<String>,
}

#[derive(Serialize)]
pub struct ResolvedRepo {
    pub owner: String,
    pub name: String,
    pub slug: String,
}

/// 이 머신이 아는 SSH 호스트. 키를 어디로 보낼지 고르는 데 쓴다.
#[derive(Serialize)]
pub struct HostRow {
    pub alias: String,
    /// 실제 주소. 적혀 있지 않으면 별칭이 곧 주소다.
    pub address: Option<String>,
    pub user: Option<String>,
}





/// 이 금고가 쥐고 있는 pem 키.
#[derive(Serialize)]
pub struct AwsHeldKeyRow {
    pub r#ref: String,
    pub name: String,
    pub account: String,
    pub machine: String,
    pub region: String,
    pub fingerprint: String,
    /// AWS 가 말하는 지문과 맞춰 본 적이 있는가.
    pub verified: bool,
    pub purpose: String,
    pub adopted_at: String,
    pub path: String,
}

#[derive(Serialize)]
pub struct AwsKeyList {
    pub keys: Vec<AwsHeldKeyRow>,
    /// 읽지 못한 기록. 조용히 숨기면 키가 사라진 것처럼 보인다.
    pub errors: Vec<String>,
}

/// 화면이 들이겠다고 말하는 것.
///
/// 값이 많아 한 덩이로 받는다. 따로 받으면 인자 순서를 틀리기 쉽고, 그러면 리전과
/// 이름이 뒤바뀐 채 저장된다.
#[derive(Deserialize)]
pub struct Adoption {
    pub account: String,
    pub machine: String,
    pub region: String,
    pub name: String,
    /// 개인 키가 지금 있는 자리.
    pub path: String,
    pub purpose: String,
    /// AWS 가 말하는 지문. 있으면 맞아야 들인다. 없으면 확인 못 한 것으로 남는다.
    pub expected: Option<String>,
}

/// 손에 든 개인 키가 그 키페어의 것인지, 그리고 무엇이 그 파일을 쓰고 있는지.
#[derive(Serialize)]
pub struct PrivateKeyCheck {
    /// AWS 의 키페어 이름. 파일 이름과 다를 수 있다 — 지문으로 찾은 것이다.
    pub name: String,
    pub fingerprint: String,
    /// AWS 가 지문을 주어 맞춰 볼 수 있었는가.
    pub verified: bool,
    pub account_id: String,
    /// 이 파일을 가리키는 `~/.ssh/config` 호스트. 옮기면 끊긴다.
    pub referred_by: Vec<String>,
}

/// 계정이 앉을 자리. 값이 많아 한 덩이로 받는다.
///
/// 따로 받으면 인자 순서를 틀리기 쉽고, 그러면 리전과 인스턴스가 뒤바뀐 채
/// 서버에 심긴다.
#[derive(Deserialize)]
pub struct Where {
    /// AWS 계정 ID. 금고 경로의 첫 단계다.
    pub aws_account: String,
    /// ec2 | lightsail
    pub machine: String,
    pub region: String,
    pub keypair: String,
    pub instance: String,
    /// 서버의 로그인 이름.
    pub account: String,
    /// pem 으로 들어갈 때 쓰는 계정. EC2 우분투는 `ubuntu` 다.
    pub via: String,
    pub address: String,
    pub workspace: String,
    pub group: String,
}

/// 한 번에 만들 계정 하나.
#[derive(Deserialize)]
pub struct NewSeat {
    pub account: String,
    /// admin | user
    pub role: String,
    pub purpose: String,
}

/// 계정 하나를 만든 결과. `error` 가 없으면 만들어졌다.
#[derive(Serialize)]
pub struct SeatOutcome {
    pub account: String,
    pub error: Option<String>,
}

/// 이 인스턴스가 계정을 받을 준비가 되었는가.
#[derive(Serialize)]
pub struct HostReadiness {
    pub ok: bool,
    /// 무엇이 없는가. 화면이 그대로 보여 준다.
    pub missing: Vec<String>,
    pub sudo: bool,
    pub acl: bool,
    pub useradd: bool,
    pub visudo: bool,
    /// 모자란 것을 채울 때 쓸 도구.
    pub packager: Option<String>,
}

/// 이 금고가 들인 인스턴스 계정.
#[derive(Serialize)]
pub struct InstanceAccountRow {
    pub r#ref: String,
    pub account: String,
    /// admin | user
    pub role: &'static str,
    pub purpose: String,
    pub instance: String,
    pub instance_name: String,
    pub address: String,
    pub keypair: String,
    pub region: String,
    pub via: String,
    pub fingerprint: String,
    pub workspace: String,
    pub group: String,
    /// local | installed | verified
    pub state: &'static str,
    pub verified_at: Option<String>,
    /// 우리가 만든 계정인가. 아니면 걷어낼 때 계정은 남긴다.
    pub ours: bool,
}

#[derive(Serialize)]
pub struct InstanceAccountList {
    pub accounts: Vec<InstanceAccountRow>,
    /// 읽지 못한 기록. 조용히 숨기면 계정이 사라진 것처럼 보인다.
    pub errors: Vec<String>,
}

/// IAM 정책 문장 하나. 화면에는 한 줄로 선다.
#[derive(Serialize, Clone)]
pub struct IamRuleRow {
    /// 허용 | 거부
    pub effect: &'static str,
    pub actions: String,
    pub target: String,
    pub condition: String,
}

/// 키를 넣었다고 기록한 곳 하나.
#[derive(Serialize, Clone)]
pub struct IamConsumerRow {
    pub host: String,
    pub file: String,
    pub id_variable: String,
    pub secret_variable: String,
    pub recorded_at: String,
}

/// IAM 하나. 시크릿은 담지 않는다.
#[derive(Serialize, Clone)]
pub struct IamRow {
    pub r#ref: String,
    pub account: String,
    pub name: String,
    pub app: String,
    pub env: String,
    pub perm: String,
    pub purpose: String,
    pub master: String,
    pub key_id: String,
    pub issued_at: String,
    pub created_at: String,
    /// 서비스 이름들. 목록의 칩에 쓴다.
    pub service: String,
    /// 대상 한 줄 요약.
    pub scope: String,
    pub rules: Vec<IamRuleRow>,
    pub consumers: Vec<IamConsumerRow>,
    pub path: String,
    /// 이 날부터 지울 수 있다. 하한이라, 이 날 전이면 묻지 않고도 막는다.
    pub deletable_from: String,
    /// 마지막으로 AWS 에 사용 기록을 물은 때. 묻기 전이면 없다.
    pub checked_at: Option<String>,
    /// 그때 AWS 가 말한 마지막 사용. 한 번도 쓰이지 않았으면 없다.
    pub last_use: Option<IamLastUse>,
}

#[derive(Serialize)]
pub struct IamList {
    pub users: Vec<IamRow>,
    pub errors: Vec<String>,
}

/// 만들기 화면이 치는 동안 보는 것.
#[derive(Serialize)]
pub struct IamPreview {
    /// 정해진 이름. 정할 수 없으면 `None` 이고 까닭은 `error` 에 있다.
    pub name: Option<String>,
    pub rules: Vec<IamRuleRow>,
    /// 규칙에서 벗어난 곳. 막지 않는다.
    pub problems: Vec<String>,
    pub error: Option<String>,
}

#[derive(Deserialize)]
pub struct IamDraft {
    pub master: String,
    pub account: String,
    pub app: String,
    pub env: String,
    #[serde(default)]
    pub perm: String,
    #[serde(default)]
    pub purpose: String,
    pub policy: String,
}

/// IAM 을 가리키는 자리.
#[derive(Deserialize)]
pub struct IamWhere {
    pub account: String,
    pub name: String,
}

/// 소비처 하나. 기록할 때와 뺄 때 쓴다.
#[derive(Deserialize)]
pub struct IamPlace {
    pub host: String,
    pub file: String,
    pub id_variable: String,
}

#[derive(Serialize, Clone)]
pub struct IamLastUse {
    pub at: String,
    pub service: String,
    pub region: String,
}

/// 기타 항목 하나를 가리킨다.
#[derive(Deserialize)]
pub struct EtcWhere {
    pub project: String,
    pub name: String,
}

/// 소비처 한 곳.
#[derive(Deserialize)]
pub struct EtcPlace {
    pub host: String,
    pub file: String,
}

#[derive(Serialize)]
pub struct EtcFileRow {
    pub name: String,
    pub size: u64,
    /// 앞 12자. 같은 파일인지 눈으로 맞춰 볼 만큼만.
    pub sha256: String,
    pub adopted_at: String,
}

#[derive(Serialize)]
pub struct EtcConsumerRow {
    pub host: String,
    pub file: String,
    pub recorded_at: String,
}

/// 기타 항목 하나. 여는 값은 이름만 담는다.
#[derive(Serialize)]
pub struct EtcRow {
    pub r#ref: String,
    pub project: String,
    pub name: String,
    pub kind: String,
    pub purpose: String,
    /// 화면에 보이는 금고 자리 (`~` 로 적음).
    pub path: String,
    /// 빌드 설정에 적을 절대 경로.
    pub absolute: String,
    pub file: EtcFileRow,
    pub values: Vec<String>,
    pub consumers: Vec<EtcConsumerRow>,
}

#[derive(Serialize)]
pub struct EtcList {
    pub items: Vec<EtcRow>,
    pub errors: Vec<String>,
}
