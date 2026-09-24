<<Hakana\SecurityAnalysis\Source('UriRequestHeader')>>
function input(): string { return ''; }
$json = json_encode(input());
rand() > 0 ? header('Content-Type: text/html') : header('Content-Type: application/json');
echo $json;
