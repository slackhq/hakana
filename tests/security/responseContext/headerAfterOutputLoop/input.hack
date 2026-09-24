<<Hakana\SecurityAnalysis\Source('UriRequestHeader')>>
function source()[]: string { return ''; }
function run(vec<string> $items): void {
	foreach ($items as $item) {
		if ($item === '') {
			echo 'output commits headers';
			break;
		}
	}
	header('Content-Type: application/json');
	echo json_encode(source());
}
