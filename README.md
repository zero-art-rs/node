# zk-messenger-node

Node for zkMessenger protocol

## Docker Build

To build the Docker image with access to private GitHub repositories:

```bash
DOCKER_BUILDKIT=1 docker build --ssh default .
```

This command uses Docker BuildKit with SSH agent forwarding to authenticate with private GitHub repositories during the build process.

## Build

### Crate features

* **api/art_modifications** - Enable art modification endpoints for use
* **api/verification** - Enable Proof verification. (Automatically enables `art_modifications` feature.)  
* **api/integration-tests** - Enables integration tests.


## Tests

To test the node api, one should prepare the environment. Firstly raise the infrastructure in docker. Node can be run in docker or locally. Then one can run tests with feature `integration-tests` like the next:
```shell
cargo test -p api --features integration-tests --release
```


