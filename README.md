# zk-messenger-node

Node for zkMessenger protocol

## Project structure:
- `apps`
    - `node` - zk-messenger-node which takes a role of a server
- `crates`
    - `api` - main logic of a server
    - `callback` - small helper crate for defining structures for communication between api and proof-verifier.
    - `proof-verifier` - separate proof verifier
    - `storage` - mongo db storage
    - `tests` - unit tests for a project
    - `types` - library with types used in the project. It contains errors, mongodb records, requests, responses, queries, etc.

## Docker Build

To build the Docker image with access to private GitHub repositories:

```bash
DOCKER_BUILDKIT=1 docker build --ssh default .
```

This command uses Docker BuildKit with SSH agent forwarding to authenticate with private GitHub repositories during the build process.

## Build

* `node` features
  * **default** - Enables verification feature. To run the node without default features, run the node with `--no-default-features` command line option.
  * **art_modifications** - Enable `api/art_modifications` feature
  * **verification** - Enable `api/verification` feature.

* `api` features
  * **api/art_modifications** - Enable art modification endpoints for use
  * **api/verification** - Enable Proof verification. (Automatically enables `api/art_modifications` feature.)
  * **api/integration-tests** - Enables integration tests.

### Run 

To run the node without features run it with `--no-default-features`.
```shell
cargo run -p zk-messenger-node --no-default-features --release -- run --config config.toml
```

## Tests

### Unit tests

### Integration tests
To test the node api, one should prepare the environment. Firstly raise the infrastructure in docker. Node can be run in docker or locally. Then one can run tests with feature `integration-tests` like the next:
```shell
cargo test -p api --features integration-tests --release
```


