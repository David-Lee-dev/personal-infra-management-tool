//! provider 를 연결하려면 무엇을 받아 적어야 하는가.
//!
//! 자격의 구성은 provider 마다 다르다. 화면은 이 명세를 보고 폼을 그린다.




/// 어떻게 붙는가.
///
/// 두 개의 불리언으로 두면 "브라우저는 안 쓰는데 코드는 받는다" 같은 있을 수 없는
/// 조합이 표현된다. 실제로는 셋 중 하나다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoginFlow {
    /// 받아 적은 값으로 붙는다.
    Credential,
    /// 브라우저가 열리고 CLI 가 스스로 끝낸다.
    BrowserCallback,
    /// 브라우저에서 받은 코드를 되돌려 넣어야 끝난다.
    BrowserCode,
}

/// 입력 칸 하나.
#[derive(Debug, Clone, Copy)]
pub struct Field {
    pub key: &'static str,
    pub label: &'static str,
    /// 가려서 입력받아야 하는가.
    pub secret: bool,
    /// 입력칸 아래 보여줄 설명.
    pub help: &'static str,
    pub required: bool,
}

/// 값을 받지 않고 브라우저에서 처리해야 하는 경우, 열어줄 주소.
#[derive(Debug, Clone, Copy)]
pub struct Browser {
    pub label: &'static str,
    pub url: &'static str,
}

/// provider 를 연결하는 방법.
#[derive(Debug, Clone, Copy)]
pub struct Method {
    pub fields: &'static [Field],
    /// 이 provider 는 어떻게 붙는가.
    pub flow: LoginFlow,
    /// 값을 얻으러 갈 곳. 폼 옆에 링크로 띄운다.
    pub browser: Option<Browser>,
    /// 사용자에게 보여줄 안내.
    pub guidance: &'static str,
}

/// 입력값 한 묶음. 키는 `Field::key`.
pub type Values = std::collections::HashMap<String, String>;
