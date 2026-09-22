//! provider 를 연결하려면 무엇을 받아 적어야 하는가.
//!
//! 자격의 구성은 provider 마다 다르다. 화면은 이 명세를 보고 폼을 그린다.




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
    /// 입력 대신 브라우저 로그인으로 연결하는가.
    ///
    /// 받아 적을 비밀값이 없는 provider 가 있다.
    pub browser_login: bool,
    /// 브라우저에서 받은 코드를 되돌려 넣어야 끝나는가.
    ///
    /// gcloud 는 브라우저를 열고 localhost 로 결과를 받아 스스로 끝낸다.
    /// firebase 는 출력이 TTY 가 아니면 URL 과 코드 입력을 요구하는 흐름으로
    /// 빠지므로, 두 단계로 나눠야 한다.
    pub browser_code: bool,
    /// 값을 얻으러 갈 곳. 폼 옆에 링크로 띄운다.
    pub browser: Option<Browser>,
    /// 사용자에게 보여줄 안내.
    pub guidance: &'static str,
}

/// 입력값 한 묶음. 키는 `Field::key`.
pub type Values = std::collections::HashMap<String, String>;
