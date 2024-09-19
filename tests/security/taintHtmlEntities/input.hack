function foo() : void {
    $a = htmlentities((string) HH\global_get('_GET')["bad"], \ENT_QUOTES);
    echo $a;
}