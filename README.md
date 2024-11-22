# Distributed Spatial Internet Graph Relay

This is the current prototype for the DSIG relay server, written in Rust.

### Setup with Docker

- [Install Docker for your system](https://docs.docker.com/engine/install/)
- Copy `.env.sample` to `.env` and fill in the fields as appropriate
  - Make sure your DB URL points to postgres:5432 and not localhost:5432.
- Run `docker compose up -d` to spin up the necessary containers.

### Non-Docker Setup

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

To use the dsig CLI tool:
```sh
$ cargo run --bin cli
```

If you need to reset the state of the database:

### Docker

Delete the Postgres data volume for the containers, it will be reinitialized on startup.

### Non-Docker

```sh
sqlx database drop
sqlx database create
sqlx migrate run
```
