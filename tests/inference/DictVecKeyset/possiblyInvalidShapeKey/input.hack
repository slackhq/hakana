function foo(shape('a' => string) $arr): void {
}

function bar(shape('a' => string) $arr, int $i): void {
    if (rand(0, 1)) {
        $arr['a'] = $i;
    }
    foo($arr);
}