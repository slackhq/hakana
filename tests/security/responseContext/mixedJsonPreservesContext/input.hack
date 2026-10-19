<<Hakana\SecurityAnalysis\Source('UriRequestHeader')>>
function source()[]: string { return ''; }
function show(mixed $data): void {
	header('Content-Type: application/json');
	echo json_encode(dict['taint' => source(), 'unknown' => $data]);
}
