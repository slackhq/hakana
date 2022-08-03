function foo() : void {
    $a = strtoupper(strtolower((string) $_GET["bad"]));
    echo $a;
}