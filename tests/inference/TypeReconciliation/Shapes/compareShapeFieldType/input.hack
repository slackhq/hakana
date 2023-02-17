function foo(shape('bar' => string, ?'baz' => int) $s) {
    if ($s['bar'] == 5 || $s['bar'] == 7) {
        if (isset($s['baz'])) {}
    }
}