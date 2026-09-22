//! 이 머신의 시계.

use secrets_core::port::Clock;

use crate::clock;

pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> String {
        clock::now()
    }

    fn today(&self) -> String {
        clock::today()
    }
}
