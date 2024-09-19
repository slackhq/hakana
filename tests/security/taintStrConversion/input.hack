function foo() : void {
    $a = strtoupper(strtolower((string) HH\global_get('_GET')["bad"]));
    echo $a;
}