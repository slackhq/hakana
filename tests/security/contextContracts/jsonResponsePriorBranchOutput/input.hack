<<Hakana\SecurityAnalysis\Source('UriRequestHeader')>>
function input(): string { return ''; }
if (rand() > 0) { echo '<html>'; }
$json = json_encode(input());
header('Content-Type: application/json');
echo $json;
