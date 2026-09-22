//! 화면으로 넘기는 표현.
//!
//! core 의 타입을 그대로 내보내지 않고 여기서 한 번 번역한다. 비밀값이 화면으로
//! 새지 않도록 경계를 한 곳에 모으기 위한 것이다.

use serde::Serialize;

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
    pub browser_login: bool,
    /// 브라우저에서 받은 코드를 되돌려 넣어야 끝나는가.
    pub browser_code: bool,
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

