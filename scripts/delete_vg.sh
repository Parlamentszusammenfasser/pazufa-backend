USAGE="<script> <prod|staging> <vorgang> <key>"

if [ "$#" -ne 3 ]; then
    echo "Usage: $USAGE"
    exit 1
fi

if [ $1 = "prod" ]; then
    BACKEND="https://api.pazufa.de"
elif [ $1 = "staging" ]; then
    BACKEND="https://staging.api.pazufa.de"
else
    echo "Unknown Backend, use either \`prod\` or \`staging\`"
    exit 1
fi

VORGANG=$2
KEY=$3

echo "Called with:"
echo "Key: $KEY"
echo "Vorgang: $VORGANG"
echo "Backend: $BACKEND"

curl -X DELETE $BACKEND/api/v2/vorgang/$VORGANG -H "x-api-key: $KEY" 
