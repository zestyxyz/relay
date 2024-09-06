# Spatial Internet Graph Relay Prototype Server

This is the current prototype for the SIG relay server, written in Rust.

## Setup

- [Install Rust](https://www.rust-lang.org/tools/install)
- Install necessary dependencies
  - On Ubuntu: `sudo apt install build-essential pkg-config libssl-dev postgresql`
- Install the sqlx CLI, which will be used for managing the database/migrations.
  - `cargo install sqlx-cli`
- Copy `.env.sample` to `.env` and fill in the fields as appropriate
- From the root directory, run the following commands:
  - `sqlx database create` to create the database at the URL specified in your `.env` file
  - `sqlx migrate run` to run the migrations in the `migrations` folder
- Build and run the server with `cargo run -r`

## Development

If you need to reset the state of the database:

```sh
sqlx database drop
sqlx database create
sqlx migrate run
```
