enum SomeField: string as string {
    A = 'a';
    B = 'b';
}

function foo(shape('bar' => SomeField, ...) $s) {
    if ($s is shape('bar' => string, 'baz' => int)) {
        echo $s['baz'];
    }
}