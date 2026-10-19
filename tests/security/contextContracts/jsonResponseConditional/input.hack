<<Hakana\SecurityAnalysis\Source('UriRequestHeader')>>
function input(): string { return ''; }
$json = json_encode(input());
if (rand() > 0) { header('Content-Type: application/json'); }
echo $json;
