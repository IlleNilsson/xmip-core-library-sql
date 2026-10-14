#![forbid(unsafe_code)]

//! SQL for Xmip, one copy for every domain that speaks it: the SQL
//! transports and their far ends, the archives in a SQL table and the SQL
//! script archive, and the SQL contract.
//!
//! ```text
//! the language
//!   delimiter.rs    Delimiter: delimited text, the closing delimiter doubled
//!   verb.rs         Verb: the keyword a statement opens with
//!   literal.rs      Literal: a literal read back, bytes or text
//!   dialect.rs      Dialect: what one server writes its own way, and the
//!                   one INSERT written and taken apart through it
//!   parameter.rs    Dialect::bound: the first parameter bound in by text
//! the dialects
//!   standard.rs     STANDARD: ISO SQL, a script any database loads
//!   tsql.rs         TSQL: SQL Server
//!   postgresql.rs   POSTGRESQL
//!   mariadb.rs      MARIADB
//!   oracle.rs       ORACLE: Oracle Database
//!   db2.rs          DB2: IBM DB2
//!   sqlite.rs       SQLITE
//! ```
//!
//! What stays with a technology is its wire — the protocol, the login, the
//! URI scheme a Location opens with — and what stays with a capability is
//! its own shape: the archive's row and receipt, the transport's column
//! setting and acknowledgement, the contract's classes of verbs (ADR-0044,
//! amendment 2026-10-12).

mod db2;
mod delimiter;
mod dialect;
mod literal;
mod mariadb;
mod oracle;
mod parameter;
mod postgresql;
mod sqlite;
mod standard;
mod tsql;
mod verb;

pub use db2::DB2;
pub use delimiter::Delimiter;
pub use dialect::{Dialect, Returning};
pub use literal::Literal;
pub use mariadb::MARIADB;
pub use oracle::ORACLE;
pub use postgresql::POSTGRESQL;
pub use sqlite::SQLITE;
pub use standard::STANDARD;
pub use tsql::TSQL;
pub use verb::Verb;
