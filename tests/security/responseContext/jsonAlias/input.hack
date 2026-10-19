<<Hakana\SecurityAnalysis\Source('UriRequestHeader')>>
function source()[]: string { return ''; }
type Payload = shape('text' => string, 'items' => vec<int>);
function run(Payload $data): void {
header('Content-Type: application/json');
echo json_encode(shape('data' => $data, 'taint' => source()));
}
