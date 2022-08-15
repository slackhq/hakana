function foo(string $s) {
    echo $s;
}

function bar(): void {
    $a = $_GET['a'];

    foo(
        /* HAKANA_SECURITY_IGNORE[HtmlTag] */
        $a
    );
}

function baz(): void {
    /* HAKANA_SECURITY_IGNORE[HtmlTag] */
    $a = $_GET['a'];

    foo($a);
}