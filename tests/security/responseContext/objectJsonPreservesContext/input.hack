<<Hakana\SecurityAnalysis\Source('UriRequestHeader')>>
function source()[]: string { return ''; }
function show(JsonSerializable $data): void {
	header('Content-Type: application/json');
	echo json_encode(dict['taint' => source(), 'object' => $data]);
}
