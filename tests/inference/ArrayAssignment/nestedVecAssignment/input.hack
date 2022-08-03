function foo(vec<string> $arr, string $b): dict<string, vec<string>> {
	$arr2 = dict[];
	foreach ($arr as $name) {
		$arr2["s:$name"] ??= vec[];
		$arr2["s:$name"][] = $b;

	}
	return $arr2;
}
