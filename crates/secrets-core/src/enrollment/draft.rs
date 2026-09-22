//! 사람이 적는 것.
//!
//! 확인으로 알 수 있는 것은 여기 없다. 신원·권한·만료일을 적어 넣을 자리가 애초에
//! 없어야 화면이 그 값을 고쳐 보낼 수 없다.

/// 사람이 적는 것. 확인으로 알 수 있는 것은 여기 없다.
pub struct Draft {
    pub slug: String,
    pub display: String,
    pub note: String,
}

