function foo(int $a): void {
    (() ==> {
        $a = $a + 1;
    })();

    echo $a;
}