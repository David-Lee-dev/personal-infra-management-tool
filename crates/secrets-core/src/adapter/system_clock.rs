//! 이 머신의 시계.

use crate::date;
use crate::port::Clock;

pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> String {
        date::now()
    }

    fn today(&self) -> String {
        date::today()
    }
}
