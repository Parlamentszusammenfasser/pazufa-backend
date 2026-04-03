# This Script is used to compare two json entities. to be more precise it
# allows you to compare the output of a irl_scenario!() test of the backend.
diff --suppress-common-lines -y <(jq '.actual' $1 | jq 'walk(if type == "array" then sort else . end)')  \
     <(jq '.expected'  $1 | jq 'walk(if type == "array" then sort else . end)')
