function foo($arr): mixed {
    return $arr["a"];
}

function bar($arr): mixed {
    if ($arr) {
        return $arr["a"];
    }
    return 0;
}

function baz($arr): mixed {
    if ($arr is nonnull) {
        return $arr["a"];
    }
    return 0;
}

function bak(dict $arr): mixed {
    $a = $arr['a'] ?? null;
    return $a['foo'] ?? null;
}

function bat(mixed $arr): mixed {
	return $arr["a"];
}