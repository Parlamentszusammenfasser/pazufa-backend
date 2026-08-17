USAGE="<script> <subdirectory> <docker image name> # Checks whether a docker image named \`pazufa/<docker image name>:<tag>\` where <tag> is the version number of the cargo.toml of that subdir is still available"

if [ $# -ne 2 ]; then
    echo $USAGE
    exit 1
fi

SUBDIR=$1
IMAGE_NAME=$2

VERSION=$(grep '^version' $SUBDIR/Cargo.toml | head -1 | sed 's/version = "//;s/"//')

STATUS=$(curl -s -o /dev/null -w "%{http_code}" "https://hub.docker.com/v2/repositories/pazufa/$IMAGE_NAME/tags/$VERSION/")

if [ $STATUS -eq 200 ]; then
    echo "Tag already exists"
    exit 1
else
    echo "Tag does not yet exist"
    exit 0
fi

