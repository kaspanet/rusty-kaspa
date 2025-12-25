#!/bin/sh

set -e

# Parse command line arguments for --tag and --repo
ARCHES="linux/amd64 linux/arm64"
ARTIFACT="kaspad"
while [ $# -gt 0 ]; do
  case "$1" in
    --tag)
      shift
      TAG="$1"
      ;;
    --arches)
      shift
      ARCHES="$1"
      ;;
    --push)
      PUSH="push"
      ;;
    --artifact)
      shift
      ARTIFACT="$1"
      ;;
    --help|-h)
      echo "Usage: $0 --tag <tag> --artifact <artifact> [--arches <arches>] [--push]"
      echo ""
      echo "  --tag <tag>         Docker image tag (required)"
      echo "  --artifact <name>   Build target/artifact (default: \"$ARTIFACT\")"
      echo "  --arches <arches>   Space-separated list of architectures (default: \"$ARCHES\")"
      echo "  --push              Push the built images"
      echo "  --help, -h          Show this help message"
      exit 0
      ;;
    *)
      break
      ;;
  esac
  shift
done

if [ -z "$TAG" ]; then
  echo "Error: --tag argument is required"
  exit 1
fi

BUILD_DIR="$(dirname $0)"
docker=docker
id -nG $USER | grep -qw docker || docker="sudo $docker"

multi_arch_build() {
  echo
  echo "===================================================="
  echo " Running build for $1"
  echo "===================================================="
  dockerRepo="${DOCKER_REPO_PREFIX}-$1"
  dockerRepoArgs=

  if [ "$PUSH" = "push" ]; then
    dockerRepoArgs="$dockerRepoArgs --push"
  fi

  dockerRepoArgs="$dockerRepoArgs --tag $TAG"
  dockerRepoArgs="$dockerRepoArgs -f docker/Dockerfile.$1"

  $docker buildx build --platform=$(echo $ARCHES | sed 's/ /,/g') $dockerRepoArgs \
    --tag $TAG "$BUILD_DIR"
  echo "===================================================="
  echo " Completed build for $1"
  echo "===================================================="
}

echo
echo "===================================================="
echo " Setup multi arch build ($ARCHES)"
echo "===================================================="
$docker buildx create --name mybuilder \
--driver docker-container \
--node mybuilder0 \
--use --bootstrap
$docker buildx create --name=mybuilder --append --node=mybuilder0 --platform=$(echo $ARCHES | sed 's/ /,/g') --bootstrap --use
echo "SUCCESS - doing multi arch build"
multi_arch_build $ARTIFACT
