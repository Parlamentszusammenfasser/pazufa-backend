USAGE="<script> <prod|staging> <scope> <key> [expiry_date] [keytag_prefix]
Expiry defaults to 24 hours from now with second precision
Remember you have to set an explicit time if you want named keytags"

if [ $1 = "prod" ]; then
    BACKEND="https://api.pazufa.de"
elif [ $1 = "staging" ]; then
    BACKEND="https://staging.api.pazufa.de"
else
    echo "Unknown Backend, use either \`prod\` or \`staging\`"
    echo "$USAGE"
    exit 1
fi

if [ -z ${4+x} ]; then
    echo "Setting default key 24 hours from now"
    EXPIRY="$(date -I seconds --utc -d '+24 hours')"
else
    EXPIRY=$4
fi
SCOPE=$2
KEY=$3
KEYTAG_PREFIX=$5

echo "Called with"
echo "Backend: $BACKEND"
echo "Scope: $SCOPE"
echo "Key: $KEY"
echo "Expiry Date: $EXPIRY"
echo "Request body: {\"scope\": \"$SCOPE\", \"expires_at\": \"$EXPIRY\"}" 

curl $BACKEND/api/v2/auth -H "x-api-key: $KEY" -H"Content-Type: application/json" -d "{\"scope\": \"$SCOPE\", \"expires_at\": \"$EXPIRY\", \"keytag_prefix\": \"$KEYTAG_PREFIX\"}" || echo $USAGE
