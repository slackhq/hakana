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