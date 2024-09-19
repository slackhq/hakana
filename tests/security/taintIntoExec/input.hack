function foo() : void {
    $a = (string) HH\global_get('_GET')["bad"];
    exec($a);
}