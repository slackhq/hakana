<<Hakana\SecurityAnalysis\Source('UriRequestHeader')>>
function input(): string { return ''; }
$json = json_encode(input());
header('Content-Type: text/html');
echo $json;
