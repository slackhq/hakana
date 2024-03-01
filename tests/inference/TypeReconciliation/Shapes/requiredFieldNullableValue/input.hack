function foo(shape('bar' => string, 'baz' => ?string, ?'boo' => string) $s) {
    echo $s['bar'] ?? null;
    echo $s['baz'] ?? 'something';
    echo $s['baz'] ?? null;
    echo $s['boo'] ?? null;
}
