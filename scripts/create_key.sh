USAGE="<script> <prod|staging> <scope> <key> [expiry_date]
Expiry default to 24 hours from now with second precision"

if [ $1 = "prod" ]; then
    BACKEND="https://api.pazufa.de"
elif [ $1 = "staging" ]; then
    BACKEND="https://staging.api.pazufa.de"
else
    echo "Unknown Backend, use either \`prod\` or \`staging\`"
    exit 1
fi

if [ -z ${4+x} ]; then
    echo "Setting default key 24 hours from now"
    EXPIRY="$(date -I seconds --utc -d '+24 hours')"
else
    EXPIRY=$4
fi
SCOPE=$3
KEY=$2

echo "Called with"
echo "Backend: $BACKEND"
echo "Scope: $SCOPE"
echo "Key: $KEY"
echo "Expiry Date: $EXPIRY"
echo "Request body: {\"scope\": \"$SCOPE\", \"expires_at\": \"$EXPIRY\"}" 

curl $BACKEND/api/v2/auth -H "x-api-key: $KEY" -H"Content-Type: application/json" -d "{\"scope\": \"$SCOPE\", \"expires_at\": \"$EXPIRY\"}" || echo $USAGE
