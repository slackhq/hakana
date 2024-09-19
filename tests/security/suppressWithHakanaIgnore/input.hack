function foo(string $s) {
    echo $s;
}

function bar(): void {
    $a = HH\global_get('_GET')['a'];

    foo(
        /* HAKANA_SECURITY_IGNORE[HtmlTag] */
        $a
    );
}

function baz(): void {
    /* HAKANA_SECURITY_IGNORE[HtmlTag] */
    $a = HH\global_get('_GET')['a'];

    foo($a);
}