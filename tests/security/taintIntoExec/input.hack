function foo() : void {
    $a = (string) $_GET["bad"];
    exec($a);
}