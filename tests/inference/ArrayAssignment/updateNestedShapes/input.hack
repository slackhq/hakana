type foo_t = shape('a' => shape('b' => string));

function foo(foo_t $arr, string $s): shape('b' => string, 'c' => string) {
    $arr['a'] = $arr['a'];
    $arr['a']['c'] = $s;
    return $arr['a'];
}