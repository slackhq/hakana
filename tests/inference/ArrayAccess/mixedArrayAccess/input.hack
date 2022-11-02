function foo($arr): mixed {
    return $arr["a"];
}

function foo($arr): mixed {
    if ($arr) {
        return $arr["a"];
    }
    return 0;
}

function bar(mixed $arr): mixed {
	return $arr["a"];
}