function foo(int $a): dict<string, arraykey> {
    $arr = dict[];
    $arr["a"] = $a;
    $arr["b"] = "hello";
    return $arr;
}

function bar(dict<int, int> $arr): dict<arraykey, arraykey> {
    $arr["b"] = "hello";
    return $arr;
}