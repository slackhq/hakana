<<Hakana\SecurityAnalysis\Source('UriRequestHeader')>>
function source()[]: string { return ''; }
function run(vec<string> $items): void {
	$filtered = vec[];
	foreach ($items as $item) {
		if ($item === '') {
			continue;
		}
		$filtered[] = $item;
	}
	header('Content-Type: application/json');
	echo json_encode(shape('items' => $filtered, 'text' => source()));
}
