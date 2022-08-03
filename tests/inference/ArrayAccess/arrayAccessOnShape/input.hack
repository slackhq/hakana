type foo_t = shape("a" => int, "b" => string);

function foo(foo_t $arr): int {
    return $arr["a"];
}

function bar(foo_t $arr): void {
	foo($arr);
}