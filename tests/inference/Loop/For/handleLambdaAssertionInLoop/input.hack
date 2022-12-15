function foo(): void {
    $data = vec[
    	shape('a' => 0),
        null,
        shape('a' => 0),
    ];

    for ($i = 0; $i < count($data); $i++) {
    	$case = $data[$i];
        (() ==> {
            if ($case is null) {}
        })();
    }
}