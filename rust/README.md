# Azos Keeper - Rust

## Getting Started

### Prerequisites

- Rust - https://rustup.rs/ - The language that this project is compiled in
- Cargo Watch (optional) - `cargo install cargo-watch` - Allows watching for file changes during local development and re-running of the application

### Get Running Locally

These instructions will get you running locally and able to develop the project and execute it while developing.

1. Clone this codebase and navigate to the Rust implementation

   ```shell
   git clone git@github.com:AzosFinance/azos-keeper.git
   cd rust
   ```

2. Copy over the example `.env` file and modify the relevant values

   ```shell
   cp example.env .env
   vim .env
   ```

3. Run the project locally

   ```shell
   make local  # or "make watch" for handling file changes
   ```

### Generating a Release Build

Assuming you have largely followed the running locally instructions above, you should have the source code available and able to run.

1. Compile the project with release

   ```shell
   make release
   ```

### Building Docker Image

Similar to the release build, these instructions will create a Docker image locally that can then be pushed to a repository and used.

1. Execute a Docker image creation, tagged with `azos-keeper`

   ```shell
   make docker
   ```

1. Run the Docker image!

   ```shell
   docker run -it azos-keeper
   ```

## Need Help

TODO, contact information for Azos rust team

In the interim, contact Guillaume "gn0me" VanderEst:

- Discord: gn0me
- Twitter: gvanderest
- Email: gvanderest@gmail.com
