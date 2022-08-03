function foo() : void {
    $a = "9" . "a" . "b" . "c" . ((string) $_GET["bad"]) . "d" . "e" . "f";
    exec($a);
}