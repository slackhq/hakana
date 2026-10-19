<<Hakana\SecurityAnalysis\Source('UriRequestHeader')>>
function source()[]: string { return ''; }
function show(mixed $data): void {
	$error = null;
	header('Content-Type: application/json');
	json_encode_with_error($data, inout $error);
	echo source();
}
