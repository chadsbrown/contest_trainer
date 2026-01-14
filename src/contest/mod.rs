pub mod cqww;
pub mod sprint;
pub mod sweepstakes;
pub mod types;

pub use cqww::CqWwContest;
pub use sprint::NaSprintContest;
pub use sweepstakes::SweepstakesContest;
pub use types::{Contest, ContestType, Exchange};

pub fn create_contest(contest_type: ContestType) -> Box<dyn Contest> {
    match contest_type {
        ContestType::CqWw => Box::new(CqWwContest::new()),
        ContestType::NaSprint => Box::new(NaSprintContest::new()),
        ContestType::Sweepstakes => Box::new(SweepstakesContest::new()),
    }
}
