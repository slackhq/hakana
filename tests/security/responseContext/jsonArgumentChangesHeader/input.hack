<<Hakana\SecurityAnalysis\Source('UriRequestHeader')>>
function source()[]: string { return ''; }
function remove_content_type(mixed $data): mixed {
	header_remove('Content-Type');
	return $data;
}
function show(mixed $data): void {
	header('Content-Type: application/json');
	echo json_encode(dict['taint' => source(), 'value' => remove_content_type($data)]);
}
