<<Hakana\SecurityAnalysis\Source('UriRequestHeader')>>
function source()[]: string { return ''; }
function show(mixed $data): void {
$json = json_encode(dict['taint' => source(), 'unknown' => $data]);
header('Content-Type: application/json');
echo $json;
}
