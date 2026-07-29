USAGE="<script> <prod|staging> <key>"

if [ "$#" -ne 2 ]; then
    echo "Usage: $USAGE"
    exit 1
fi

if [ $1 = "prod" ]; then
    BACKEND="https://api.pazufa.de"
elif [ $1 = "staging" ]; then
    BACKEND="https://staging.api.pazufa.de"
else
    echo "Unknown Backend, use either \`prod\` or \`staging\`"
    echo "Usage: $USAGE"
    exit 1
fi

KEY=$2

echo "Called with:"
echo "Backend: $BACKEND"
echo "Key to revoke: $REVOKE_KEY"
echo "Key to authenticate with: $KEY"

curl $BACKEND/api/v2/auth/keys -H "x-api-key: $KEY" || echo "Usage: $USAGE"
