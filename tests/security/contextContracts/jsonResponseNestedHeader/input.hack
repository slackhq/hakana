<<Hakana\SecurityAnalysis\Source('UriRequestHeader')>>
function input(): string { return ''; }
function consume(mixed $v): void {}
$json = json_encode(input());
consume(header('Content-Type: application/json'));
echo $json;
