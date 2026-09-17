//! Statement assembly.
//!
//! Every statement in this crate is built from a whitelist: table and column
//! names come from `match` arms over closed sets, `?` counts from
//! [`placeholders`], and any dimension name from an explicit membership test.
//! No caller-supplied text reaches the statement text; values are bound. That
//! is what [`sqlx::AssertSqlSafe`] is being asserted about here, in the one
//! place it can be read in full.

pub(crate) type Sql = sqlx::query::Query<'static, sqlx::Sqlite, sqlx::sqlite::SqliteArguments>;

pub(crate) fn sql(text: String) -> Sql {
    sqlx::query(sqlx::AssertSqlSafe(text))
}

/// `?,?,?` for an `IN` list.
pub(crate) fn placeholders(n: usize) -> String {
    let mut out = String::with_capacity(n * 2);
    for i in 0..n {
        if i > 0 {
            out.push(',');
        }
        out.push('?');
    }
    out
}

/// Binds a predicate's values in order. Every filter value is a string, which
/// is why one helper covers all of them.
pub(crate) fn bind_all(mut query: Sql, binds: &[String]) -> Sql {
    for value in binds {
        query = query.bind(value.clone());
    }
    query
}

/// Binds an `IN` list's values in order.
pub(crate) fn bind_kinds(mut query: Sql, kinds: &[&str]) -> Sql {
    for kind in kinds {
        query = query.bind((*kind).to_owned());
    }
    query
}
