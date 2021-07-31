# paprika-rs

A client for the [Paprika](https://www.paprikaapp.com) API and API server for
other uses.

The Paprika API is designed for use in app and given it's unofficial nature,
probably shouldn't be used much. This attempts to sync everything with a
PostgreSQL database that can be used for other purposes.

All data available through the Paprika app is synced (additions, changes, and
deletions) and can be queried through a GraphQL interface. The `/playground`
endpoint provides information about the schema.

It is currently read-only, but may have limited support for writing data back to
Paprika in the future.
