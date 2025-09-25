use chrono::NaiveDate;
use chrono_tz::US::Eastern;

// Use this for naive dates - I'm based in Eastern currently
// and it's easier to just use a single reference time zone for all ops
pub fn now_date_naive_eastern() -> NaiveDate {
    chrono::Utc::now().with_timezone(&Eastern).date_naive()
}

pub trait DefaultDate {
    fn or_naive_date_now(&self) -> NaiveDate;
}
impl DefaultDate for Option<NaiveDate> {
    fn or_naive_date_now(&self) -> NaiveDate {
        match self {
            Some(e) => *e,
            None => now_date_naive_eastern(),
        }
    }
}
