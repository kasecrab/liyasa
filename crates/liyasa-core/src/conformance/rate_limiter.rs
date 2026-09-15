//! What every `RateLimiter` must do (PRD §30.2.5, §34.9).

use std::net::{IpAddr, Ipv4Addr};

use super::require;
use crate::ids::TokenId;
use crate::server::{IpNet, LimiterSubject, RateLimit, RateLimitKey, RateLimitPool, RateLimiter};

fn ip(last: u8) -> LimiterSubject {
    LimiterSubject::Ip(IpNet(IpAddr::V4(Ipv4Addr::new(203, 0, 113, last)), 32))
}

pub fn check(limiter: &dyn RateLimiter) {
    let limit = RateLimit {
        per_minute: 60,
        burst: 2,
        daily: None,
    };
    limiter.configure(RateLimitPool::Pages, limit);
    limiter.configure(RateLimitPool::Search, limit);

    let subject = RateLimitKey {
        pool: RateLimitPool::Pages,
        subject: ip(1),
    };
    for n in 0..limit.burst {
        require!(
            limiter.check(&subject).is_ok(),
            "request {n} is within the burst and must pass"
        );
    }
    require!(
        limiter.check(&subject).is_err(),
        "a request past the burst must be refused with RetryAfter"
    );

    let other_subject = RateLimitKey {
        pool: RateLimitPool::Pages,
        subject: ip(2),
    };
    require!(
        limiter.check(&other_subject).is_ok(),
        "one subject exhausting its budget must not limit another"
    );

    let other_pool = RateLimitKey {
        pool: RateLimitPool::Search,
        subject: ip(1),
    };
    require!(
        limiter.check(&other_pool).is_ok(),
        "pools are counted separately; exhausting Pages must not limit Search"
    );

    let token = RateLimitKey {
        pool: RateLimitPool::Rest,
        subject: LimiterSubject::Token(TokenId::new("conformance")),
    };
    limiter.configure(
        RateLimitPool::Rest,
        RateLimit {
            per_minute: 60,
            burst: 1,
            daily: None,
        },
    );
    require!(
        limiter.check(&token).is_ok(),
        "a token subject is limited like an address"
    );
    require!(
        limiter.check(&token).is_err(),
        "a token subject's burst is enforced"
    );
}
