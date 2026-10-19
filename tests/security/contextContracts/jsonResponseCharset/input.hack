<<Hakana\SecurityAnalysis\Source('UriRequestHeader')>>
function input(): string { return ''; }
$json = json_encode(input());
header('content-type: application/json; charset=utf-8');
echo $json;
