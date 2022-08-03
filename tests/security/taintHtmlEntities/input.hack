function foo() : void {
    $a = htmlentities((string) $_GET["bad"], \ENT_QUOTES);
    echo $a;
}