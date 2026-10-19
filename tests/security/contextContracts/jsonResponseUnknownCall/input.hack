<<Hakana\SecurityAnalysis\Source('UriRequestHeader')>>
function input(): string { return ''; }
function intervene(): void {}
$json = json_encode(input());
header('Content-Type: application/json');
intervene();
echo $json;
