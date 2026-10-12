# xmip-core-library-sql

SQL for Xmip, one copy for every domain that speaks it: the SQL transports
and their far ends, the archives in a SQL table and the SQL script archive,
and the SQL contract. What a server's wire is — its protocol, its login, the
URI scheme a Location opens with — stays with the technology that speaks it;
what a capability makes of SQL — the archive's row and receipt, the
transport's payload column and acknowledgement, the contract's classes of
verbs — stays with the capability.

| Item | What it is |
| --- | --- |
| `Delimiter` | SQL's delimited text — a `'…'` literal, an `"…"`, `[…]` or backtick identifier — written with the closing delimiter doubled and read back with the doubling undone (ISO/IEC 9075, 5.3 and 5.4) |
| `Verb` | The keyword a statement opens with, which names its kind: read from a keyword or from a statement's first word. The SQL contract holds a script to a set of them; a SQL far end answers a statement by its verb |
| `Literal` | One literal read back from a statement: the bytes a binary literal names, or a string's text with its escapes undone |
| `Dialect` | What one server writes its own way: the identifier delimiter, what a bare identifier takes, the first parameter marker, the string literal, the binary literal and how a binary column comes back, and how an INSERT answers its new row's id (`Returning`). Through it, one INSERT is written and taken apart, a table is quoted segment by segment, and a parameter is bound in by text where it is a parameter and nowhere else (`Dialect::bound`) |
| `STANDARD` | ISO SQL: `"…"` identifiers, `'…'` literals, bytes as `X'…'`, `?` — the SQL script archive's script |
| `TSQL` | SQL Server: `[…]` identifiers, `N'…'` literals, bytes as `0x…`, `@P1`, `OUTPUT INSERTED` |
| `POSTGRESQL` | `"…"` identifiers, `'…'` literals with backslashes literal, bytes in the bytea hex form `\x…`, `$1`, `RETURNING` |
| `MARIADB` | Backtick identifiers, `'…'` literals with the backslash escapes the server reads, bytes as `X'…'` and back through `CONCAT('0x', HEX(…))`, `?`, the id asked for after the INSERT |
| `SQLITE` | ISO's literals, a BLOB back through `hex(…)`, `?`, `RETURNING` |

Until 2026-09-24 the SQL transports, the SQL script archive and the SQL
contract each wrote the doubling rule themselves, and two far ends read an
identifier's doubled delimiter as its end. Until 2026-10-12 the transport capability and the
archive capability each had a dialect of their own, every SQL technology
filled both, and the transport and the contract each read a statement's verb
their own way. The owner, 2026-10-12: *Well I guess that SQL should live
under a more common repo/module if it is shared between domains?* and *Go
ahead, it does not fit under archive* (ADR-0044, amendment 2026-10-12).

Xmip ships nothing for Oracle (ADR-0011, amendment 2026-10-12): there is no
PL/SQL dialect here, and a provider who wants one writes it.

`architecture.toml` carries the maturity.
