# zk-messenger-node

Node for zkMessenger protocol

## Docker Build

To build the Docker image with access to private GitHub repositories:

```bash
DOCKER_BUILDKIT=1 docker build --ssh default .
```

This command uses Docker BuildKit with SSH agent forwarding to authenticate with private GitHub repositories during the build process.
