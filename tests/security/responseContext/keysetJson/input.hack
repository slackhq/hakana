<<Hakana\SecurityAnalysis\Source('UriRequestHeader')>>
function source()[]: string { return ''; }
function run(keyset<string> $scopes): void {
	header('Content-Type: application/json');
	echo json_encode(shape('scopes' => $scopes, 'text' => source()));
}
